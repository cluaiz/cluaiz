#!/usr/bin/env bash
# Feature sweep of the installed release on a test VM: every CLI command group, the CEP journeys
# of the repository (test/cep), MCP, automation (flows), ingest, the agent. One log per step in
# ~/t7/features/, one line per step in ~/t7/features/summary.tsv (written as each step ends, so a
# stopped Spot VM keeps what finished). Isolated home ~/ch, ports 8012-8019 (never 8000/8011).
set -uo pipefail
B="$HOME/.cluaiz/bin/cluaiz"
OUT="$HOME/t7/features"; mkdir -p "$OUT"
export CLUAIZ_HOME="$HOME/ch" CLUAIZ_PORT=8012 CLUAIZ_HOST=127.0.0.1 CLUAIZ_NONINTERACTIVE=1
SUM="$OUT/summary.tsv"
[ -f "$SUM" ] || printf 'step\texit\tseconds\n' > "$SUM"

step() {  # step <name> <command...>; skipped when it already has a result
  local name="$1"; shift
  grep -q "^$name	" "$SUM" && return 0
  local start=$(date +%s)
  echo "== $name" | tee "$OUT/$name.log"
  echo "\$ $*" >> "$OUT/$name.log"
  timeout "${STEP_TIMEOUT:-900}" "$@" >> "$OUT/$name.log" 2>&1 < /dev/null
  local code=$?
  printf '%s\t%s\t%s\n' "$name" "$code" "$(( $(date +%s) - start ))" | tee -a "$SUM"
}

# Source at main, for the journeys
if [ -d "$HOME/cluaiz/.git" ]; then
  git -C "$HOME/cluaiz" fetch -q origin main && git -C "$HOME/cluaiz" checkout -q -f origin/main
else
  git clone -q https://github.com/cluaiz/cluaiz.git "$HOME/cluaiz"
fi
git -C "$HOME/cluaiz" log --oneline -1 > "$OUT/source_commit.txt"
"$B" --version > "$OUT/version.txt" 2>&1

# ── CLI: local commands (no server) ──────────────────────────────────────────────────────────
step cli_status        "$B" status
step cli_list          "$B" list
step cli_config_list   "$B" config list
step cli_config_path   "$B" config path
step cli_token_list    "$B" token list
step cli_permission    "$B" permission
step cli_skill_list    "$B" skill list
step cli_plugin_list   "$B" plugin list
step cli_mcp_list      "$B" mcp list
step cli_provider_list "$B" provider list
for id in $("$B" list 2>/dev/null | awk 'NR>1 && $1 != "" {print $1}'); do
  step "cli_show_$id" "$B" show "$id"
done
step cli_show_repo_name      "$B" show unsloth/Qwen3-4B-GGUF
step cli_show_repo_quant     "$B" show unsloth/Qwen3-4B-GGUF:Q4_K_M
step cli_show_not_installed  "$B" show unsloth/no-such-model
step cli_run_qwen3_4b        "$B" run qwen3-4b-gguf-Q4_K_M "Name three rivers in India, one line." --local -n 64
step cli_run_think_off       "$B" run qwen3-4b-gguf-Q4_K_M "What is 17 times 3? Answer with the number." --local -n 32 --think off
step cli_bench_qwen3_4b      "$B" bench qwen3-4b-gguf-Q4_K_M -r 2
step cli_bench_repo_name     "$B" bench unsloth/Qwen3-4B-GGUF -r 1
STEP_TIMEOUT=2400 step cli_bench_gemma_12b "$B" bench gemma-3-12b-it-gguf-Q4_K_M -r 1

# ── Server up: commands and journeys that need it ────────────────────────────────────────────
"$B" serve > "$OUT/server.log" 2>&1 &
SERVER=$!
for i in $(seq 1 180); do curl -s -m 2 "http://127.0.0.1:8012/health" >/dev/null && break; sleep 1; done
step cli_ps              "$B" ps
step cli_storage_status  "$B" storage temp-status
cat > "$OUT/sample.md" <<'EOF'
# River notes
The Ganga rises in the Himalaya and flows east to the Bay of Bengal.
The Narmada flows west into the Arabian Sea through a rift valley.
The Brahmaputra enters India in Arunachal Pradesh and joins the Ganga delta.
EOF
step cli_ingest_md       "$B" ingest "$OUT/sample.md"
step flow_nodes          "$B" flow nodes
cat > "$OUT/hello.flow.yaml" <<'EOF'
flow: cluaiz.flow/1
name: vm-hello
on: [{ manual: {} }]
steps:
  - id: info
    call: system.info
  - id: answer
    ask: "In one short sentence, say hello to the test VM."
EOF
step flow_validate       "$B" flow validate "$OUT/hello.flow.yaml"
step flow_save           "$B" flow save "$OUT/hello.flow.yaml" --yes
step flow_list           "$B" flow list
STEP_TIMEOUT=1200 step flow_run "$B" flow run vm-hello
step flow_logs           "$B" flow logs vm-hello
step flow_explain        "$B" flow explain vm-hello
STEP_TIMEOUT=1500 step flow_draft "$B" flow draft "every morning at 8 read system info and write a one line summary to a note"
step flow_remove         "$B" flow remove vm-hello --yes
mkdir -p "$OUT/agent-work" && cd "$OUT/agent-work"
STEP_TIMEOUT=1500 step agent_task "$B" agent "List the files in this folder and say how many there are."
cd "$HOME"
kill "$SERVER" 2>/dev/null; wait "$SERVER" 2>/dev/null

# ── CEP journeys from the repository (each starts its own server) ──────────────────────────
R="$HOME/cluaiz/test/cep"
step cep_journeys       python3 "$R/journeys.py"    --exe "$B" --home "$HOME/ch" --port 8013
step cep_mcp_journey    python3 "$R/mcp_journey.py" --exe "$B" --home "$HOME/ch" --port 8014
for j in system fs models tools; do
  rm -rf "$HOME/jh-$j"
  step "cep_${j}_journey" python3 "$R/wave_b/${j}_http_journey.py" --exe "$B" --seed-home "$HOME/ch" --home "$HOME/jh-$j" --port 8015
  cp "$HOME/jh-$j/"*-http-report.json "$OUT/" 2>/dev/null
done
echo "== features done" | tee -a "$OUT/done.txt"
