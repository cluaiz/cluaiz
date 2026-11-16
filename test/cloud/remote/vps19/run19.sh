#!/usr/bin/env bash
# Run 19: deep test of release v0.1.0 on a 1 vCPU / 3.8 GB VPS without a GPU, with a resource log of the whole run
# (sampler.py: CPU and memory of the machine and of Cluaiz, every second, tagged with the phase).
#   A  install size, idle server
#   B  model matrix: which GGUF chat models run on the CPU, their plan, `cluaiz bench` speed and memory
#   C  the server with each chat model: time to first token, generation speed, memory
#   D  speech (ONNX): Kokoro TTS real-time factor; moonshine and whisper STT speed and word recall
#   E  embeddings throughput; document ingest
#   F  a cloud provider (Groq, free key) through Cluaiz: latency and tokens
#   G  automation: a flow with a local model step, a provider step with a JSON schema and a file write
#   H  coding agent: a provider model and a local model build calc.py with tests
#   I  load: 1-8 requests at once; everything at once for 3 minutes
#   J  a model larger than RAM (Qwen3-8B, 5 GB)
# The Cluaiz processes run in a 2.9 GB memory scope so the website container on this VPS keeps its memory.
# Usage: run19.sh <dir with these scripts>   (results in ~/results-run19; a Groq key in <dir>/.groq is optional)
set -uo pipefail
HERE="$1"; OUT="$HOME/results-run19"; mkdir -p "$OUT"; rm -f "$OUT/stop"
export CLUAIZ_PORT=8011 CLUAIZ_HOST=127.0.0.1 CLUAIZ_NONINTERACTIVE=1
BIN="$HOME/.cluaiz/bin/cluaiz"; B=http://127.0.0.1:8011; MODELS="$HOME/.cluaiz/models"
step() { echo "== $(date +%H:%M:%S) $*" | tee -a "$OUT/steps.log"; echo "$*" | tr ',' ';' > "$OUT/phase"; }
scoped() { systemd-run --scope --quiet -p MemoryMax=2900M -p MemorySwapMax=0 "$@"; }
WL() { python3 -u "$HERE/workloads.py" "$B" "$TOKEN" "$OUT" "$@" >> "$OUT/workloads.log" 2>&1; }
idof() { ls "$MODELS/chat" 2>/dev/null | grep -i -- "$1" | head -1; }
mid() { curl -s -H "authorization: Bearer $TOKEN" "$B/v1/models" | python3 -c 'import json,sys; n=sys.argv[1].lower(); print(next((m["id"] for m in json.load(sys.stdin)["data"] if n in m["id"].lower()), ""))' "$1"; }
# The PID that listens on port 8011 (systemd-run --scope does not always exec, so its own PID is not the server)
port_pid() { ss -ltnpH "sport = :8011" | grep -oE 'pid=[0-9]+' | head -1 | cut -d= -f2; }
stop_server() {
  local p; p=$(port_pid)
  if [ -n "$p" ]; then kill "$p"; for _ in $(seq 1 30); do kill -0 "$p" 2> /dev/null || break; sleep 1; done; fi
}
start_server() {
  stop_server
  scoped env GROQ_API_KEY="${GROQ_API_KEY:-}" "$BIN" serve >> "$OUT/server.log" 2>&1 &
  for _ in $(seq 1 120); do curl -s -m 2 "$B/health" > /dev/null && break; sleep 1; done
  echo "server pid $(port_pid)" | tee -a "$OUT/steps.log"
  TOKEN=$(cat "$HOME/.cluaiz/session.token")
}
[ -f "$HERE/.groq" ] && GROQ_API_KEY="$(head -1 "$HERE/.groq" | tr -d '\r\n" ')" && export GROQ_API_KEY

python3 -u "$HERE/sampler.py" "$OUT" > "$OUT/sampler.log" 2>&1 & SAMPLER=$!

step "A baseline, no Cluaiz"; sleep 30
"$BIN" --version | tee -a "$OUT/steps.log"
"$BIN" status > "$OUT/status.txt" 2>&1
du -sh "$HOME/.cluaiz/bin" "$HOME/.cluaiz/lib" 2> /dev/null | tee -a "$OUT/steps.log"

