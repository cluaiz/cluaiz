#!/usr/bin/env bash
# Run 40: run 26's checks again with a current Cluaiz build, engines interleaved so a CPU cap or a busy
# host hits all three alike (run 26 ran one engine after another, and a host cap fell between them).
#   1. W1 chat, Qwen3 0.6B and 1.7B, 1 and 4 requests at once: cluaiz, llama, ollama, cluaiz, llama, ollama
#   2. Cluaiz only: W4 embeddings and W5 speech at 1, 2 and 4 at once (both refused requests in run 26)
# Usage: CLUAIZ_COMMIT=<sha> bash run40.sh [out_dir]   (each block is a run26.sh call with its own folder)
# ROUNDS="1 2" picks the W1 rounds. SOAK_SECS adds run26's soak after each model's steps; the default is 0
# because run26.sh alone defaults to a 1-hour soak, which made each engine block take about 2 hours.
# WEB_USERS=<n> runs webapp/server.js (Node.js + PostgreSQL) for the whole run with n simulated users
# (webapp/web_users.js) instead of run26's one-check-a-second neighbour_app.py; needs nodejs and node-pg.
set -euo pipefail
HERE="$(cd "$(dirname "$0")" && pwd)"
OUT="${1:-/root/results-run40}"
STEP="${STEP_SECS:-120}"
SOAK="${SOAK_SECS:-0}"
ROUNDS="${ROUNDS:-1 2}"
WEB_USERS="${WEB_USERS:-0}"
mkdir -p "$OUT"
# The CPU state before each block: steal time shows a host that takes CPU back
cpu_note() { echo "$(date -u +%FT%TZ) $1 $(head -1 /proc/stat)" >> "$OUT/cpu_stat.log"; }

NB=()
if [ "$WEB_USERS" -gt 0 ]; then
  node "$HERE/webapp/server.js" 3100 neighbour_app > "$OUT/webapp.log" 2>&1 &
  WEB_PID=$!
  for _ in $(seq 1 30); do curl -sf http://127.0.0.1:3100/health >/dev/null && break; sleep 1; done
  curl -sf http://127.0.0.1:3100/health >/dev/null || { echo "website did not start: $OUT/webapp.log" >&2; exit 1; }
  node "$HERE/webapp/web_users.js" http://127.0.0.1:3100 "$WEB_USERS" "$OUT/web_users.csv" > "$OUT/web_users.log" 2>&1 &
  USERS_PID=$!
  trap 'kill "$USERS_PID" 2>/dev/null; sleep 7; kill "$WEB_PID" 2>/dev/null' EXIT
  NB=(--external-neighbour)
  sleep 60   # one minute of website traffic alone, as the baseline
fi

for round in $ROUNDS; do
  for eng in cluaiz llama ollama; do
    cpu_note "r${round}_${eng}_W1"
    bash "$HERE/run26.sh" --mode native --engines "$eng" --models qwen3-0.6b,qwen3-1.7b --workloads W1 \
      --levels 1,4 --step-secs "$STEP" --soak-secs "$SOAK" --out-dir "$OUT/r${round}_${eng}_w1" \
      --inputs-dir "$OUT/inputs" "${NB[@]}"
  done
done
cpu_note "cluaiz_W4_W5"
bash "$HERE/run26.sh" --mode native --engines cluaiz --workloads W4,W5 --levels 1,2,4 --step-secs "$STEP" \
  --soak-secs 0 --w4-limit 200 --out-dir "$OUT/cluaiz_w4_w5" --inputs-dir "$OUT/inputs" "${NB[@]}"
cpu_note "done"
echo "ALL_DONE"
