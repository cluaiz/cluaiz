#!/usr/bin/env bash
# Run 21 on the L4 VM: the planner release check on models the laptop did not load. Each step is
# skipped when its output exists, so a Spot restart continues where it stopped.
#   1 pull the models
#   2 per model: `cluaiz show` (the plan report) and `cluaiz bench --placements` (every placement
#     that fits, measured; the log line "VRAM planned X, taken Y" for each load)
#   3 Gemma 4 31B: the context it gets on the GPU (was 4096 only before the sliding-window fix)
#   4 step-down: another process takes VRAM while a model loads; the Engine must step down or
#     retry one placement smaller, never crash
#   5 multi-turn chat without a session id on a sliding-window model: later turns reuse the cache
# Usage: run21_planner.sh <path to the built cluaiz binary>
set -uo pipefail
BIN="$1"
HERE="$(cd "$(dirname "$0")" && pwd)"
OUT="$HOME/results-run21"; mkdir -p "$OUT"
export CLUAIZ_HOME="$HOME/ch-run21" CLUAIZ_PORT=8011 CLUAIZ_HOST=127.0.0.1 CLUAIZ_NONINTERACTIVE=1
step() { echo "== $(date +%H:%M:%S) $*" | tee -a "$OUT/steps.log"; }
# -L: a model folder may be a link to a copy pulled for an earlier run; drafter (MTP) and projector
# files are not the model
gguf() { find -L "$CLUAIZ_HOME/models" -iname "*$1*.gguf" ! -iname '*mmproj*' ! -iname 'mtp-*' | head -1; }

MODELS=(
  "llama-3.2-3b|bartowski/Llama-3.2-3B-Instruct-GGUF:Q4_K_M|Llama-3.2-3B"
  "gemma-3-12b|unsloth/gemma-3-12b-it-GGUF:Q4_K_M|gemma-3-12b"
  "mistral-24b|unsloth/Mistral-Small-3.2-24B-Instruct-2506-GGUF:Q4_K_M|Mistral-Small"
  "qwen3-30b-a3b|unsloth/Qwen3-30B-A3B-Instruct-2507-GGUF:Q4_K_M|Qwen3-30B-A3B"
  "gpt-oss-20b|ggml-org/gpt-oss-20b-GGUF:MXFP4|gpt-oss-20b"
  "gemma-4-31b|unsloth/gemma-4-31B-it-GGUF:Q4_K_M|gemma-4-31B"
)

"$BIN" --version | tee -a "$OUT/steps.log"
nvidia-smi --query-gpu=name,memory.total,memory.free --format=csv | tee -a "$OUT/steps.log"
free -g | tee -a "$OUT/steps.log"

step "1 pull"
for m in "${MODELS[@]}"; do
  IFS='|' read -r name id pattern <<< "$m"
  [ -n "$(gguf "$pattern")" ] && continue
  "$BIN" pull "$id" </dev/null > "$OUT/pull_$name.log" 2>&1
  echo "pull $name exit $? -> $(gguf "$pattern")" | tee -a "$OUT/steps.log"
done

step "2 show + bench --placements"
for m in "${MODELS[@]}"; do
  IFS='|' read -r name id pattern <<< "$m"
  file="$(gguf "$pattern")"
  [ -z "$file" ] && { echo "skip $name: not pulled" | tee -a "$OUT/steps.log"; continue; }
  [ -f "$OUT/show_$name.txt" ] || "$BIN" show "$file" > "$OUT/show_$name.txt" 2>&1
  if [ ! -f "$OUT/bench_$name.md" ]; then
    "$BIN" bench "$file" --placements -p 512 -n 128 -r 2 -v > "$OUT/bench_$name.md" 2> "$OUT/bench_$name.err"
    echo "bench $name exit $?" | tee -a "$OUT/steps.log"
  fi
  grep -h "VRAM planned" "$OUT/bench_$name.err" | sed "s/^/$name: /" | tee -a "$OUT/vram.log"
done

step "3 Gemma 4 31B context on the GPU"
grep -E "^PLAN|Context |KV cache|llama.cpp " "$OUT/show_gemma-4-31b.txt" | tee -a "$OUT/steps.log"

step "4 step-down under VRAM pressure"
if [ ! -f "$OUT/stepdown.log" ]; then
  file="$(gguf Qwen3-30B-A3B)"
  # Once the load has taken 1 GB, takes all but 1.5 GB of what is still free, for 180 s
  python3 "$HERE/vram_hog.py" load 1.5 180 > "$OUT/stepdown_hog.log" 2>&1 & HOG=$!
  "$BIN" bench "$file" -p 64 -n 16 -r 1 -v > "$OUT/stepdown_bench.md" 2> "$OUT/stepdown.log"
  echo "step-down bench exit $?" | tee -a "$OUT/steps.log"
  wait $HOG 2>/dev/null
  grep -E "VRAM planned|left after|failed to load|retry|smaller|GPU is full|kept as" "$OUT/stepdown.log" | tee -a "$OUT/steps.log"