step "B pull chat models"
for pair in "unsloth/Qwen3-0.6B-GGUF:Q4_K_M" "bartowski/Llama-3.2-1B-Instruct-GGUF:Q4_K_M" "unsloth/gemma-3-1b-it-GGUF:Q4_K_M" \
            "unsloth/Qwen3-1.7B-GGUF:Q4_K_M" "unsloth/Qwen3-4B-GGUF:Q4_K_M" "unsloth/Phi-4-mini-instruct-GGUF:Q4_K_M" \
            "unsloth/Qwen3-8B-GGUF:Q4_K_M" "onnx-community/moonshine-base-ONNX:int8" "onnx-community/whisper-base:int8" \
            "onnx-community/Kokoro-82M-v1.0-ONNX:uint8" "onnx-community/moonshine-tiny-ONNX:int8" "ggml-org/embeddinggemma-300M-GGUF"; do
  log="$OUT/pull_$(echo "$pair" | tr '/:' '__').log"; t=$(date +%s)
  "$BIN" pull "$pair" < /dev/null > "$log" 2>&1; echo "pull $pair exit $? $(( $(date +%s) - t ))s" | tee -a "$OUT/steps.log"
done
du -sh "$MODELS"/*/* 2> /dev/null | tee "$OUT/model_sizes.txt"; df -h / | tail -1 | tee -a "$OUT/steps.log"
CHAT_IDS=$(ls "$MODELS/chat")

for id in $CHAT_IDS; do
  case "$id" in *8b*) continue;; esac
  step "B bench $id"
  "$BIN" show "$id" > "$OUT/show_$id.txt" 2>&1
  scoped timeout 1500 "$BIN" bench "$id" -p 512 -n 128 -r 2 -v > "$OUT/bench_$id.md" 2> "$OUT/bench_$id.log"
  echo "bench $id exit $?: $(grep -hE 'pp512|tg128' "$OUT/bench_$id.md" | awk -F'|' '{printf "%s %s; ", $8, $9}')" | tee -a "$OUT/steps.log"
done

step "A server idle"; start_server; sleep 60
curl -s -H "authorization: Bearer $TOKEN" "$B/v1/models" > "$OUT/models.json"

for id in $CHAT_IDS; do
  case "$id" in *8b*) continue;; esac
  step "C chat $id"; WL chat "$id" 3
done

step "D speech"
WL speech "$(mid kokoro)" $(mid moonshine-tiny) $(mid moonshine-base) $(mid whisper)

step "E embeddings"; WL embed "$(mid embeddinggemma)"
step "E ingest"
cp /usr/share/common-licenses/GPL-3 "$OUT/doc_gpl3.txt" 2> /dev/null
printf '# Rivers\n\nRivers carry water from the mountains to the sea. The Ganga is 2,525 km long.\n\n## Lakes\n\nLake Baikal is the deepest lake on Earth.\n' > "$OUT/doc_rivers.md"
for f in "$OUT/doc_rivers.md" "$OUT/doc_gpl3.txt"; do
  t=$(date +%s%N); "$BIN" ingest "$f" > "$OUT/ingest_$(basename "$f").log" 2>&1; code=$?
  echo "ingest $(basename "$f") $(wc -c < "$f") bytes exit $code $(( ($(date +%s%N) - t) / 1000000 )) ms" | tee -a "$OUT/steps.log"
done

PROVIDER_MODEL=""
if [ -n "${GROQ_API_KEY:-}" ]; then
  step "F provider"
  stop_server
  "$BIN" provider add groq --url https://api.groq.com/openai/v1 --key-env GROQ_API_KEY > "$OUT/provider_add.log" 2>&1
  "$BIN" provider models groq > "$OUT/provider_models.txt" 2>&1
  for want in openai/gpt-oss-20b llama-3.1-8b-instant llama-3.3-70b-versatile; do
    grep -q "$want" "$OUT/provider_models.txt" && PROVIDER_MODEL="groq/$want" && break
  done
  echo "provider model: ${PROVIDER_MODEL:-none}" | tee -a "$OUT/steps.log"
  start_server
  [ -n "$PROVIDER_MODEL" ] && "$BIN" provider test groq > "$OUT/provider_test.log" 2>&1 && WL provider "$PROVIDER_MODEL"
fi

step "G flow"
LOCAL=$(idof llama-3.2-3b)
PSTEP=""
if [ -n "$PROVIDER_MODEL" ]; then
  PSTEP="  - id: facts
    ask: \"Give the river's name and its length in km.\"
    model: $PROVIDER_MODEL
    output_schema: { type: object, properties: { river: { type: string }, km: { type: number } }, required: [river, km] }
    with: { text: \"The Ganga is 2,525 km long.\" }"
fi
{
  echo "flow: cluaiz.flow/1"
  echo "name: vps-notes"
  echo "steps:"
  echo "  - id: summary"
  echo "    ask: \"Summarise this text in one sentence.\""
  echo "    model: $LOCAL"
  echo "    think: \"off\""
  echo "    max_tokens: 60"
  echo "    with: { text: \"Rivers carry water from the mountains to the sea. The Ganga is 2,525 km long and flows into the Bay of Bengal.\" }"
  [ -n "$PSTEP" ] && echo "$PSTEP"
  echo "  - call: fs.write"
  echo "    with: { path: \"$OUT/flow-out.md\", text: \"\${{ steps.summary.output }}\" }"
} > "$OUT/vps-notes.flow.yaml"
"$BIN" flow validate "$OUT/vps-notes.flow.yaml" > "$OUT/flow_validate.log" 2>&1; echo "flow validate exit $?" | tee -a "$OUT/steps.log"
yes | "$BIN" flow save "$OUT/vps-notes.flow.yaml" --overwrite > "$OUT/flow_save.log" 2>&1; echo "flow save exit $?" | tee -a "$OUT/steps.log"
t=$(date +%s); "$BIN" flow run vps-notes > "$OUT/flow_run.log" 2>&1; echo "flow run exit $? $(( $(date +%s) - t ))s" | tee -a "$OUT/steps.log"
"$BIN" flow logs vps-notes > "$OUT/flow_logs.log" 2>&1
cat "$OUT/flow-out.md" 2> /dev/null | head -3 | tee -a "$OUT/steps.log"

TASK="Create calc.py with functions add, sub, mul and div (div raises ZeroDivisionError when dividing by zero), and test_calc.py with unittest tests for all four. Run python3 -m unittest and make every test pass."
agent() { # label model
  local W="$OUT/agent-$1"; rm -rf "$W"; mkdir -p "$W"; local t=$(date +%s)
  timeout 2400 "$BIN" agent -C "$W" -m "$2" --mode full --ui plain --output-format json -p "$TASK" > "$OUT/agent_$1.log" 2>&1
  local code=$?; local tests; tests=$(cd "$W" && timeout 60 python3 -m unittest 2>&1 | tail -1)
  echo "agent $1 ($2) exit $code $(( $(date +%s) - t ))s; files: $(ls "$W" | tr '\n' ' '); our unittest run: $tests" | tee -a "$OUT/steps.log"
}
[ -n "$PROVIDER_MODEL" ] && { step "H agent provider"; agent provider "$PROVIDER_MODEL"; }
step "H agent local"; agent local "$LOCAL"

SMALL=$(idof qwen3-0.6b)
step "I concurrency $SMALL"; WL concurrency "$SMALL" 1,2,4,8
step "I concurrency $LOCAL"; WL concurrency "$LOCAL" 1,2,4
step "I mixed load"; WL mixed 180 "$SMALL" "$(mid embeddinggemma)" "$(mid kokoro)" "$(mid moonshine-tiny)" $PROVIDER_MODEL

BIG=$(idof qwen3-8b)
if [ -n "$BIG" ]; then
  step "J larger than RAM: $BIG"
  "$BIN" show "$BIG" > "$OUT/show_$BIG.txt" 2>&1
  timeout 1200 python3 -u "$HERE/workloads.py" "$B" "$TOKEN" "$OUT" chat "$BIG" 1 >> "$OUT/workloads.log" 2>&1; echo "8b chat exit $?" | tee -a "$OUT/steps.log"
fi

step "end"; stop_server; sleep 20
[ -n "${GROQ_API_KEY:-}" ] && "$BIN" provider remove groq > /dev/null 2>&1
rm -f "$HERE/.groq"
touch "$OUT/stop"; wait "$SAMPLER" 2> /dev/null
python3 "$HERE/analyze.py" "$OUT" > /dev/null
echo "== $(date +%H:%M:%S) all done" | tee -a "$OUT/steps.log"
