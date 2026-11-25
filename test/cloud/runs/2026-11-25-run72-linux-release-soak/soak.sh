#!/usr/bin/env bash
# Run 72: two-hour soak on Linux with the existing client test/release_soak/soak.py (unchanged), plus
# bg_users.py (extra users the whole time) and sampler.sh (process RSS and GPU memory every 10 s).
# soak.py starts and restarts the server itself (port 8013, host 127.0.0.1, the e2e home with the math
# plugin and filesystem MCP installed, so long_input takes the Engine's tool loop as in runs 52 / 55 / 70).
# Usage: soak.sh <package dir> <repo dir> <out dir> <minutes> <seed>
set -uo pipefail
PKG="$1" REPO="$2" OUT="$3" MIN="$4" SEED="$5"; HERE="$(cd "$(dirname "$0")" && pwd)"
HOMEDIR="$HOME/run72/home"; MODEL=qwen3.5-0.8b-gguf-Q4_K_M
unset LD_LIBRARY_PATH
# soak.py replaces $USERNAME (the Windows name) with <user> in what it writes; on Linux the name is $USER
export USERNAME="$USER"
mkdir -p "$OUT"
pgrep -fa "cluaiz serve" > "$OUT/before_processes.txt"; nvidia-smi > "$OUT/before_gpu.txt"; free -m >> "$OUT/before_gpu.txt"
bash "$HERE/sampler.sh" "$PKG" "$OUT/sampler.jsonl" & SAMPLER=$!
python3 "$REPO/test/release_soak/soak.py" --exe "$PKG/cluaiz" --home "$HOMEDIR" --out "$OUT" --model "$MODEL" \
  --port 8013 --minutes "$MIN" --seed "$SEED" --users 4 --restart-every 2 > "$OUT/soak_console.txt" 2>&1 & SOAK=$!
# start the extra users once the first server answers
for i in $(seq 1 120); do curl -s -m 2 http://127.0.0.1:8013/health >/dev/null && break; sleep 1; done
python3 "$HERE/bg_users.py" --home "$HOMEDIR" --port 8013 --model "$MODEL" --users 2 --minutes "$MIN" --seed "$SEED" \
  --out "$OUT/bg.jsonl" > "$OUT/bg_console.txt" 2>&1 & BG=$!
wait $SOAK; echo "soak exit $?" >> "$OUT/soak_console.txt"
wait $BG
kill $SAMPLER
nvidia-smi > "$OUT/after_gpu.txt"; free -m >> "$OUT/after_gpu.txt"; pgrep -fa "cluaiz serve" > "$OUT/after_processes.txt"
echo "== soak done"
