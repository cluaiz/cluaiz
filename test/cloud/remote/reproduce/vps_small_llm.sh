#!/usr/bin/env bash
# Short, one-command version of run 19 / run 26: a small LLM next to a website with PostgreSQL on a
# small Ubuntu 24.04 machine (1-2 vCPU, 4 GB RAM). Not run end to end yet: see README.md in this folder.
#
#   sudo bash vps_small_llm.sh [options]
#
#   --model 0.6b|1.7b     Qwen3 model, Q4_K_M (default 0.6b)
#   --compare             also run official llama.cpp and Ollama, same settings (about +35 minutes)
#   --seq-secs N          1 request at a time, N seconds (default 600)
#   --queue-secs N        queue phase, N seconds (default 300)
#   --queue-level N       requests in flight in the queue phase, 2 or more (default 8)
#   --threads N           CPU threads for the engines (default 1, as in run 26)
#   --out-dir DIR         results folder (default ./vps_small_llm_results_<date>)
#   --keep-work           keep the downloaded engines and the model after the run
#   --skip-os-check       run on something other than Ubuntu 24.04 x86_64 (the pins below are for x86_64)
#   --help
#
# What it changes on the machine: installs apt packages (below), creates one PostgreSQL database and,
# if missing, one PostgreSQL role named root; opens ports 3100, 8011, 8012, 11435 on 127.0.0.1 only;
# writes under one temporary folder and the results folder. On exit it stops everything it started and
# drops the database and the role it created. The apt packages stay installed.
# No account, API key or token is needed. It never prints the engine's session token.
set -euo pipefail

HERE="$(cd "$(dirname "$0")" && pwd)"
VPS26="$HERE/../vps26"

# ---------------------------------------------------------------------------------------------
# Pins. Every download is checked against the sha256 below; a mismatch stops the run.
# Sources: GitHub release "digest" fields and Hugging Face LFS ids, read 2026-11-24.
# ---------------------------------------------------------------------------------------------
CLUAIZ_TAG="v0.1.0"
export CLUAIZ_TAG
CLUAIZ_ASSET="cluaiz-0.1.0-linux-x64.tar.gz"
CLUAIZ_URL="https://github.com/cluaiz/cluaiz/releases/download/$CLUAIZ_TAG/$CLUAIZ_ASSET"
CLUAIZ_SHA256="f37957bc5413eccbd746a56392cb53063ecb7d797d329f87389439bf65cf29ce"

LLAMA_TAG="b12428"
LLAMA_ASSET="llama-b12428-bin-ubuntu-x64.tar.gz"
LLAMA_URL="https://github.com/ggml-org/llama.cpp/releases/download/$LLAMA_TAG/$LLAMA_ASSET"
LLAMA_SHA256="a9b243a3bc4ca172010e96c64062fc02afd1dc4f8b699716f813ae6845ada4c6"

OLLAMA_TAG="v0.40.16"
OLLAMA_ASSET="ollama-linux-amd64.tar.zst"
OLLAMA_URL="https://github.com/ollama/ollama/releases/download/$OLLAMA_TAG/$OLLAMA_ASSET"
OLLAMA_SHA256="87bfafb2acbc89b2e6a1be0a9043de6aa75fb450696f86bc426e65fc448f788d"

# Models: official Hugging Face repo, file name, sha256 of the file (Hugging Face LFS id at the repo
# revision named in the comment), and the name `cluaiz pull` takes. Cluaiz downloads the file itself
# (the same path the release smoke test uses); this script then checks it against the pin.
#   unsloth/Qwen3-0.6B-GGUF revision 50968a4468ef4233ed78cd7c3de230dd1d61a56b
#   unsloth/Qwen3-1.7B-GGUF revision d7f544eead698dbd1f15126ef60b45a1e1933222
declare -A MODEL_FILE=( [qwen3-0.6b]="Qwen3-0.6B-Q4_K_M.gguf" [qwen3-1.7b]="Qwen3-1.7B-Q4_K_M.gguf" )
declare -A MODEL_SHA256=(
  [qwen3-0.6b]="ac2d97712095a558e31573f62f466a3f9d93990898b0ec79d7c974c1780d524a"
  [qwen3-1.7b]="b139949c5bd74937ad8ed8c8cf3d9ffb1e99c866c823204dc42c0d91fa181897" )
