#!/usr/bin/env bash
# Parameterized master runner for vps26 (supports 4 GB VPS and upcoming 8 GB VM).
set -euo pipefail

HERE="$(cd "$(dirname "$0")" && pwd)"
source "$HERE/engines.sh"

MODES="native"
ENGINES="cluaiz,ollama,llama"
MODELS="qwen3-0.6b,qwen3-1.7b,llama3.2-3b"
W6_MODEL="qwen3-0.6b"
WORKLOADS="W1,W2,W3,W4,W5,W6"
MEM_MAX_MB="auto"
THREADS="auto"
CTX=4096
KV="q8_0"
SLOTS=1
LEVELS="1,2,4,8,16,32"
STEP_SECS=300
SOAK_SECS=3600
W4_LIMIT=2000
OLLAMA_IMAGE="ollama/ollama:0.40.16"
OUT_DIR="/root/results-run26"
INPUTS_DIR=""
PHASE_KIND="ramp"
SKIP_INPUTS=0
CHECK_RECOVERY=0

while [ "$#" -gt 0 ]; do
  case "$1" in
    --mode) MODES="$2"; shift 2 ;;
    --engines) ENGINES="$2"; shift 2 ;;
    --models) MODELS="$2"; shift 2 ;;
    --w6-model) W6_MODEL="$2"; shift 2 ;;
    --workloads) WORKLOADS="$2"; shift 2 ;;
    --mem-max-mb) MEM_MAX_MB="$2"; shift 2 ;;
    --threads) THREADS="$2"; shift 2 ;;
    --ctx) CTX="$2"; shift 2 ;;
    --kv) KV="$2"; shift 2 ;;
    --slots) SLOTS="$2"; shift 2 ;;
    --levels) LEVELS="$2"; shift 2 ;;
    --step-secs) STEP_SECS="$2"; shift 2 ;;
    --soak-secs) SOAK_SECS="$2"; shift 2 ;;
    --w4-limit) W4_LIMIT="$2"; shift 2 ;;
    --ollama-image) OLLAMA_IMAGE="$2"; shift 2 ;;
    --out-dir) OUT_DIR="$2"; shift 2 ;;
    --inputs-dir) INPUTS_DIR="$2"; shift 2 ;;
    --phase-kind) PHASE_KIND="$2"; shift 2 ;;
    --skip-inputs) SKIP_INPUTS=1; shift ;;
    --check-recovery) CHECK_RECOVERY=1; shift ;;
    *) echo "Unknown arg: $1" >&2; exit 2 ;;
  esac
done

[ -z "$INPUTS_DIR" ] && INPUTS_DIR="$OUT_DIR/inputs"

[ "$MODES" = "both" ] && MODES="native,docker"
if [ "$THREADS" = "auto" ]; then
  THREADS=$(nproc)
fi
TOTAL_MEM_MB=$(awk '/MemTotal/{print int($2/1024)}' /proc/meminfo)
if [ "$MEM_MAX_MB" = "auto" ]; then
  MEM_MAX_MB=$((TOTAL_MEM_MB - 400))
fi

mkdir -p "$OUT_DIR"
rm -f "$OUT_DIR/stop"
echo "none" > "$OUT_DIR/target"
echo "" > "$OUT_DIR/cgroup_path"
echo "init" > "$OUT_DIR/phase"

NEIGHBOUR_PID=""
SAMPLER_PID=""
cleanup() {
  stop_all_engines "$OUT_DIR" || true
  [ -n "$NEIGHBOUR_PID" ] && kill "$NEIGHBOUR_PID" 2>/dev/null || true
  kill_port_pid 3100 || true
  touch "$OUT_DIR/stop"
  [ -n "$SAMPLER_PID" ] && wait "$SAMPLER_PID" 2>/dev/null || true
  rm -f /tmp/vps26_cluaiz.token
}
trap cleanup EXIT

echo "=== Write manifest (no IP/hostname/secrets) ==="
python3 - "$OUT_DIR/manifest.json" "$MODES" "$ENGINES" "$MODELS" "$W6_MODEL" "$WORKLOADS" "$TOTAL_MEM_MB" "$MEM_MAX_MB" "$THREADS" "$CTX" "$KV" "$SLOTS" "$LEVELS" "$STEP_SECS" "$SOAK_SECS" "$W4_LIMIT" "$OLLAMA_IMAGE" <<'PY'
import hashlib, json, os, platform, subprocess, sys, time
out = sys.argv[1]
def cmd(c):
    try:
        return subprocess.check_output(c, shell=True, text=True, stderr=subprocess.STDOUT).strip()
    except Exception as e:
        return f"err: {e}"
def sha(p):
    try:
        h = hashlib.sha256()
        with open(p, "rb") as f:
            for ch in iter(lambda: f.read(1048576), b""):
                h.update(ch)
        return h.hexdigest()
    except Exception:
        return None
