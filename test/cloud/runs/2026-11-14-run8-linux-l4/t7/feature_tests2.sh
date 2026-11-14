#!/usr/bin/env bash
# Second pass of feature_tests.sh: the steps whose first run had a test-side mistake
# (no default chat model for the agent; `flow save` has no --yes; the wave_b journeys expect a seed
# home with engine/config, the layout before config/ moved to the home's root).
set -uo pipefail
B="$HOME/.cluaiz/bin/cluaiz"
OUT="$HOME/t7/features"
export CLUAIZ_HOME="$HOME/ch" CLUAIZ_PORT=8012 CLUAIZ_HOST=127.0.0.1 CLUAIZ_NONINTERACTIVE=1
SUM="$OUT/summary2.tsv"
[ -f "$SUM" ] || printf 'step\texit\tseconds\n' > "$SUM"
step() {
  local name="$1"; shift
  grep -q "^$name	" "$SUM" && return 0
  local start=$(date +%s)
  echo "== $name" | tee "$OUT/$name.log"
  echo "\$ $*" >> "$OUT/$name.log"
  timeout "${STEP_TIMEOUT:-900}" "$@" >> "$OUT/$name.log" 2>&1 < /dev/null
  local code=$?
  printf '%s\t%s\t%s\n' "$name" "$code" "$(( $(date +%s) - start ))" | tee -a "$SUM"
}

step model_set_chat "$B" model set-chat qwen3-4b-gguf-Q4_K_M
"$B" serve > "$OUT/server2.log" 2>&1 &
SERVER=$!
for i in $(seq 1 180); do curl -s -m 2 "http://127.0.0.1:8012/health" >/dev/null && break; sleep 1; done
step flow_save2      "$B" flow save "$OUT/hello.flow.yaml"
step flow_list2      "$B" flow list
STEP_TIMEOUT=1200 step flow_run2 "$B" flow run vm-hello
step flow_logs2      "$B" flow logs vm-hello
STEP_TIMEOUT=1200 step flow_explain2 "$B" flow explain vm-hello
step flow_remove2    "$B" flow remove vm-hello
mkdir -p "$OUT/agent-work" && cd "$OUT/agent-work" && printf 'a\n' > one.txt && printf 'b\n' > two.txt
STEP_TIMEOUT=1500 step agent_task2 "$B" agent "List the files in this folder and say how many there are."
cd "$HOME"
kill "$SERVER" 2>/dev/null; wait "$SERVER" 2>/dev/null

# Seed home in the layout the wave_b journeys read (engine/config)
rm -rf "$HOME/seed" && mkdir -p "$HOME/seed/engine" && cp -r "$HOME/ch/config" "$HOME/seed/engine/config"
R="$HOME/cluaiz/test/cep"
for j in system fs models tools; do
  rm -rf "$HOME/jh-$j"
  step "cep_${j}_journey2" python3 "$R/wave_b/${j}_http_journey.py" --exe "$B" --seed-home "$HOME/seed" --home "$HOME/jh-$j" --port 8015
  cp "$HOME/jh-$j/"*-http-report.json "$OUT/" 2>/dev/null
done
echo "== features2 done" | tee -a "$OUT/done2.txt"
