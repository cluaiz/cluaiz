#!/usr/bin/env bash
# Run 72: release end-to-end checks on Linux with the packaged build (build.sh), run 49's way.
# Fresh isolated home, port 8013, host 127.0.0.1, auth settings left at their defaults; models pulled
# with `cluaiz pull`; then CLI (cli_checks.sh), HTTP (release_e2e.py), tools (tools_checks.py) and the
# CEP journeys of test/cep. Raw output goes to <out>.
# Usage: e2e.sh <package dir> <repo dir> <out dir>
set -uo pipefail
PKG="$1" REPO="$2" OUT="$3"; HERE="$(cd "$(dirname "$0")" && pwd)"
mkdir -p "$OUT"
unset LD_LIBRARY_PATH
EXE="$PKG/cluaiz"
export CLUAIZ_HOME="$HOME/run72/home" CLUAIZ_PORT=8013 CLUAIZ_HOST=127.0.0.1 CLUAIZ_NONINTERACTIVE=1 NO_COLOR=1
[ -e "$CLUAIZ_HOME" ] && { echo "home exists, refusing to reuse"; exit 1; }
mkdir -p "$CLUAIZ_HOME"
{
  echo "== hardware"; lscpu | grep -E "Model name|^CPU\(s\)"; free -m; nvidia-smi; df -h /
  echo "== os"; (. /etc/os-release; echo "$PRETTY_NAME"); ldd --version | head -1; uname -r
  echo "== binary"; sha256sum "$EXE"; ls -la "$PKG"
} > "$OUT/machine.txt" 2>&1

echo "== pulls" | tee "$OUT/pulls.txt"
for id in unsloth/Qwen3.5-0.8B-GGUF:Q4_K_M ggml-org/embeddinggemma-300M-GGUF onnx-community/Kokoro-82M-v1.0-ONNX:uint8 onnx-community/moonshine-tiny-ONNX:int8; do
  s=$(date +%s)
  "$EXE" pull "$id" </dev/null 2>&1 | tr '\r' '\n' > "$OUT/pull_$(echo "$id" | tr '/:' '__').txt"
  echo "$id exit ${PIPESTATUS[0]} $(( $(date +%s) - s ))s $(grep -a 'Registered ID' "$OUT/pull_$(echo "$id" | tr '/:' '__').txt" | tail -1 | sed 's/\x1b\[[0-9;]*m//g')" | tee -a "$OUT/pulls.txt"
done

"$EXE" serve > "$OUT/serve.log" 2>&1 &
SERVER=$!
for i in $(seq 1 120); do curl -s -m 2 http://127.0.0.1:8013/health >/dev/null && break; sleep 1; done
echo "server pid $SERVER up after ${i}s" | tee "$OUT/serve_start.txt"

# The agent check reads notes.txt from a workspace folder outside the home
export AGENT_WS="$HOME/run72/agent-ws"; mkdir -p "$AGENT_WS"
printf 'Project notes.\nThe secret number is 4917.\n' > "$AGENT_WS/notes.txt"
bash "$HERE/cli_checks.sh" "$EXE" "$OUT" 2>&1 | tee "$OUT/cli_summary.txt"
# Run 49: `flow save` asks before saving; answer y, then run the saved flow
{ echo "\$ cluaiz flow save (answer y)"; printf 'y\n' | "$EXE" flow save "$HERE/run49-check.flow.yaml" 2>&1; echo "exit=$?"; } | tr '\r' '\n' > "$OUT/cli_flow_save_yes.txt"
{ echo "\$ cluaiz flow run run49-check"; timeout 900 "$EXE" flow run run49-check </dev/null 2>&1; echo "exit=$?"; } | tr '\r' '\n' > "$OUT/cli_flow_run_after_save.txt"
tail -1 "$OUT/cli_flow_save_yes.txt" "$OUT/cli_flow_run_after_save.txt" | tee -a "$OUT/cli_summary.txt"

python3 "$HERE/release_e2e.py" --home "$CLUAIZ_HOME" --port 8013 --out "$OUT/http_checks.jsonl" 2>&1 | tee "$OUT/http_summary.txt"
python3 "$HERE/tools_checks.py" --home "$CLUAIZ_HOME" --port 8013 --out "$OUT/tools_checks.jsonl" 2>&1 | tee "$OUT/tools_summary.txt"

grep -oE "/[^ ]*(ggml|llama|mtmd|cudart|cublas|gomp|onnxruntime)[^ ]*\.so[^ ]*" "/proc/$SERVER/maps" | sort -u > "$OUT/loaded_libs.txt"
nvidia-smi --query-compute-apps=pid,used_memory --format=csv > "$OUT/gpu_apps_end.txt"
kill "$SERVER"; wait "$SERVER" 2>/dev/null
grep -aE "Backends loaded|PLAN|Full GPU|ONNX on|fell back|could not load|out of memory|ERROR|panicked|step/repeat limit" "$OUT/serve.log" | cut -c1-300 > "$OUT/serve_highlights.txt"

python3 "$REPO/test/cep/journeys.py" --exe "$EXE" --home "$CLUAIZ_HOME" --port 8013 > "$OUT/journeys_cep.txt" 2>&1; echo "journeys exit $?" >> "$OUT/journeys_cep.txt"
python3 "$REPO/test/cep/mcp_journey.py" --exe "$EXE" --home "$CLUAIZ_HOME" --port 8013 > "$OUT/journeys_mcp.txt" 2>&1; echo "mcp_journey exit $?" >> "$OUT/journeys_mcp.txt"
echo "== e2e done"