declare -A MODEL_PULL=( [qwen3-0.6b]="unsloth/Qwen3-0.6B-GGUF:Q4_K_M" [qwen3-1.7b]="unsloth/Qwen3-1.7B-GGUF:Q4_K_M" )

# Same ubuntu archive for everyone on 24.04; the installed versions are written to versions.txt.
APT_PACKAGES=(ca-certificates curl tar zstd libgomp1 iproute2 procps util-linux postgresql python3 python3-psycopg2)

# ---------------------------------------------------------------------------------------------
# Options
# ---------------------------------------------------------------------------------------------
MODEL="qwen3-0.6b"
COMPARE=0
SEQ_SECS=600
QUEUE_SECS=300
QUEUE_LEVEL=8
THREADS=1
CTX=4096
KV="q8_0"
SLOTS=1
OUT=""
KEEP_WORK=0
SKIP_OS_CHECK=0

die() { echo "ERROR: $*" >&2; exit 1; }

while [ "$#" -gt 0 ]; do
  case "$1" in
    --model)
      case "${2:-}" in 0.6b) MODEL="qwen3-0.6b" ;; 1.7b) MODEL="qwen3-1.7b" ;; *) die "--model takes 0.6b or 1.7b" ;; esac
      shift 2 ;;
    --compare) COMPARE=1; shift ;;
    --seq-secs) SEQ_SECS="${2:?}"; shift 2 ;;
    --queue-secs) QUEUE_SECS="${2:?}"; shift 2 ;;
    --queue-level) QUEUE_LEVEL="${2:?}"; shift 2 ;;
    --threads) THREADS="${2:?}"; shift 2 ;;
    --out-dir) OUT="${2:?}"; shift 2 ;;
    --keep-work) KEEP_WORK=1; shift ;;
    --skip-os-check) SKIP_OS_CHECK=1; shift ;;
    --help|-h) sed -n '2,/^set -euo/p' "$0" | sed '$d' | sed 's/^# \{0,1\}//'; exit 0 ;;
    *) die "unknown option: $1 (see --help)" ;;
  esac
done
for v in SEQ_SECS QUEUE_SECS QUEUE_LEVEL THREADS; do
  [[ "${!v}" =~ ^[0-9]+$ ]] && [ "${!v}" -ge 1 ] || die "$v must be a whole number of 1 or more"
done
[ "$QUEUE_LEVEL" -ge 2 ] || die "--queue-level must be 2 or more (level 1 is the other phase)"

[ -z "$OUT" ] && OUT="$PWD/vps_small_llm_results_$(date -u +%Y%m%d_%H%M%S)"
mkdir -p "$OUT"
OUT="$(cd "$OUT" && pwd)"

# ---------------------------------------------------------------------------------------------
# Preflight: refuse early, before anything is installed or started
# ---------------------------------------------------------------------------------------------
[ "$(id -u)" -eq 0 ] || die "run as root: sudo bash $0"
[ -f "$VPS26/engines.sh" ] && [ -f "$VPS26/workloads.py" ] || die "vps26 scripts not found at $VPS26 (use a full checkout of the repository)"
if [ "$SKIP_OS_CHECK" -eq 0 ]; then
  [ "$(uname -s)" = Linux ] && [ "$(uname -m)" = x86_64 ] || die "needs Linux x86_64 (the pinned downloads are x86_64)"
  # shellcheck disable=SC1091
  . /etc/os-release
  [ "${ID:-}" = ubuntu ] && [ "${VERSION_ID:-}" = "24.04" ] || die "needs Ubuntu 24.04 (found ${PRETTY_NAME:-unknown}); --skip-os-check overrides"
fi
TOTAL_MEM_MB=$(awk '/MemTotal/{print int($2/1024)}' /proc/meminfo)
[ "$TOTAL_MEM_MB" -ge 2500 ] || die "needs at least 2.5 GB RAM (found $TOTAL_MEM_MB MB); the target is 4 GB"
MEM_MAX_MB=$((TOTAL_MEM_MB - 400))   # same rule as run26.sh
command -v systemd-run >/dev/null || die "systemd-run not found: the engines run in a memory-limited systemd scope, as in run 26"
systemd-run --scope --quiet -p MemoryMax=200M true 2>/dev/null || die "systemd-run --scope does not work here (a container without systemd?)"
# The vps26 helpers kill whatever listens on these ports, so a busy port stops the run instead
for port in 3100 8011 8012 8013 11435; do
  if ss -ltnH "sport = :$port" 2>/dev/null | grep -q .; then die "port $port is already in use; stop that service first"; fi