fi

step "5 multi-turn cache without a session id"
if [ ! -f "$OUT/multiturn.log" ]; then
  "$BIN" serve > "$OUT/server.log" 2>&1 & SERVER=$!
  for i in $(seq 1 120); do curl -s -m 2 http://127.0.0.1:8011/health >/dev/null && break; sleep 1; done
  id="$("$BIN" list 2>/dev/null | awk '/gemma-3-12b/ {print $1; exit}')"
  python3 -u "$HERE/multiturn_cache.py" http://127.0.0.1:8011 "$CLUAIZ_HOME" "$id" > "$OUT/multiturn.log" 2>&1
  kill $SERVER; wait $SERVER 2>/dev/null
  cat "$OUT/multiturn.log" | tee -a "$OUT/steps.log"
fi
step "6 architecture sweep: one load per architecture, VRAM planned against taken"
# Small models of each KV layout the planner sums differently: plain attention, sliding window
# (listed or default pattern), shared KV, hybrid recurrent/linear layers, MLA, MoE. Each model is
# deleted after its load to keep the disk free; the run's own models are deleted first (steps 2-5 are
# done with them, and Kimi-Linear 48B needs the room).
rm -rf "$CLUAIZ_HOME"/models/chat/{llama-3.2-3b,gemma-3-12b,mistral-small,gemma-4-31b}*
ARCH_MODELS=(
  "llama|bartowski/Llama-3.2-1B-Instruct-GGUF:Q4_K_M|Llama-3.2-1B"
  "gemma2|bartowski/gemma-2-2b-it-GGUF:Q4_K_M|gemma-2-2b"
  "gemma3|unsloth/gemma-3-1b-it-GGUF:Q4_K_M|gemma-3-1b"
  "gemma3n|unsloth/gemma-3n-E2B-it-GGUF:Q4_K_M|gemma-3n-E2B"
  "qwen3|unsloth/Qwen3-1.7B-GGUF:Q4_K_M|Qwen3-1.7B"
  "qwen35|unsloth/Qwen3.5-2B-GGUF:Q4_K_M|Qwen3.5-2B"
  "phi3|unsloth/Phi-4-mini-instruct-GGUF:Q4_K_M|Phi-4-mini"
  "mistral3|unsloth/Ministral-3-3B-Instruct-2512-GGUF:Q4_K_M|Ministral-3-3B"
  "olmo2|unsloth/OLMo-2-0425-1B-Instruct-GGUF:Q4_K_M|OLMo-2-0425-1B"
  "granitehybrid|ibm-granite/granite-4.0-h-tiny-GGUF:Q4_K_M|granite-4.0-h-tiny"
  "lfm2|LiquidAI/LFM2-1.2B-GGUF:Q4_K_M|LFM2-1.2B-Q4"
  "deepseek2|bartowski/DeepSeek-Coder-V2-Lite-Instruct-GGUF:Q4_K_M|DeepSeek-Coder-V2-Lite"
  "kimi-vl|ggml-org/Kimi-VL-A3B-Thinking-2506-GGUF:Q4_K_M|Kimi-VL-A3B"
  "moonlight|gabriellarson/Moonlight-16B-A3B-Instruct-GGUF:Q4_K_M|Moonlight-16B"
  "glm47flash|unsloth/GLM-4.7-Flash-GGUF:Q4_K_M|GLM-4.7-Flash"
  "kimi-linear|bartowski/moonshotai_Kimi-Linear-48B-A3B-Instruct-GGUF:Q4_K_M|Kimi-Linear-48B"
)
for m in "${ARCH_MODELS[@]}"; do
  IFS='|' read -r name id pattern <<< "$m"
  [ -f "$OUT/arch_$name.err" ] && continue
  "$BIN" pull "$id" </dev/null > "$OUT/arch_pull_$name.log" 2>&1
  file="$(gguf "$pattern")"
  [ -z "$file" ] && { echo "arch $name: not pulled" | tee -a "$OUT/arch.log"; continue; }
  "$BIN" show "$file" > "$OUT/arch_show_$name.txt" 2>&1
  "$BIN" bench "$file" -p 128 -n 32 -r 1 -v > "$OUT/arch_bench_$name.md" 2> "$OUT/arch_$name.err"
  code=$?
  line="$(grep -h "VRAM planned" "$OUT/arch_$name.err" | tail -1 | sed 's/.*VRAM planned/VRAM planned/')"
  echo "arch $name exit $code | $(grep -m1 '^PLAN' "$OUT/arch_show_$name.txt") | $line" | tee -a "$OUT/arch.log"
  rm -rf "$(dirname "$file")"
done
step "all done"
