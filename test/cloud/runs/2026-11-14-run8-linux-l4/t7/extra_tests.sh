#!/usr/bin/env bash
# Extra checks after release_tests.sh and feature_tests.sh: flows with the permission confirmed,
# bench of every chat model, the speech outputs saved as files, and (GPU) a model bigger than VRAM.
# Results in ~/t7/extra/, one line per step in summary.tsv. Usage: extra_tests.sh [big]
set -uo pipefail
B="$HOME/.cluaiz/bin/cluaiz"
OUT="$HOME/t7/extra"; mkdir -p "$OUT"
export CLUAIZ_HOME="$HOME/ch" CLUAIZ_PORT=8012 CLUAIZ_HOST=127.0.0.1 CLUAIZ_NONINTERACTIVE=1
SUM="$OUT/summary.tsv"
[ -f "$SUM" ] || printf 'step\texit\tseconds\n' > "$SUM"
step() {
  local name="$1"; shift
  grep -q "^$name	" "$SUM" && return 0
  local start=$(date +%s)
  echo "== $name" | tee "$OUT/$name.log"
  echo "\$ $*" >> "$OUT/$name.log"
  timeout "${STEP_TIMEOUT:-1800}" bash -c "$*" >> "$OUT/$name.log" 2>&1 < /dev/null
  local code=$?
  printf '%s\t%s\t%s\n' "$name" "$code" "$(( $(date +%s) - start ))" | tee -a "$SUM"
}

step model_set_chat "$B model set-chat qwen3-4b-gguf-Q4_K_M"
for id in qwen3-4b-gguf-Q4_K_M gemma-3-12b-it-gguf-Q4_K_M mistral-small-3.2-24b-instruct-2506-gguf-Q4_K_M; do
  step "bench_$id" "$B bench $id -r 2"
done
if [ "${1:-}" = big ]; then
  STEP_TIMEOUT=3600 step pull_gemma4_31b "$B pull unsloth/gemma-4-31B-it-GGUF:Q4_K_M"
  step show_gemma4_31b  "$B show unsloth/gemma-4-31B-it-GGUF:Q4_K_M"
  STEP_TIMEOUT=3600 step bench_gemma4_31b "$B bench unsloth/gemma-4-31B-it-GGUF:Q4_K_M -r 1"
  STEP_TIMEOUT=1800 step run_gemma4_31b "$B run unsloth/gemma-4-31B-it-GGUF:Q4_K_M 'Name three rivers in India, one line.' --local -n 64"
fi

"$B" serve > "$OUT/server.log" 2>&1 &
SERVER=$!
for i in $(seq 1 180); do curl -s -m 2 http://127.0.0.1:8012/health >/dev/null && break; sleep 1; done
TOKEN=$(cat "$CLUAIZ_HOME/session.token")
export OUT TOKEN
speech() {  # speech <model> <file> <text>
  curl -s -m 600 http://127.0.0.1:8012/v1/audio/speech -H "authorization: Bearer $TOKEN" -H 'content-type: application/json' \
    -d "{\"model\":\"$1\",\"input\":\"$3\"}" | python3 -c "
import sys, json, base64
d = json.load(sys.stdin); uri = ((d.get('output') or {}).get('audio_data') or '')
open('$OUT/$2', 'wb').write(base64.b64decode(uri.split(',', 1)[1])) if ',' in uri else sys.exit('no audio: ' + json.dumps(d)[:400])
print('$2', 'saved')"
}
step tts_kokoro_en   "$(declare -f speech); speech kokoro-82m-v1.0-onnx-UINT8 kokoro_en.wav 'Cluaiz runs language, speech and vision models on your own machine.'"
step tts_kokoro_hi   "$(declare -f speech); speech kokoro-82m-v1.0-onnx-UINT8 kokoro_hi.wav 'Namaste, yeh Cluaiz ka awaaz test hai.'"
step tts_qwen3_en    "$(declare -f speech); speech qwen3-tts-12hz-1.7b-base-gguf-Q4_K_M qwen3tts_en.wav 'Cluaiz runs language, speech and vision models on your own machine.'"
step stt_kokoro_en   "curl -s -m 600 http://127.0.0.1:8012/v1/audio/transcriptions -H 'authorization: Bearer $TOKEN' -H 'content-type: application/json' -d '{\"model\":\"whisper-large-v3-turbo-INT8\",\"file\":\"$OUT/kokoro_en.wav\"}'"
step stt_qwen3_en    "curl -s -m 600 http://127.0.0.1:8012/v1/audio/transcriptions -H 'authorization: Bearer $TOKEN' -H 'content-type: application/json' -d '{\"model\":\"whisper-large-v3-turbo-INT8\",\"file\":\"$OUT/qwen3tts_en.wav\"}'"
step flow_save   "printf 'y\n' | $B flow save $HOME/t7/features/hello.flow.yaml --overwrite"
step flow_run    "$B flow run vm-hello"
step flow_logs   "$B flow logs vm-hello"
step flow_explain "$B flow explain vm-hello"
step flow_remove "printf 'y\n' | $B flow remove vm-hello"
mkdir -p "$OUT/agent-work" && printf 'a\n' > "$OUT/agent-work/one.txt" && printf 'b\n' > "$OUT/agent-work/two.txt"
step agent_task  "cd $OUT/agent-work && $B agent 'List the files in this folder and say how many there are.'"
kill "$SERVER" 2>/dev/null; wait "$SERVER" 2>/dev/null
command -v nvidia-smi >/dev/null && nvidia-smi > "$OUT/nvidia-smi.txt" 2>&1
echo "== extra done" | tee "$OUT/done.txt"