done
NEED_GB=4; [ "$COMPARE" -eq 1 ] && NEED_GB=8
WORK_PARENT=/var/tmp
FREE_GB=$(df -Pk "$WORK_PARENT" | awk 'NR==2{print int($4/1048576)}')
[ "$FREE_GB" -ge "$NEED_GB" ] || die "needs about $NEED_GB GB free in $WORK_PARENT (found $FREE_GB GB)"

WORK="$(mktemp -d "$WORK_PARENT/cluaiz-repro.XXXXXX")"
export HOME="$WORK/home"   # keeps engine state (for example Ollama's key) out of the real home
mkdir -p "$HOME"

# ---------------------------------------------------------------------------------------------
# Reuse the vps26 helpers (start_engine, stop_all_engines, wait_http, ...), then point them at this
# machine's folders. The helpers assume /root/... paths from the run 26 VPS, so those are overridden.
# ---------------------------------------------------------------------------------------------
# shellcheck source=../vps26/engines.sh
. "$VPS26/engines.sh"
CLUAIZ_BIN="$WORK/cluaiz/cluaiz"
CLUAIZ_HOME_NATIVE="$WORK/cluaiz-home"
OLLAMA_BIN="$WORK/ollama/bin/ollama"
OLLAMA_MODELS_DIR="$WORK/ollama-models"
LLAMA_DIR="$WORK/llama/llama-$LLAMA_TAG"
LLAMA_BIN="$LLAMA_DIR/llama-server"
MODEL_GGUF=""
# The helper loops over four model keys; keys without a file here must still answer (with a path that
# does not exist) so its "skip missing GGUF" branch runs.
model_gguf_host() {
  if [ "$1" = "$MODEL" ] && [ -n "$MODEL_GGUF" ]; then echo "$MODEL_GGUF"; else echo "/nonexistent/$1"; fi
}
# Same settings as engines.sh configure_cluaiz_home, minus its /root symlinks and Docker copy
configure_cluaiz_repro() {
  local kv_enum k v
  kv_enum=$(cluaiz_kv_enum "$KV")
  mkdir -p "$CLUAIZ_HOME_NATIVE"
  while read -r k v; do
    CLUAIZ_HOME="$CLUAIZ_HOME_NATIVE" CLUAIZ_NONINTERACTIVE=1 "$CLUAIZ_BIN" config set "$k" "$v" >/dev/null </dev/null
  done <<EOF
optimization.custom_context_window $CTX
optimization.n_gpu_layers 0
optimization.n_threads $THREADS
optimization.n_threads_batch $THREADS
optimization.kv_cache_quantization $kv_enum
optimization.flash_attention Auto
optimization.think_mode off
chat_memory.enabled false
EOF
}

# ---------------------------------------------------------------------------------------------
# Cleanup: runs on every exit
# ---------------------------------------------------------------------------------------------
SITE_DB="cluaiz_repro_site"
DB_CREATED=0
ROLE_CREATED=0
NEIGHBOUR_PID=""
SAMPLER_PID=""
cleanup() {
  local rc=$?
  set +e
  stop_all_engines "$OUT" >/dev/null 2>&1
  touch "$OUT/stop"
  [ -n "$SAMPLER_PID" ] && wait "$SAMPLER_PID" 2>/dev/null
  [ -n "$NEIGHBOUR_PID" ] && kill "$NEIGHBOUR_PID" 2>/dev/null
  [ -n "$NEIGHBOUR_PID" ] && wait "$NEIGHBOUR_PID" 2>/dev/null
  if [ "$DB_CREATED" -eq 1 ]; then runuser -u postgres -- dropdb --if-exists "$SITE_DB" >/dev/null 2>&1; fi
  if [ "$ROLE_CREATED" -eq 1 ]; then runuser -u postgres -- psql -qc "DROP ROLE IF EXISTS root" >/dev/null 2>&1; fi
  rm -f /tmp/vps26_cluaiz.token
  if [ "$KEEP_WORK" -eq 0 ]; then rm -rf "$WORK"; else echo "kept: $WORK"; fi
  [ "$rc" -eq 0 ] || echo "Stopped with an error (exit $rc). Raw files so far: $OUT" >&2
  exit "$rc"
}
trap cleanup EXIT
trap 'exit 130' INT TERM

