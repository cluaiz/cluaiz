#!/usr/bin/env bash
# Run 14: Cluaiz against official llama.cpp (ggml-org) with the same settings and the same method, in every
# placement mode. Both sides use their project's bench tool: llama.cpp's llama-bench and `cluaiz bench`.
#
# Modes, per model:
#   auto   Cluaiz's planner picks; llama-bench gets exactly the placement and KV type the plan printed
#   gpu    every block on the GPU (-ngl 99 / --n-gpu-layers all), F16 KV
#   split  half of the blocks on the GPU (-ngl half+1 / --n-gpu-layers half+1), F16 KV
#   cpu    -ngl 0 / --n-gpu-layers 0, F16 KV
#   ssd    gpt-oss-120b (59 GiB > 31 GB RAM): the planner's placement, page cache dropped before the
#          first run of each tool, one cold run, then 3 runs
# Every run: prompt 512, generation 128, 4 threads, flash attention auto, 3 repetitions.
# Usage: ssd_same_settings.sh   (results in ~/results-run14)
set -uo pipefail
OUT="$HOME/results-run14"; mkdir -p "$OUT"
SRC="$HOME/llama.cpp-official"
BENCH="$SRC/build/bin/llama-bench"
CLUAIZ="$HOME/.cluaiz/bin/cluaiz"
export CLUAIZ_HOME="$HOME/ch-reg"
MODELS="$CLUAIZ_HOME/models/chat"
step() { echo "== $(date +%H:%M:%S) $*" | tee -a "$OUT/steps.log"; }
drop() { sync; echo 3 | sudo tee /proc/sys/vm/drop_caches > /dev/null; }
gguf() { ls "$MODELS/$1"/*.gguf | grep -viE "mmproj|mtp" | head -1; }

step "build official llama-bench"
[ -x "$BENCH" ] || (cd "$SRC" && export PATH="/usr/local/cuda/bin:$PATH" && cmake --build build -j "$(nproc)" --target llama-bench) > "$OUT/build.log" 2>&1
[ -x "$BENCH" ] || { step "llama-bench build failed"; exit 1; }
git -C "$SRC" log -1 --format="official llama.cpp %h %ad" --date=short | tee -a "$OUT/steps.log"
"$CLUAIZ" --version 2>&1 | head -1 | tee -a "$OUT/steps.log"
COMMON=(-t 4 -p 512 -n 128)

# pair <model id> <mode> <reps> <cold 0|1> <cluaiz placement args...> -- <llama-bench placement args...>
pair() {
  local id="$1" mode="$2" reps="$3" cold="$4"; shift 4
  local cz=() lb=()
  while [ "$1" != "--" ]; do cz+=("$1"); shift; done; shift; lb=("$@")
  local tag="${id}_${mode}"
  [ -f "$OUT/$tag.done" ] && return
  step "$tag llama.cpp: ${lb[*]}"
  if [ "$cold" = 1 ]; then drop; "$BENCH" -m "$(gguf "$id")" "${lb[@]}" "${COMMON[@]}" -fa auto -r 1 --no-warmup -o md > "$OUT/${tag}_llama_cold.md" 2>> "$OUT/${tag}_llama.log"; fi
  "$BENCH" -m "$(gguf "$id")" "${lb[@]}" "${COMMON[@]}" -fa auto -r "$reps" -o md > "$OUT/${tag}_llama.md" 2>> "$OUT/${tag}_llama.log"
  step "$tag Cluaiz: ${cz[*]}"
  if [ "$cold" = 1 ]; then drop; "$CLUAIZ" bench "$id" "${cz[@]}" "${COMMON[@]}" --flash-attn auto -r 1 -v > "$OUT/${tag}_cluaiz_cold.md" 2>> "$OUT/${tag}_cluaiz.log"; fi
  "$CLUAIZ" bench "$id" "${cz[@]}" "${COMMON[@]}" --flash-attn auto -r "$reps" -v > "$OUT/${tag}_cluaiz.md" 2>> "$OUT/${tag}_cluaiz.log"
  touch "$OUT/$tag.done"
}

# The auto plan of a model: n_gpu_layers, n_cpu_moe, KV type and block count, read from `cluaiz bench -v`
plan_of() {
  "$CLUAIZ" bench "$1" -p 0 -n 8 -r 1 -v 2>&1 | python3 -c '
import re, sys
t = sys.stdin.read()
m = re.search(r"n_gpu_layers = (-?\d+) \| n_cpu_moe = (\d+) \| n_ctx = (\d+) \| KV (Kv\w+)", t)
b = re.search(r"Blocks\s*: \d+ on GPU, \d+ in RAM \(of (\d+)\)", t)
kv = {"Kv16": "f16", "Kv8": "q8_0", "Kv4": "q4_0"}
print(m.group(1), m.group(2), m.group(3), kv.get(m.group(4), "f16"), b.group(1)) if m and b else print("")'
}

for id in phi-4-mini-instruct-gguf-Q4_K_M qwen3.5-9b-gguf-Q4_K_M qwen3-30b-a3b-instruct-2507-gguf-Q4_K_M gpt-oss-20b-gguf-FP4; do
  [ -n "$(gguf "$id" 2>/dev/null)" ] || { step "$id missing"; continue; }
  read -r ngl ncmoe nctx kv blocks <<< "$(plan_of "$id")"
  [ -n "${blocks:-}" ] || { step "$id: no plan read"; continue; }
  echo "$id auto plan: ngl $ngl ncmoe $ncmoe ctx $nctx kv $kv blocks $blocks" | tee -a "$OUT/steps.log"
  lb_ngl=$ngl; [ "$ngl" -lt 0 ] && lb_ngl=99
  half=$(( blocks / 2 + 1 ))
  pair "$id" auto 3 0 -- -ngl "$lb_ngl" -ncmoe "$ncmoe" -ctk "$kv" -ctv "$kv"
  pair "$id" gpu 3 0 --n-gpu-layers all -c 4096 --cache-type-k f16 --cache-type-v f16 -- -ngl 99 -ctk f16 -ctv f16
  pair "$id" split 3 0 --n-gpu-layers "$half" -c 4096 --cache-type-k f16 --cache-type-v f16 -- -ngl "$half" -ctk f16 -ctv f16
  pair "$id" cpu 3 0 --n-gpu-layers 0 -c 4096 --cache-type-k f16 --cache-type-v f16 -- -ngl 0 -ctk f16 -ctv f16
done

id=gpt-oss-120b-gguf-FP4
read -r ngl ncmoe nctx kv blocks <<< "$(plan_of "$id")"
echo "$id auto plan: ngl $ngl ncmoe $ncmoe ctx $nctx kv $kv blocks $blocks" | tee -a "$OUT/steps.log"
if [ -n "${blocks:-}" ]; then
  lb_ngl=$ngl; [ "$ngl" -lt 0 ] && lb_ngl=99
  pair "$id" ssd 3 1 -- -ngl "$lb_ngl" -ncmoe "$ncmoe" -ctk "$kv" -ctv "$kv"
fi

for f in "$OUT"/*.md; do [ "$f" = "$OUT/results.md" ] && continue; echo "### $(basename "$f" .md)"; grep -E "^\|" "$f"; done > "$OUT/results.md"
step "all done"