manifest = {
    "created_at": time.strftime("%Y-%m-%dT%H:%M:%S%z"),
    "machine": {
        "cpu_model": cmd("lscpu | awk -F: '/Model name/{gsub(/^ +/,\"\",$2); print $2; exit}'"),
        "vcpus": os.cpu_count(),
        "mem_total_mb": int(sys.argv[7]),
        "kernel": platform.release(),
        "os": cmd(". /etc/os-release && echo $PRETTY_NAME"),
    },
    "params": {
        "modes": sys.argv[2].split(","),
        "engines": sys.argv[3].split(","),
        "models": sys.argv[4].split(","),
        "w6_model": sys.argv[5],
        "workloads": sys.argv[6].split(","),
        "mem_max_mb": int(sys.argv[8]),
        "threads": int(sys.argv[9]),
        "ctx": int(sys.argv[10]),
        "kv": sys.argv[11],
        "slots": int(sys.argv[12]),
        "levels": [int(x) for x in sys.argv[13].split(",")],
        "step_secs": int(sys.argv[14]),
        "soak_secs": int(sys.argv[15]),
        "w4_limit": int(sys.argv[16]),
    },
    "engines": {
        "cluaiz_version": cmd("CLUAIZ_HOME=/root/vps26-cluaiz-home /root/.cluaiz/bin/cluaiz --version | head -1"),
        "cluaiz_sha256": sha("/root/.cluaiz/bin/cluaiz"),
        # Set CLUAIZ_COMMIT to the commit the binary was built from (run 26 did not record it)
        "cluaiz_commit": os.environ.get("CLUAIZ_COMMIT"),
        "ollama_version": cmd("/root/ollama-official/bin/ollama --version | tail -1"),
        "ollama_sha256": sha("/root/ollama-official/bin/ollama"),
        "llama_version": cmd("LD_LIBRARY_PATH=/root/llama-official/llama-b12428 /root/llama-official/llama-b12428/llama-server --version 2>&1 | head -2"),
        "llama_sha256": sha("/root/llama-official/llama-b12428/llama-server"),
        "ollama_image": sys.argv[17],
        "docker_images": cmd("docker images --digests --format '{{.Repository}}:{{.Tag}} {{.ID}} {{.Digest}} {{.Size}}'"),
    },
}
with open(out, "w") as f:
    json.dump(manifest, f, indent=2)
print(json.dumps(manifest, indent=2))
PY

if [ "$SKIP_INPUTS" -eq 0 ] && [ ! -f "$INPUTS_DIR/w1_tickets.json" ]; then
  python3 "$HERE/generate_inputs.py" --out-dir "$(dirname "$INPUTS_DIR")"
fi

echo "=== Sync engine configs ==="
configure_cluaiz_home "$THREADS" "$CTX" "$KV"
sync_ollama_modelfiles "$THREADS" "$CTX"

echo "=== Start neighbour app (port 3100) and per-second sampler ==="
kill_port_pid 3100 || true
python3 -u "$HERE/neighbour_app.py" --port 3100 --dbname neighbour_app > "$OUT_DIR/neighbour_app.log" 2>&1 &
NEIGHBOUR_PID=$!
wait_http "http://127.0.0.1:3100/health" 20

python3 -u "$HERE/sampler.py" --out-dir "$OUT_DIR" > "$OUT_DIR/sampler.log" 2>&1 &
SAMPLER_PID=$!

echo "baseline_with_neighbour" > "$OUT_DIR/phase"
sleep 15

IFS=',' read -r -a MODE_ARR <<< "$MODES"
IFS=',' read -r -a ENG_ARR <<< "$ENGINES"
IFS=',' read -r -a MOD_ARR <<< "$MODELS"
IFS=',' read -r -a WL_ARR <<< "$WORKLOADS"