fetch() {  # fetch <url> <dest> <sha256>: https only, checksum checked before anything is unpacked
  local url="$1" dest="$2" sha="$3"
  echo "download: ${url##*/}"
  curl -fL --proto '=https' --tlsv1.2 --retry 3 --retry-delay 5 --connect-timeout 20 -sS -o "$dest" "$url"
  echo "$sha  $dest" | sha256sum -c --status - || die "checksum mismatch for ${url##*/} (expected $sha); nothing was run"
}

# ---------------------------------------------------------------------------------------------
echo "=== 1/7 Install packages (apt) ==="
export DEBIAN_FRONTEND=noninteractive
apt-get update -q >"$OUT/apt.log" 2>&1
apt-get install -yq --no-install-recommends "${APT_PACKAGES[@]}" >>"$OUT/apt.log" 2>&1 || { tail -5 "$OUT/apt.log" >&2; die "apt install failed (full log: $OUT/apt.log)"; }
dpkg-query -W -f='${Package} ${Version}\n' "${APT_PACKAGES[@]}" postgresql-16 2>/dev/null >"$OUT/versions.txt" || true

echo "=== 2/7 Download and check Cluaiz $CLUAIZ_TAG (with --compare also llama.cpp $LLAMA_TAG and Ollama $OLLAMA_TAG) ==="
mkdir -p "$WORK/dl" "$WORK/cluaiz" "$WORK/llama" "$WORK/ollama"
fetch "$CLUAIZ_URL" "$WORK/dl/$CLUAIZ_ASSET" "$CLUAIZ_SHA256"
tar -xzf "$WORK/dl/$CLUAIZ_ASSET" -C "$WORK/cluaiz"
rm -f "$WORK/dl/$CLUAIZ_ASSET"
[ -x "$CLUAIZ_BIN" ] || die "the Cluaiz archive has no cluaiz binary"
CLUAIZ_VERSION_LINE=$(CLUAIZ_HOME="$CLUAIZ_HOME_NATIVE" "$CLUAIZ_BIN" --version 2>&1 | tail -1)
ENGINES="cluaiz"
if [ "$COMPARE" -eq 1 ]; then
  fetch "$LLAMA_URL" "$WORK/dl/$LLAMA_ASSET" "$LLAMA_SHA256"
  tar -xzf "$WORK/dl/$LLAMA_ASSET" -C "$WORK/llama"; rm -f "$WORK/dl/$LLAMA_ASSET"
  [ -x "$LLAMA_BIN" ] || die "the llama.cpp archive has no llama-$LLAMA_TAG/llama-server"
  fetch "$OLLAMA_URL" "$WORK/dl/$OLLAMA_ASSET" "$OLLAMA_SHA256"
  tar --zstd -xf "$WORK/dl/$OLLAMA_ASSET" -C "$WORK/ollama"; rm -f "$WORK/dl/$OLLAMA_ASSET"
  [ -x "$OLLAMA_BIN" ] || die "the Ollama archive has no bin/ollama"
  ENGINES="cluaiz,llama,ollama"
fi

echo "=== 3/7 Download the model with Cluaiz and check its sha256 ==="
configure_cluaiz_repro
PULL_LOG="$OUT/pull.log"
CLUAIZ_HOME="$CLUAIZ_HOME_NATIVE" CLUAIZ_NONINTERACTIVE=1 "$CLUAIZ_BIN" pull "${MODEL_PULL[$MODEL]}" >"$PULL_LOG" 2>&1 \
  || { tail -5 "$PULL_LOG" >&2; die "cluaiz pull failed (log: $PULL_LOG)"; }
