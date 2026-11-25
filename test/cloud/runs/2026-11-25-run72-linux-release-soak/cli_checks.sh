#!/usr/bin/env bash
# Run 72 (copied from run 49): CLI checks against the isolated home. Usage: cli_checks.sh <exe> <out dir>
# The server (if a check needs one) is started by the caller on CLUAIZ_PORT.
EXE="$1"; OUT="$2"; HERE="$(cd "$(dirname "$0")" && pwd)"
: "${CLUAIZ_HOME:?set CLUAIZ_HOME}"; : "${CLUAIZ_PORT:?set CLUAIZ_PORT}"
[ "$CLUAIZ_PORT" = 8000 ] && { echo "never port 8000"; exit 1; }
export CLUAIZ_NONINTERACTIVE=1 NO_COLOR=1
run() { local name="$1"; shift; local s=$SECONDS; { echo "\$ cluaiz $*"; timeout 900 "$EXE" "$@" </dev/null 2>&1; echo "exit=$?"; } | tr '\r' '\n' > "$OUT/cli_$name.txt"; printf '%-22s %s %ss\n' "$name" "$(tail -1 "$OUT/cli_$name.txt")" "$((SECONDS-s))"; }
run version --version
run list list
run show show qwen3.5-0.8b-gguf-Q4_K_M
run status status
run config_path config path
run permission permission
run token_list token list
run provider_list provider list
run set_chat model set-chat qwen3.5-0.8b-gguf-Q4_K_M
run set_vector model set-vector embeddinggemma-300m-gguf-Q8_0
run run_prompt run qwen3.5-0.8b-gguf-Q4_K_M "What is 2+2? Answer with the number only." --think off
run bench bench qwen3.5-0.8b-gguf-Q4_K_M -p 64 -n 16 -r 1
run ps ps
run plugin_install plugin install math --yes
run plugin_list plugin list
run mcp_install mcp install filesystem --yes
run mcp_list mcp list
run flow_validate flow validate "$HERE/run49-check.flow.yaml"
run flow_save flow save "$HERE/run49-check.flow.yaml"
run flow_list flow list
run flow_run flow run run49-check
run flow_logs flow logs run49-check
run storage storage temp-status
run config_list config list
printf '# Notes\n\nThe release check number is 4917.\n' > "$CLUAIZ_HOME/ingest-sample.md"
run ingest ingest "$CLUAIZ_HOME/ingest-sample.md"
WS="${AGENT_WS:-$CLUAIZ_HOME/../run49-agent-ws}"
( cd "$WS" && run agent_headless agent -p "Read notes.txt and tell me the secret number in it." --output-format json )
