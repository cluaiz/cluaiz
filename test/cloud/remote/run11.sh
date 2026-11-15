#!/usr/bin/env bash
# Run 11 on the L4 VM: the republished v0.1.0 (llama.cpp update + VRAM reserve change) with models the
# earlier runs did not use. Steps, each skipped when its output exists (a rerun continues):
#   1 install the release, pull the models          4 SSD streaming (gpt-oss 120B > RAM + VRAM)
#   2 placements auto/gpu/split/cpu + llama-bench    5 Cluaiz, Ollama and LM Studio on the same files
#   3 features (tools, thinking, vision, speech)     6 the reserve change on Gemma 4 31B (auto placement)
# Usage: run11.sh <cluaiz commit of the release>
set -uo pipefail
COMMIT="$1"
HERE="$(cd "$(dirname "$0")" && pwd)"
OUT="$HOME/results-run11"; mkdir -p "$OUT"
BIN="$HOME/.cluaiz/bin/cluaiz"
export CLUAIZ_HOME="$HOME/ch-reg" CLUAIZ_PORT=8011 CLUAIZ_HOST=127.0.0.1 CLUAIZ_NONINTERACTIVE=1
step() { echo "== $(date +%H:%M:%S) $*" | tee -a "$OUT/steps.log"; }

step "1 install"
if ! "$BIN" --version 2>/dev/null | grep -q 0.1.0 || [ ! -f "$OUT/installed" ]; then
  curl -fsSL https://raw.githubusercontent.com/cluaiz/cluaiz/main/install.sh | sh > "$OUT/install.log" 2>&1
  touch "$OUT/installed"
fi
"$BIN" --version | tee -a "$OUT/steps.log"
for id in unsloth/Qwen3-30B-A3B-Instruct-2507-GGUF:Q4_K_M ggml-org/gpt-oss-20b-GGUF:MXFP4 unsloth/Qwen3.5-9B-GGUF:Q4_K_M \
          unsloth/Phi-4-mini-instruct-GGUF:Q4_K_M ggml-org/embeddinggemma-300M-GGUF onnx-community/moonshine-tiny-ONNX:int8 \
          onnx-community/Kokoro-82M-v1.0-ONNX:uint8 ggml-org/Qwen3-TTS-12Hz-1.7B-Base-GGUF:Q4_K_M \
          ggml-org/gpt-oss-120b-GGUF:MXFP4 unsloth/gemma-4-31B-it-GGUF:Q4_K_M; do
  log="$OUT/pull_$(echo "$id" | tr '/:' '__').log"
  [ -f "$log.ok" ] && continue
  start=$(date +%s); "$BIN" pull "$id" </dev/null > "$log" 2>&1 && touch "$log.ok"
  echo "$id exit $? $(( $(date +%s) - start ))s" | tee -a "$OUT/steps.log"
done
df -h / | tee -a "$OUT/steps.log"

step "2 placements"
if [ ! -f "$HOME/results-regression/RESULTS_l4r11_qwen3-30b-a3b.md" ]; then  # regression_all skips finished models
  "$HERE/regression_all.sh" "$COMMIT" 89 "$HERE/models_run11.txt" l4r11 \
    "phi-4-mini,qwen3.5-9b,gpt-oss-20b,qwen3-30b-a3b" --modes auto,gpu,split,cpu --runs 2 --skip 120b \
    > "$OUT/placements.log" 2>&1
fi

step "3 features"
if [ ! -f "$OUT/features/results.json" ]; then
  "$BIN" serve > "$OUT/server_features.log" 2>&1 & SERVER=$!
  for i in $(seq 1 120); do curl -s -m 2 http://127.0.0.1:8011/health >/dev/null && break; sleep 1; done
  python3 -u "$HERE/features11.py" http://127.0.0.1:8011 "$CLUAIZ_HOME" "$OUT/features" > "$OUT/features.log" 2>&1
  kill $SERVER; wait $SERVER 2>/dev/null
fi

step "4 SSD streaming"
if [ ! -f "$HOME/results-regression/ssd_gpt-oss-120b.log" ]; then
  id=$(ls "$CLUAIZ_HOME/models/chat" | grep -i "gpt-oss-120b" | head -1)
  [ -n "$id" ] && "$HERE/ssd_test.sh" "$id" gpt-oss-120b 8 > "$OUT/ssd.log" 2>&1
fi
# the 148 GB disk cannot hold the 120B (61 GB) and Ollama's copies of the compare models together
dir="$CLUAIZ_HOME/models/chat/gpt-oss-120b-gguf-FP4"
if [ -f "$HOME/results-regression/ssd_gpt-oss-120b.log" ] && [ -d "$dir" ] && [ ! -L "$dir" ]; then
  rm -rf "$dir"; df -h / | tee -a "$OUT/steps.log"
fi

step "5 compare"
if [ ! -f "$OUT/compare/results.md" ]; then
  command -v ollama >/dev/null || curl -fsSL https://ollama.com/install.sh | sh > "$OUT/ollama_install.log" 2>&1
  [ -x "$HOME/.lmstudio/bin/lms" ] || curl -fsSL https://lmstudio.ai/install.sh | bash > "$OUT/lms_install.log" 2>&1
  ids=$(ls "$CLUAIZ_HOME/models/chat" | grep -iE "qwen3-30b-a3b|gpt-oss-20b|phi-4-mini" | tr '\n' ' ')
  python3 -u "$HERE/compare3.py" "$OUT/compare" 16384 $ids > "$OUT/compare.log" 2>&1
fi

step "6 reserve on Gemma 4 31B"
if [ ! -f "$OUT/gemma31b_auto.json" ]; then
  id=$(ls "$CLUAIZ_HOME/models/chat" | grep -i "gemma-4-31b" | grep -i q4 | head -1)
  if [ -n "$id" ]; then
    "$BIN" serve > "$OUT/server_gemma31b.log" 2>&1 & SERVER=$!
    for i in $(seq 1 120); do curl -s -m 2 http://127.0.0.1:8011/health >/dev/null && break; sleep 1; done
    TOKEN=$(cat "$CLUAIZ_HOME/session.token")
    for run in 1 2 3; do
      curl -s -m 1800 http://127.0.0.1:8011/v1/chat/completions -H "Authorization: Bearer $TOKEN" -H "Content-Type: application/json" \
        -d "{\"model\":\"$id\",\"max_tokens\":64,\"think_mode\":\"off\",\"temporary_chat\":\"strict\",\"messages\":[{\"role\":\"user\",\"content\":\"Note $RANDOM. Write two sentences about rivers.\"}]}" \
        | python3 -c 'import json,sys; d=json.load(sys.stdin); e=d.get("engine",{}); l=e.get("load",{}); t=e.get("timings",{}); print(json.dumps({"placement":l.get("placement"),"n_gpu_layers":l.get("n_gpu_layers"),"n_ctx":l.get("n_ctx"),"gen_tps":t.get("predicted_per_second"),"prompt_tps":t.get("prompt_per_second")}))' \
        >> "$OUT/gemma31b_auto.jsonl"
      nvidia-smi --query-gpu=memory.used,memory.total --format=csv,noheader >> "$OUT/gemma31b_auto.jsonl"
    done
    cp "$OUT/gemma31b_auto.jsonl" "$OUT/gemma31b_auto.json"
    kill $SERVER; wait $SERVER 2>/dev/null
  fi
fi
step "all done"
