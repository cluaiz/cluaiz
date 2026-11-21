#!/usr/bin/env bash
# Resumable, parameterized 10-stage Day 2-3 master runner for vps26 (4 GB VPS or 8 GB VM).
set -euo pipefail

HERE="$(cd "$(dirname "$0")" && pwd)"
MEM_MAX_MB="auto"
THREADS="auto"
CTX=4096
KV="q8_0"
SLOTS=1
OUT_ROOT="/root/results-run26"

while [ "$#" -gt 0 ]; do
  case "$1" in
    --mem-max-mb) MEM_MAX_MB="$2"; shift 2 ;;
    --threads) THREADS="$2"; shift 2 ;;
    --ctx) CTX="$2"; shift 2 ;;
    --kv) KV="$2"; shift 2 ;;
    --slots) SLOTS="$2"; shift 2 ;;
    --out-root) OUT_ROOT="$2"; shift 2 ;;
    *) echo "Unknown arg: $1" >&2; exit 2 ;;
  esac
done

mkdir -p "$OUT_ROOT"
echo $$ > "$OUT_ROOT/master.pid"

echo "=== PRE-FLIGHT CHECK $(date -Iseconds) ==="
if docker ps --format '{{.Names}}' 2>/dev/null | grep -qx cluaiz-web; then
  docker stop cluaiz-web >/dev/null || true
fi
systemctl stop nginx >/dev/null 2>&1 || true
docker ps -a --format 'table {{.Names}}\t{{.Status}}'
df -h /
free -m

if [ ! -f "$OUT_ROOT/inputs/w1_tickets.json" ]; then
  python3 "$HERE/generate_inputs.py" --out-dir "$OUT_ROOT"
fi

last_good_level() {
  local jsonl="$1" wl="$2" model="$3"
  python3 - "$jsonl" "$wl" "$model" <<'PY'
import json, sys
p, wl, m = sys.argv[1], sys.argv[2], sys.argv[3]
best = None
try:
    for line in open(p):
        if not line.strip():
            continue
        r = json.loads(line)
        if r.get("workload") == wl and r.get("model") == m and r.get("phase_kind") == "ramp" and r.get("passed"):
            lvl = int(r.get("level", 1))
            if best is None or lvl > best:
                best = lvl
except OSError:
    pass
print(best if best is not None else 1)
PY
}

run_stage() {
  local name="$1"; shift
  local sdir="$OUT_ROOT/$name"
  if [ -f "$sdir/STAGE_DONE" ]; then
    echo "== SKIP $name (already done) =="
    return 0
  fi
  echo "== BEGIN STAGE $name at $(date -Iseconds) =="
  mkdir -p "$sdir"
  bash "$HERE/run26.sh" \
    --mem-max-mb "$MEM_MAX_MB" --threads "$THREADS" --ctx "$CTX" --kv "$KV" --slots "$SLOTS" \
    --inputs-dir "$OUT_ROOT/inputs" --skip-inputs --out-dir "$sdir" "$@"
  date -Iseconds > "$sdir/STAGE_DONE"
  echo "== END STAGE $name at $(date -Iseconds) =="
}

# Stages 1-3: Native full W1-W6 across all 3 engines
run_stage native_cluaiz \
  --mode native --engines cluaiz --models qwen3-0.6b,qwen3-1.7b,llama3.2-3b \
  --workloads W1,W2,W3,W4,W5,W6 --levels 1,2,4,8,16,32 --step-secs 300 --soak-secs 0 --w4-limit 2000 --check-recovery

run_stage native_ollama \
  --mode native --engines ollama --models qwen3-0.6b,qwen3-1.7b,llama3.2-3b \
  --workloads W1,W2,W3,W4,W5,W6 --levels 1,2,4,8,16,32 --step-secs 300 --soak-secs 0 --w4-limit 2000 --check-recovery

run_stage native_llama \
  --mode native --engines llama --models qwen3-0.6b,qwen3-1.7b,llama3.2-3b \
  --workloads W1,W2,W3,W4,W5,W6 --levels 1,2,4,8,16,32 --step-secs 300 --soak-secs 0 --w4-limit 2000 --check-recovery

# Stages 4-7: 1-hour Native soaks at last passing level
CLUAIZ_SOAK_LVL=$(last_good_level "$OUT_ROOT/native_cluaiz/steps_native_cluaiz.jsonl" W1 qwen3-1.7b)
run_stage soak_native_cluaiz_w1 \
  --mode native --engines cluaiz --models qwen3-1.7b --workloads W1 \
  --levels "$CLUAIZ_SOAK_LVL" --step-secs 3600 --soak-secs 0 --phase-kind soak

OLLAMA_SOAK_LVL=$(last_good_level "$OUT_ROOT/native_ollama/steps_native_ollama.jsonl" W1 qwen3-1.7b)
run_stage soak_native_ollama_w1 \
  --mode native --engines ollama --models qwen3-1.7b --workloads W1 \
  --levels "$OLLAMA_SOAK_LVL" --step-secs 3600 --soak-secs 0 --phase-kind soak

LLAMA_SOAK_LVL=$(last_good_level "$OUT_ROOT/native_llama/steps_native_llama.jsonl" W1 qwen3-1.7b)
run_stage soak_native_llama_w1 \
  --mode native --engines llama --models qwen3-1.7b --workloads W1 \
  --levels "$LLAMA_SOAK_LVL" --step-secs 3600 --soak-secs 0 --phase-kind soak

W6_SOAK_LVL=$(last_good_level "$OUT_ROOT/native_cluaiz/steps_native_cluaiz.jsonl" W6 qwen3-0.6b)
run_stage soak_native_cluaiz_w6 \
  --mode native --engines cluaiz --models qwen3-0.6b --workloads W6 \
  --levels "$W6_SOAK_LVL" --step-secs 3600 --soak-secs 0 --phase-kind soak

# Stages 8-10: Docker core W1, W4, W6 across all 3 engines
run_stage docker_cluaiz \
  --mode docker --engines cluaiz --models qwen3-0.6b,qwen3-1.7b,llama3.2-3b \
  --workloads W1,W4,W6 --levels 1,2,4,8,16,32 --step-secs 300 --soak-secs 0 --w4-limit 2000

run_stage docker_ollama \
  --mode docker --engines ollama --models qwen3-0.6b,qwen3-1.7b,llama3.2-3b \
  --workloads W1,W4,W6 --levels 1,2,4,8,16,32 --step-secs 300 --soak-secs 0 --w4-limit 2000

run_stage docker_llama \
  --mode docker --engines llama --models qwen3-0.6b,qwen3-1.7b,llama3.2-3b \
  --workloads W1,W4,W6 --levels 1,2,4,8,16,32 --step-secs 300 --soak-secs 0 --w4-limit 2000

date -Iseconds > "$OUT_ROOT/ALL_STAGES_DONE"
echo "=== ALL 10 STAGES COMPLETE at $(date -Iseconds) ==="
df -h /
