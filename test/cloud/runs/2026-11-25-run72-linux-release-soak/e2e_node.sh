#!/usr/bin/env bash
# Run 72: second pass after Node.js 20 / npm 9 were installed on the VM (Debian packages). The first pass
# (e2e.sh) ran without npx, so the `filesystem` MCP server could not start. This pass reruns, on the same home
# and binary: tools_checks.py, and from release_e2e.py the checks that involve the Engine's own tools
# (long_input_option_b, chat_vision, vision_tools_call, chat_tools, json_schema_with_tools).
# Usage: e2e_node.sh <package dir> <out dir>
set -uo pipefail
PKG="$1" OUT="$2"; HERE="$(cd "$(dirname "$0")" && pwd)"
unset LD_LIBRARY_PATH
export CLUAIZ_HOME="$HOME/run72/home" CLUAIZ_PORT=8013 CLUAIZ_HOST=127.0.0.1 CLUAIZ_NONINTERACTIVE=1 NO_COLOR=1
{ node --version; npm --version; npx --version; } > "$OUT/node_versions.txt" 2>&1
"$PKG/cluaiz" serve > "$OUT/serve_node.log" 2>&1 &
SERVER=$!
for i in $(seq 1 120); do curl -s -m 2 http://127.0.0.1:8013/health >/dev/null && break; sleep 1; done
python3 "$HERE/tools_checks.py" --home "$CLUAIZ_HOME" --port 8013 --out "$OUT/tools_checks_node.jsonl" 2>&1 | tee "$OUT/tools_summary_node.txt"
python3 "$HERE/release_e2e.py" --home "$CLUAIZ_HOME" --port 8013 --out "$OUT/http_checks_node.jsonl" \
  --only long_input_option_b,chat_vision,vision_tools_call,chat_tools,json_schema_with_tools 2>&1 | tee "$OUT/http_summary_node.txt"
kill "$SERVER"; wait "$SERVER" 2>/dev/null
grep -aE "step/repeat limit|Confirmation timed out|Failed to spawn|ERROR|panicked" "$OUT/serve_node.log" | cut -c1-300 > "$OUT/serve_node_highlights.txt"
echo "== e2e_node done"