REG_ID=$(awk '/Registered ID:/ { gsub(/\033\[[0-9;]*m|\r/, ""); print $NF }' "$PULL_LOG")
[ "$REG_ID" = "$(model_cluaiz_id "$MODEL")" ] || die "Cluaiz registered '$REG_ID', the harness expects '$(model_cluaiz_id "$MODEL")'"
MODEL_GGUF=$(find "$CLUAIZ_HOME_NATIVE/models" -type f -name "${MODEL_FILE[$MODEL]}" | head -1)
[ -n "$MODEL_GGUF" ] || die "model file ${MODEL_FILE[$MODEL]} not found under $CLUAIZ_HOME_NATIVE/models"
echo "${MODEL_SHA256[$MODEL]}  $MODEL_GGUF" | sha256sum -c --status - \
  || die "model checksum mismatch for ${MODEL_FILE[$MODEL]} (pinned ${MODEL_SHA256[$MODEL]}): the Hugging Face file changed or the download is damaged"
echo "model ok: ${MODEL_FILE[$MODEL]}"

echo "=== 4/7 Start the website neighbour (PostgreSQL + neighbour_app.py) ==="
runuser -u postgres -- psql -tAc "SELECT 1 FROM pg_database WHERE datname='$SITE_DB'" | grep -q 1 && die "database $SITE_DB already exists; remove it first"
if ! runuser -u postgres -- psql -tAc "SELECT 1 FROM pg_roles WHERE rolname='root'" | grep -q 1; then
  runuser -u postgres -- psql -qc "CREATE ROLE root LOGIN"; ROLE_CREATED=1
fi
runuser -u postgres -- createdb -O root "$SITE_DB"; DB_CREATED=1
# Tables and rows as in vps26/webapp/server.js setup(); neighbour_app.py itself creates nothing
psql -q -d "$SITE_DB" -v ON_ERROR_STOP=1 <<'SQL'
CREATE TABLE items (id serial PRIMARY KEY, category text NOT NULL, payload text NOT NULL, updated_at timestamptz NOT NULL DEFAULT now());
CREATE INDEX idx_items_category ON items (category);
CREATE TABLE pings (id serial PRIMARY KEY, ts timestamptz NOT NULL DEFAULT now(), note text NOT NULL);
INSERT INTO items (category, payload) SELECT (ARRAY['orders','users','billing','events','audit'])[1 + g % 5], repeat(md5(g::text), 4) FROM generate_series(1, 5000) g;
SQL
python3 -u "$VPS26/neighbour_app.py" --port 3100 --dbname "$SITE_DB" >"$OUT/neighbour_app.log" 2>&1 &
NEIGHBOUR_PID=$!
wait_http "http://127.0.0.1:3100/health" 40 || die "the website did not answer (see $OUT/neighbour_app.log)"

echo "=== 5/7 Write machine facts, start the per-second sampler, measure the website alone for 30 s ==="
python3 - "$OUT/machine.json" "$TOTAL_MEM_MB" "$MODEL" "$THREADS" "$CTX" "$KV" "$SLOTS" "$SEQ_SECS" "$QUEUE_SECS" "$QUEUE_LEVEL" \
  "$CLUAIZ_VERSION_LINE" "$CLUAIZ_SHA256" "$LLAMA_TAG" "$OLLAMA_TAG" "$COMPARE" <<'PY'
import json, os, platform, subprocess, sys, time
a = sys.argv
def cmd(c):
    try:
        return subprocess.check_output(c, shell=True, text=True, stderr=subprocess.STDOUT).strip()
    except Exception as e:
        return f"err: {e}"
engines = {"cluaiz": f"{a[11]} (release {os.environ.get('CLUAIZ_TAG', '')}, archive sha256 {a[12]})"}
if a[15] == "1":
    engines["llama"] = f"llama.cpp {a[13]}"
    engines["ollama"] = f"Ollama {a[14]}"
