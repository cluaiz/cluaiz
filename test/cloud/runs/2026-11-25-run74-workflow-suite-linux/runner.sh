#!/usr/bin/env bash
# Run 74: run 56's workflow suites on Linux (L4 VM) with the run 72 release build.
# engine-check.cjs and runtime-check.cjs are run 56's files, unchanged. runtime-check.cjs already waits 90 s for
# the Engine to listen (raised from 18 s after run 56, never rerun until now).
#   1. fresh isolated home, `cluaiz pull` of the 0.8B chat model, `cluaiz plugin install math --yes`
#   2. engine-check.cjs against a server started here (run 56's runner.py does the same on Windows)
#   3. runtime-check.cjs full: starts its own server, prepares a waiting run, stops it, starts a new server and
#      checks startup trigger, restart recovery, webhook secret, tool trigger over MCP, Code, Tool, Ask, Agent
# Usage: runner.sh <package dir>
set -uo pipefail
PKG="$1"; HERE="$(cd "$(dirname "$0")" && pwd)"
unset LD_LIBRARY_PATH
export CLUAIZ_HOME="$HOME/run74/home" CLUAIZ_PORT=8013 CLUAIZ_HOST=127.0.0.1 CLUAIZ_NONINTERACTIVE=1 NO_COLOR=1
export CLUAIZ_EXE="$PKG/cluaiz"
[ -e "$CLUAIZ_HOME" ] && { echo "home exists"; exit 1; }
mkdir -p "$CLUAIZ_HOME/workspace" "$HERE/raw"
{ node --version; sha256sum "$CLUAIZ_EXE"; } > "$HERE/raw/versions.txt" 2>&1
"$CLUAIZ_EXE" pull unsloth/Qwen3.5-0.8B-GGUF:Q4_K_M </dev/null 2>&1 | tr '\r' '\n' | tail -5 > "$HERE/raw/pull.txt"
"$CLUAIZ_EXE" plugin install math --yes </dev/null > "$HERE/raw/plugin_install.txt" 2>&1; echo "exit=$?" >> "$HERE/raw/plugin_install.txt"

"$CLUAIZ_EXE" serve > "$HERE/raw/engine-check-engine.log" 2>&1 &
SERVER=$!
s=$(date +%s); for i in $(seq 1 120); do curl -s -m 2 http://127.0.0.1:8013/health >/dev/null && break; sleep 1; done
echo "server ready after $(( $(date +%s) - s )) s" > "$HERE/raw/engine-check-console.txt"
(cd "$CLUAIZ_HOME/workspace" && node "$HERE/engine-check.cjs") >> "$HERE/raw/engine-check-console.txt" 2>&1
echo "exit=$?" >> "$HERE/raw/engine-check-console.txt"
kill "$SERVER"; wait "$SERVER" 2>/dev/null

(cd "$CLUAIZ_HOME/workspace" && node "$HERE/runtime-check.cjs" full) > "$HERE/raw/runtime-check-console.txt" 2>&1
echo "exit=$?" >> "$HERE/raw/runtime-check-console.txt"
echo "== run74 done"
