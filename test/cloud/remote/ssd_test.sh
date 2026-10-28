#!/usr/bin/env bash
# A model bigger than RAM + VRAM: what the planner chooses, and Cluaiz against llama.cpp with that placement,
# both reading the rest from the disk. run.py's runs are too long for this (its first request thinks freely,
# hundreds of tokens at seconds per token), so here: two short requests without thinking, and llama-bench with
# the placement Cluaiz reported. Disk reads are counted from /proc/diskstats around each step.
# Usage: ssd_test.sh <model id, as registered> <name> [max tokens, default 8] [request llama params as JSON]
# The planner refuses a dense model that does not fit RAM + VRAM; the JSON (e.g. {"n_gpu_layers": 23}) is
# then the user setting that runs it anyway.
set -uo pipefail
MODEL="$1"; NAME="$2"; NTOK="${3:-8}"; LLAMA="${4:-}"
OUT="$HOME/results-regression"; mkdir -p "$OUT"
BIN="${CLUAIZ_BIN:-$HOME/.cluaiz/bin/cluaiz}"
LB="$HOME/cluaiz/engine/llama/llama.cpp/build-bench/bin/llama-bench"
export CLUAIZ_HOME="$HOME/ch-reg" CLUAIZ_PORT=8011 CLUAIZ_HOST=127.0.0.1 CLUAIZ_NONINTERACTIVE=1
DISK=$(basename "$(findmnt -no SOURCE /)" | sed 's/[0-9]*$//')
read_gb() { awk -v d="$DISK" '$3 == d { printf "%.2f", $6 * 512 / 1e9 }' /proc/diskstats; }
LOG="$OUT/ssd_$NAME.log"
{
  echo "== machine"; free -g; nvidia-smi --query-gpu=name,memory.total,memory.used --format=csv
  ls -la "$CLUAIZ_HOME/models/chat/$MODEL"
} > "$LOG" 2>&1
sync; echo 3 | sudo tee /proc/sys/vm/drop_caches >/dev/null

"$BIN" serve > "$OUT/server_ssd_$NAME.log" 2>&1 &
SERVER=$!
for i in $(seq 1 120); do curl -s -m 2 http://127.0.0.1:8011/health >/dev/null && break; sleep 1; done

for run in 1 2; do
  before=$(read_gb); start=$(date +%s)
  python3 - "$MODEL" "$NTOK" "$run" "$LLAMA" >> "$LOG" 2>&1 <<'EOF'
import json, random, sys, urllib.error, urllib.request
from pathlib import Path
model, ntok, run, llama = sys.argv[1], int(sys.argv[2]), sys.argv[3], sys.argv[4]
token = (Path.home() / "ch-reg" / "session.token").read_text().strip()
filler = ("The river carried small stones from the mountains to the valley, and the farmers along its "
          "banks used the water for their fields in every season. ")
body = {"model": model, "max_tokens": ntok, "stream": False, "temporary_chat": "strict", "think_mode": "off",
        "messages": [{"role": "user", "content": f"Note {random.randint(0, 10**9)}. {filler * 18}\n"
                                                 "In two sentences, what is this text about?"}]}
if llama:
    body["llama"] = json.loads(llama)
req = urllib.request.Request("http://127.0.0.1:8011/v1/chat/completions", data=json.dumps(body).encode(),
                             headers={"Content-Type": "application/json", "Authorization": f"Bearer {token}"})
try:
    resp = json.loads(urllib.request.urlopen(req, timeout=7200).read())
except urllib.error.HTTPError as e:
    print(f"== cluaiz request {run}: HTTP {e.code}: {e.read().decode(errors='replace')[:400]}")
    sys.exit(0)
engine = resp.get("engine") or {}
t, load = engine.get("timings") or {}, engine.get("load") or {}
print(f"== cluaiz request {run}: prompt {t.get('prompt_n')} tok {t.get('prompt_per_second', 0):.2f} t/s | "
      f"gen {t.get('predicted_n')} tok {t.get('predicted_per_second', 0):.3f} t/s")
print("   load:", json.dumps({k: load.get(k) for k in ("placement", "n_gpu_layers", "n_cpu_moe", "cache_type", "n_ctx", "settings")}))
print("   answer:", ((resp.get("choices") or [{}])[0].get("message") or {}).get("content", "")[:200].replace("\n", " "))
EOF
  echo "   wall $(( $(date +%s) - start )) s, disk read $(awk -v a="$(read_gb)" -v b="$before" 'BEGIN { printf "%.2f", a - b }') GB" >> "$LOG"
done
NGL=$(grep -o '"n_gpu_layers": [-0-9]*' "$LOG" | tail -1 | grep -o '[-0-9]*$')
NCMOE=$(grep -o '"n_cpu_moe": [0-9]*' "$LOG" | tail -1 | grep -o '[0-9]*$')
KV=$(grep -o '"cache_type": "[A-Za-z0-9]*"' "$LOG" | tail -1 | grep -o 'Kv[0-9]*')
kv=$(case "$KV" in Kv8) echo q8_0 ;; Kv4) echo q4_0 ;; *) echo f16 ;; esac)
THREADS=$(grep -o '"n_threads": [0-9]*' "$LOG" | tail -1 | grep -o '[0-9]*$')
kill "$SERVER"; wait "$SERVER" 2>/dev/null
sed 's/\x1b\[[0-9;]*m//g' "$OUT/server_ssd_$NAME.log" | grep -aE "^║|Loading model:|VRAM planned|over its estimate|timings" > "$OUT/planner_ssd_$NAME.log"

if [ -z "$NGL" ]; then
  echo "== Cluaiz did not load the model, so there is no placement to give llama-bench" >> "$LOG"
  echo "== done $NAME" >> "$LOG"
  exit 0
fi
GGUF=$(ls -S "$CLUAIZ_HOME/models/chat/$MODEL"/*.gguf | head -1)
sync; echo 3 | sudo tee /proc/sys/vm/drop_caches >/dev/null
for args in "-p 535 -n 0" "-p 0 -n $NTOK -d 535"; do
  before=$(read_gb); start=$(date +%s)
  echo "== llama-bench -ngl ${NGL:-0} -ncmoe ${NCMOE:-0} -ctk $kv -ctv $kv $args" >> "$LOG"
  "$LB" -m "$GGUF" -ngl "${NGL:-0}" -ncmoe "${NCMOE:-0}" -ctk "$kv" -ctv "$kv" -fa auto -t "${THREADS:-$(nproc)}" \
    -b 512 -ub 512 -r 1 $args -o md >> "$LOG" 2>&1
  echo "   wall $(( $(date +%s) - start )) s, disk read $(awk -v a="$(read_gb)" -v b="$before" 'BEGIN { printf "%.2f", a - b }') GB" >> "$LOG"
done
echo "== done $NAME" >> "$LOG"