m = {
    "created_at": time.strftime("%Y-%m-%dT%H:%M:%S%z"),
    "machine": {
        "cpu_model": cmd("lscpu | awk -F: '/Model name/{gsub(/^ +/,\"\",$2); print $2; exit}'"),
        "vcpus": os.cpu_count(), "mem_total_mb": int(a[2]), "kernel": platform.release(),
        "os": cmd(". /etc/os-release && echo $PRETTY_NAME"),
    },
    "params": {"model": a[3], "threads": int(a[4]), "ctx": int(a[5]), "kv": a[6], "slots": int(a[7]),
               "seq_secs": int(a[8]), "queue_secs": int(a[9]), "queue_level": int(a[10])},
    "engines": engines,
}
json.dump(m, open(a[1], "w"), indent=2)
PY
echo "none" > "$OUT/target"; echo "" > "$OUT/cgroup_path"; echo "base" > "$OUT/phase"; rm -f "$OUT/stop"
python3 -u "$VPS26/sampler.py" --out-dir "$OUT" >"$OUT/sampler.log" 2>&1 &
SAMPLER_PID=$!
python3 "$VPS26/generate_inputs.py" --out-dir "$OUT" >/dev/null
sleep 30

mark() { echo "$1 $(head -1 /proc/stat)" >> "$OUT/cpu_stat.log"; }
run_block() {
  local eng="$1"
  echo "=== 6/7 $eng: start, warm up, $SEQ_SECS s at 1 request at a time, $QUEUE_SECS s with $QUEUE_LEVEL in flight ==="
  mark "${eng}_begin"
  echo "${eng}_start" > "$OUT/phase"
  if ! start_engine native "$eng" "$OUT" "$MEM_MAX_MB" "$THREADS" "$CTX" "$KV" "$SLOTS" "$MODEL" none "ollama/ollama:${OLLAMA_TAG#v}"; then
    echo "$eng did not start (see $OUT/engine_native_$eng.log)" >&2
    stop_all_engines "$OUT"; mark "${eng}_end"; return 0
  fi
  local common=(--mode native --engine "$eng" --model-key "$MODEL" --workload W1 --inputs-dir "$OUT/inputs"
                --threads "$THREADS" --ctx "$CTX" --soak-secs 0 --cluaiz-token-file /tmp/vps26_cluaiz.token)
  # Warm-up in its own folder so the first request (model load) is not in the measured numbers.
  echo "${eng}_warmup" > "$OUT/phase"
  python3 -u "$VPS26/workloads.py" "${common[@]}" --out-dir "$OUT/warmup_$eng" --levels 1 --step-secs 15 >/dev/null || true
  # workloads.py names the phase itself (native_<engine>_W1_<model>_ramp_c<level>) while it runs
  python3 -u "$VPS26/workloads.py" "${common[@]}" --out-dir "$OUT" --levels 1 --step-secs "$SEQ_SECS" || true
  python3 -u "$VPS26/workloads.py" "${common[@]}" --out-dir "$OUT" --levels "$QUEUE_LEVEL" --step-secs "$QUEUE_SECS" || true
  echo "${eng}_stop" > "$OUT/phase"
  stop_all_engines "$OUT"
  journalctl -k --since "-45 minutes" --no-pager 2>/dev/null | grep -iE "oom|killed process" >> "$OUT/oom_events.log" || true
  mark "${eng}_end"
  sleep 3
}

# Ollama needs its models created from the GGUF once (the helper skips keys with no file)
if [ "$COMPARE" -eq 1 ]; then sync_ollama_modelfiles "$THREADS" "$CTX"; fi
IFS=',' read -r -a ENG_ARR <<< "$ENGINES"
for eng in "${ENG_ARR[@]}"; do run_block "$eng"; done

echo "=== 7/7 Result ==="
echo "cooldown" > "$OUT/phase"
touch "$OUT/stop"
wait "$SAMPLER_PID" 2>/dev/null || true
SAMPLER_PID=""
python3 "$VPS26/analyze.py" --out-dir "$OUT" >/dev/null
# The server's session token must not sit in files people share: redact it if any log holds it
if [ -s /tmp/vps26_cluaiz.token ]; then
  python3 - "$OUT" <<'PY'
import pathlib, sys
tok = pathlib.Path("/tmp/vps26_cluaiz.token").read_text().strip()
if len(tok) >= 8:
    for p in pathlib.Path(sys.argv[1]).rglob("*"):
        if p.is_file() and p.stat().st_size < 50_000_000:
            try:
                s = p.read_text(errors="strict")
            except (UnicodeDecodeError, OSError):
                continue
            if tok in s:
                p.write_text(s.replace(tok, "<redacted>"))
                print(f"redacted the session token in {p.name}")
PY
fi
python3 "$HERE/table.py" --out-dir "$OUT"
echo "Raw files: $OUT"