for mode in "${MODE_ARR[@]}"; do
  for eng in "${ENG_ARR[@]}"; do
    for wl in "${WL_ARR[@]}"; do
      if [ "$wl" = "W1" ] || [ "$wl" = "W2" ] || [ "$wl" = "W3" ]; then
        for mkey in "${MOD_ARR[@]}"; do
          rec_arg=""
          if [ "$CHECK_RECOVERY" -eq 1 ] && [ "$wl" = "W1" ] && [ "$mkey" = "${MOD_ARR[0]}" ]; then
            rec_arg="--check-recovery"
          fi
          echo "== START $mode / $eng / $wl / $mkey =="
          echo "${mode}_${eng}_${wl}_${mkey}_start" > "$OUT_DIR/phase"
          if start_engine "$mode" "$eng" "$OUT_DIR" "$MEM_MAX_MB" "$THREADS" "$CTX" "$KV" "$SLOTS" "$mkey" "none" "$OLLAMA_IMAGE"; then
            python3 -u "$HERE/workloads.py" \
              --mode "$mode" --engine "$eng" --model-key "$mkey" --workload "$wl" \
              --inputs-dir "$INPUTS_DIR" --out-dir "$OUT_DIR" \
              --levels "$LEVELS" --step-secs "$STEP_SECS" --soak-secs "$SOAK_SECS" \
              --threads "$THREADS" --ctx "$CTX" --w4-limit "$W4_LIMIT" \
              --phase-kind "$PHASE_KIND" \
              --cluaiz-token-file /tmp/vps26_cluaiz.token $rec_arg || true
          else
            echo "FAILED TO START $mode $eng $wl $mkey"
          fi
          stop_all_engines "$OUT_DIR"
          journalctl -k --since "-10 minutes" --no-pager 2>/dev/null | grep -iE "oom|killed process" >> "$OUT_DIR/oom_events.log" || true
          sleep 3
        done

      elif [ "$wl" = "W4" ]; then
        echo "== START $mode / $eng / W4 =="
        echo "${mode}_${eng}_W4_start" > "$OUT_DIR/phase"
        if start_engine "$mode" "$eng" "$OUT_DIR" "$MEM_MAX_MB" "$THREADS" "$CTX" "$KV" "$SLOTS" "none" "embed-gemma" "$OLLAMA_IMAGE"; then
          python3 -u "$HERE/workloads.py" \
            --mode "$mode" --engine "$eng" --model-key "embed-gemma" --workload W4 \
            --inputs-dir "$INPUTS_DIR" --out-dir "$OUT_DIR" \
            --levels "$LEVELS" --step-secs "$STEP_SECS" --soak-secs 0 \
            --threads "$THREADS" --ctx "$CTX" --w4-limit "$W4_LIMIT" \
            --cluaiz-token-file /tmp/vps26_cluaiz.token || true
        else
          echo "FAILED TO START $mode $eng W4"
        fi
        stop_all_engines "$OUT_DIR"
        journalctl -k --since "-10 minutes" --no-pager 2>/dev/null | grep -iE "oom|killed process" >> "$OUT_DIR/oom_events.log" || true
        sleep 3

      elif [ "$wl" = "W5" ]; then
        if [ "$eng" != "cluaiz" ]; then
          python3 -u "$HERE/workloads.py" \
            --mode "$mode" --engine "$eng" --model-key "speech" --workload W5 \
            --inputs-dir "$INPUTS_DIR" --out-dir "$OUT_DIR" \
            --levels "$LEVELS" --step-secs "$STEP_SECS" --soak-secs 0 \
            --threads "$THREADS" --ctx "$CTX" || true
        else
          echo "== START $mode / $eng / W5 =="
          echo "${mode}_${eng}_W5_start" > "$OUT_DIR/phase"
          if start_engine "$mode" "$eng" "$OUT_DIR" "$MEM_MAX_MB" "$THREADS" "$CTX" "$KV" "$SLOTS" "kokoro" "none" "$OLLAMA_IMAGE"; then
            python3 -u "$HERE/workloads.py" \
              --mode "$mode" --engine "$eng" --model-key "speech" --workload W5 \
              --inputs-dir "$INPUTS_DIR" --out-dir "$OUT_DIR" \
              --levels "$LEVELS" --step-secs "$STEP_SECS" --soak-secs 0 \
              --threads "$THREADS" --ctx "$CTX" \
              --cluaiz-token-file /tmp/vps26_cluaiz.token || true
          fi
          stop_all_engines "$OUT_DIR"
          journalctl -k --since "-10 minutes" --no-pager 2>/dev/null | grep -iE "oom|killed process" >> "$OUT_DIR/oom_events.log" || true
          sleep 3
        fi

      elif [ "$wl" = "W6" ]; then
        echo "== START $mode / $eng / W6 ($W6_MODEL) =="
        echo "${mode}_${eng}_W6_start" > "$OUT_DIR/phase"
        if start_engine "$mode" "$eng" "$OUT_DIR" "$MEM_MAX_MB" "$THREADS" "$CTX" "$KV" "$SLOTS" "$W6_MODEL" "embed-gemma" "$OLLAMA_IMAGE"; then
          python3 -u "$HERE/workloads.py" \
            --mode "$mode" --engine "$eng" --model-key "$W6_MODEL" --workload W6 \
            --inputs-dir "$INPUTS_DIR" --out-dir "$OUT_DIR" \
            --levels "$LEVELS" --step-secs "$STEP_SECS" --soak-secs "$SOAK_SECS" \
            --threads "$THREADS" --ctx "$CTX" \
            --phase-kind "$PHASE_KIND" \
            --cluaiz-token-file /tmp/vps26_cluaiz.token || true
        else
          echo "FAILED TO START $mode $eng W6"
        fi
        stop_all_engines "$OUT_DIR"
        journalctl -k --since "-10 minutes" --no-pager 2>/dev/null | grep -iE "oom|killed process" >> "$OUT_DIR/oom_events.log" || true
        sleep 3
      fi
    done
  done
done

echo "cooldown" > "$OUT_DIR/phase"
sleep 5
touch "$OUT_DIR/stop"
wait "$SAMPLER_PID" 2>/dev/null || true
SAMPLER_PID=""
kill "$NEIGHBOUR_PID" 2>/dev/null || true
NEIGHBOUR_PID=""

if [ -f /root/results-run26/docker_baseline.json ] && [ ! -f "$OUT_DIR/docker_baseline.json" ]; then
  cp /root/results-run26/docker_baseline.json "$OUT_DIR/docker_baseline.json"
fi

python3 "$HERE/analyze.py" --out-dir "$OUT_DIR"
echo "ALL DONE: $OUT_DIR"
