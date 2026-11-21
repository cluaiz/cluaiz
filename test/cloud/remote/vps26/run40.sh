#!/usr/bin/env bash
# Run 40: run 26's checks again with a current Cluaiz build, engines interleaved so a CPU cap or a busy
# host hits all three alike (run 26 ran one engine after another, and a host cap fell between them).
#   1. W1 chat, Qwen3 0.6B and 1.7B, 1 and 4 requests at once: cluaiz, llama, ollama, cluaiz, llama, ollama
#   2. Cluaiz only: W4 embeddings and W5 speech at 1, 2 and 4 at once (both refused requests in run 26)
# Usage: CLUAIZ_COMMIT=<sha> bash run40.sh [out_dir]   (each block is a run26.sh call with its own folder)
set -euo pipefail
HERE="$(cd "$(dirname "$0")" && pwd)"
OUT="${1:-/root/results-run40}"
STEP="${STEP_SECS:-120}"
mkdir -p "$OUT"
# The CPU state before each block: steal time shows a host that takes CPU back
cpu_note() { echo "$(date -u +%FT%TZ) $1 $(head -1 /proc/stat)" >> "$OUT/cpu_stat.log"; }

for round in 1 2; do
  for eng in cluaiz llama ollama; do
    cpu_note "r${round}_${eng}_W1"
    bash "$HERE/run26.sh" --mode native --engines "$eng" --models qwen3-0.6b,qwen3-1.7b --workloads W1 \
      --levels 1,4 --step-secs "$STEP" --out-dir "$OUT/r${round}_${eng}_w1" --inputs-dir "$OUT/inputs"
  done
done
cpu_note "cluaiz_W4_W5"
bash "$HERE/run26.sh" --mode native --engines cluaiz --workloads W4,W5 --levels 1,2,4 --step-secs "$STEP" \
  --w4-limit 200 --out-dir "$OUT/cluaiz_w4_w5" --inputs-dir "$OUT/inputs"
cpu_note "done"
echo "ALL_DONE"
