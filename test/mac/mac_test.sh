#!/usr/bin/env bash
# Cluaiz on a Mac with Apple Silicon, in about an hour: the release installed by the website's one command,
# then test/engine_regression/run.py against plain llama.cpp (llama-bench built with Metal from the same
# llama.cpp commit as the release), every model and placement mode, and every log packed into one archive
# on the Desktop. Everything else stays in ~/cluaiz-mactest (delete it afterwards).
#
# Usage, in Terminal with the charger plugged in (not "curl | bash": the commands inside would read the
# rest of the script from that pipe):
#   bash -c "$(curl -fsSL https://raw.githubusercontent.com/cluaiz/cluaiz/main/test/mac/mac_test.sh)"
#   bash mac_test.sh        (a copy; run.py is taken from beside it, else downloaded)
# Environment (optional):
#   BIG=1                 also Gemma 4 31B Q4 (17 GB) when it is bigger than the RAM: read from the SSD
#   RUNS=2                requests per model and mode (default 3; the first is the cold one)
#   CLUAIZ_VERSION=0.1.1  release to install (default: the newest, pre-releases included)
set -uo pipefail
# "$0" is "bash" when the script comes from curl: then nothing is beside it
HERE="$(cd "$(dirname "$0")" 2>/dev/null && pwd)"
W="$HOME/cluaiz-mactest"
OUT="$W/results"
BIN="$W/bin/cluaiz"
LLAMA_COMMIT="${LLAMA_COMMIT:-8f9842cd1faebd06413805eb495bdf1c7e80660f}" # engine/llama/llama.cpp of the release
mkdir -p "$OUT"
exec > >(tee -a "$OUT/mac_test.log") 2>&1
T0=$(date +%s)
STEPS=8
N=0
elapsed() { local s=$(( $(date +%s) - T0 )); printf '%d:%02d' $(( s / 60 )) $(( s % 60 )); }
step() { N=$(( N + 1 )); echo; echo "== [step $N of $STEPS, $(elapsed) elapsed] $*"; }
note() { echo "   $*"; }

# The archive is written however the run ends (also after Ctrl+C)
pack() {
  [ -n "${SERVER:-}" ] && kill "$SERVER" 2>/dev/null
  [ -n "${BUILD:-}" ] && kill "$BUILD" 2>/dev/null
  local archive="$HOME/Desktop/cluaiz-mac-results-$(date +%Y%m%d-%H%M).tar.gz"
  tar -czf "$archive" -C "$W" results 2>/dev/null && echo "== results: $archive (send this file)"
}
trap pack EXIT

# ── 1. This Mac ─────────────────────────────────────────────────────────────────────────────
step "checking this Mac"
RAM_GB=$(( $(sysctl -n hw.memsize) / 1073741824 ))
MACOS=$(sw_vers -productVersion)
CHIP=$(sysctl -n machdep.cpu.brand_string)
METAL=$(system_profiler SPDisplaysDataType 2>/dev/null | sed -n 's/.*Metal Support: //p' | head -n 1)
FREE_GB=$(df -g "$HOME" | awk 'NR==2 {print $4}')
POWER=$(pmset -g batt | head -n 1 | sed "s/.*'\(.*\)'.*/\1/")

# Models by RAM, smallest first (a stopped run keeps the reports already written). Q4 only.
MODELS="unsloth/Qwen3-4B-GGUF:Q4_K_M unsloth/gemma-3-12b-it-GGUF:Q4_K_M"
NEED_GB=10
[ "$RAM_GB" -ge 24 ] && { MODELS="$MODELS unsloth/Mistral-Small-3.2-24B-Instruct-2506-GGUF:Q4_K_M"; NEED_GB=25; }
if [ "$RAM_GB" -ge 32 ] || [ "${BIG:-0}" = 1 ]; then MODELS="$MODELS unsloth/gemma-4-31B-it-GGUF:Q4_K_M"; NEED_GB=45; fi
NEED_GB=$(( NEED_GB + 5 )) # llama.cpp checkout and build, the engine, logs

problems=0
check() { # check <ok|warn|fail> <what> <detail>
  printf '   %-6s %-24s %s\n' "$1" "$2" "$3"
  [ "$1" = fail ] && problems=$(( problems + 1 ))
  return 0 # "cond && check ok || check fail" must not run both
}
[ "$(uname -m)" = arm64 ] && check ok "processor" "$CHIP (arm64)" || check fail "processor" "$(uname -m): this test is for Apple Silicon"
case "$MACOS" in
  1[4-9]*|2[0-9]*) check ok "macOS" "$MACOS" ;;
  *) check warn "macOS" "$MACOS: the release is built on macOS 14; older versions are not tested" ;;
esac
[ -n "$METAL" ] && check ok "GPU" "Metal: $METAL" || check warn "GPU" "no Metal support reported"
[ "$RAM_GB" -ge 8 ] && check ok "memory" "$RAM_GB GB" || check fail "memory" "$RAM_GB GB: at least 8 GB is needed"
[ "${FREE_GB:-0}" -ge "$NEED_GB" ] && check ok "disk" "$FREE_GB GB free, about $NEED_GB GB needed" \
  || check fail "disk" "$FREE_GB GB free, about $NEED_GB GB needed"
case "$POWER" in
  "AC Power") check ok "power" "charger connected" ;;
  *) check warn "power" "on battery: plug in the charger, numbers on battery are not comparable" ;;
esac
xcode-select -p >/dev/null 2>&1 && check ok "command line tools" "installed" \
  || check fail "command line tools" "missing: a window opens to install them; run this script again after"
for host in github.com huggingface.co cluaiz.com; do
  curl -fsS -m 10 -o /dev/null "https://$host" && check ok "network" "$host reachable" \
    || check fail "network" "$host not reachable"
done
note "models: $MODELS"
if [ "$problems" -gt 0 ]; then
  xcode-select -p >/dev/null 2>&1 || xcode-select --install
  echo "== $problems check(s) failed; fix them and run the script again"
  exit 1
fi
{
  sw_vers; uname -smr
  echo "$CHIP"
  sysctl hw.memsize hw.ncpu hw.perflevel0.physicalcpu hw.perflevel1.physicalcpu 2>/dev/null
  # Without the Mac's serial number and IDs: the results go into a public repository
  system_profiler SPHardwareDataType SPDisplaysDataType 2>/dev/null | grep -vE "Serial|UUID|UDID|Activation Lock"
  df -h "$HOME"; pmset -g batt; pmset -g therm 2>/dev/null
  echo "== SSD write, 2 GB"; dd if=/dev/zero of="$W/ddtest" bs=1m count=2048 2>&1 | tail -1; rm -f "$W/ddtest"
} > "$OUT/machine.txt" 2>&1

# ── 2. Install ──────────────────────────────────────────────────────────────────────────────
step "installing Cluaiz with the website's command (curl -fsSL https://cluaiz.com/install.sh | sh)"
curl -fsSL https://cluaiz.com/install.sh |
  CLUAIZ_INSTALL_DIR="$W/bin" CLUAIZ_VERSION="${CLUAIZ_VERSION:-}" sh > "$OUT/install.log" 2>&1
[ -x "$BIN" ] || { echo "install failed:"; tail -20 "$OUT/install.log"; exit 1; }
sed 's/\x1b\[[0-9;]*m//g' "$OUT/install.log" | grep -E "DONE|ERROR|error" | sed 's/^ */   /'
note "installed: $("$BIN" --version 2>/dev/null | tail -n 1)"
ls -la "$W/bin" > "$OUT/installed_files.txt"

# ── 3. llama.cpp, built in the background ───────────────────────────────────────────────────
step "building plain llama.cpp (llama-bench, Metal) in the background while the models download"
LB="$W/llama.cpp/build/bin/llama-bench"
(
  set -e
  if ! command -v cmake >/dev/null; then
    python3 -m venv "$W/venv" && "$W/venv/bin/pip" -q install cmake
    export PATH="$W/venv/bin:$PATH"
  fi
  [ -d "$W/llama.cpp" ] || git clone -q --filter=blob:none https://github.com/cluaiz/llama.cpp.git "$W/llama.cpp"
  cd "$W/llama.cpp" && git checkout -q "$LLAMA_COMMIT"
  cmake -B build -DCMAKE_BUILD_TYPE=Release -DLLAMA_BUILD_TESTS=OFF -DLLAMA_BUILD_SERVER=OFF -DLLAMA_OPENSSL=OFF
  cmake --build build -j "$(sysctl -n hw.ncpu)" --target llama-bench
) > "$OUT/llama_bench_build.log" 2>&1 &
BUILD=$!

# ── 4. Models ───────────────────────────────────────────────────────────────────────────────
export CLUAIZ_HOME="$W/home" CLUAIZ_PORT=8011 CLUAIZ_HOST=127.0.0.1 CLUAIZ_NONINTERACTIVE=1
mkdir -p "$CLUAIZ_HOME"
size_gb() { # expected download of a model, GB
  case "$1" in
    *Qwen3-4B*) echo 2.5 ;; *gemma-3-12b*) echo 7.3 ;; *Mistral-Small*) echo 14.3 ;; *gemma-4-31B*) echo 18.3 ;; *) echo 0 ;;
  esac
}
home_mb() { du -sm "$CLUAIZ_HOME" 2>/dev/null | awk '{print $1}'; }
TOTAL_GB=0; for id in $MODELS; do TOTAL_GB=$(echo "$TOTAL_GB + $(size_gb "$id")" | bc); done
step "downloading the models (about $TOTAL_GB GB)"
k=0; count=$(echo "$MODELS" | wc -w | tr -d ' ')
for id in $MODELS; do
  k=$(( k + 1 ))
  start=$(date +%s); base=$(home_mb); want=$(size_gb "$id")
  note "[$k/$count] $id (about $want GB)"
  "$BIN" pull "$id" </dev/null > "$OUT/pull_$(echo "$id" | tr '/:' '__').log" 2>&1 &
  PULL=$!
  while kill -0 "$PULL" 2>/dev/null; do
    sleep 20
    kill -0 "$PULL" 2>/dev/null || break
    got=$(( $(home_mb) - base )); secs=$(( $(date +%s) - start ))
    awk -v got="$got" -v want="$want" -v secs="$secs" 'BEGIN {
      gb = got / 1024; rate = got / (secs > 0 ? secs : 1); pct = (want > 0) ? 100 * gb / want : 0; if (pct > 99) pct = 99
      left = (rate > 0 && want > gb) ? (want - gb) * 1024 / rate / 60 : 0
      printf "      %.1f of %.1f GB (%d%%), %.1f MB/s, about %d min left\n", gb, want, pct, rate, left + 0.5 }'
  done
  wait "$PULL"; rc=$?
  echo "$id exit $rc $(( $(date +%s) - start ))s" >> "$OUT/pulls.log"
  [ "$rc" = 0 ] && note "      done in $(( ($(date +%s) - start) / 60 )) min" || note "      FAILED (exit $rc): see pull_*.log"
done
if kill -0 "$BUILD" 2>/dev/null; then note "waiting for the llama.cpp build to finish"; fi
wait "$BUILD"; BUILD=""
[ -x "$LB" ] || { echo "llama-bench did not build:"; tail -30 "$OUT/llama_bench_build.log"; exit 1; }
note "llama-bench built"

RUNPY="$HERE/run.py"
if [ ! -f "$RUNPY" ]; then
  RUNPY="$W/run.py"
  curl -fsSL https://raw.githubusercontent.com/cluaiz/cluaiz/main/test/engine_regression/run.py -o "$RUNPY"
fi

# ── 5. Engine ───────────────────────────────────────────────────────────────────────────────
step "starting the Cluaiz server (port 8011, its own home)"
"$BIN" serve > "$OUT/server.log" 2>&1 &
SERVER=$!
for i in $(seq 1 120); do curl -s -m 2 http://127.0.0.1:8011/health >/dev/null && break; sleep 1; done
curl -s -m 2 http://127.0.0.1:8011/health >/dev/null && note "server is up" || { echo "server did not start:"; tail -20 "$OUT/server.log"; exit 1; }
"$BIN" status > "$OUT/status.txt" 2>&1
grep -iE "gpu|metal|ram|memory" "$OUT/status.txt" | head -6 | sed 's/^ */   /'

# ── 6. Cluaiz vs llama.cpp ──────────────────────────────────────────────────────────────────
step "Cluaiz vs llama.cpp, every model and placement mode"
k=0
for id in $MODELS; do
  k=$(( k + 1 ))
  # run.py's --only matches the registered ID, e.g. gemma-3-12b-it-gguf-Q4_K_M
  key=$(echo "${id#*/}" | tr 'A-Z' 'a-z' | sed 's/:/-/')
  name=$(echo "$key" | cut -d- -f1-3)
  modes="auto,gpu,split,cpu"
  extra=()
  if [ "$id" = unsloth/gemma-4-31B-it-GGUF:Q4_K_M ] && [ "$RAM_GB" -lt 32 ]; then
    modes="auto"; extra=(--allow-big) # bigger than the RAM: the planner's own placement, read from the SSD
  fi
  note "[$k/$count] $id ($modes), $(elapsed) elapsed"
  python3 -u "$RUNPY" --api http://127.0.0.1:8011 --token-file "$CLUAIZ_HOME/session.token" \
    --models-dir "$CLUAIZ_HOME/models" --llama-bench "$LB" --only "$key" --modes "$modes" \
    --runs "${RUNS:-3}" ${extra[@]+"${extra[@]}"} --report "$OUT/RESULTS_$name.md" 2>&1 | tee "$OUT/run_$name.log"
done

# ── 7. Metal reference ──────────────────────────────────────────────────────────────────────
# What the Metal GPU does with every layer on it, whatever Cluaiz chose
step "plain llama.cpp with every layer on the Metal GPU (-ngl 99), for reference"
find "$CLUAIZ_HOME/models" -name '*.gguf' ! -name 'mmproj*' | while read -r f; do
  gb=$(( $(stat -f %z "$f") / 1073741824 ))
  if [ "$gb" -ge $(( RAM_GB * 6 / 10 )) ]; then echo "skip $(basename "$f"): $gb GB"; continue; fi
  "$LB" -m "$f" -ngl 99 -p 512 -n 64 -r 2
done 2>&1 | tee "$OUT/metal_reference.log"

# ── 8. Logs ─────────────────────────────────────────────────────────────────────────────────
step "collecting the logs"
lsof -p "$SERVER" 2>/dev/null | grep -E "\.dylib|\.metallib" > "$OUT/loaded_libs.txt"
kill "$SERVER" 2>/dev/null; SERVER=""
sed 's/\x1b\[[0-9;]*m//g' "$OUT/server.log" |
  grep -aE "^║|^╔|^╚|^╠|Loading model:|Measured on|over its estimate|VRAM planned|timings" > "$OUT/planner.log"
echo
echo "== summary"
grep -hE "^[a-z0-9.-]+-Q[0-9A-Z_]+ +(auto|gpu|split|cpu|-) " "$OUT"/run_*.log 2>/dev/null | sed 's/^/   /'
echo "== done in $(elapsed)"
echo "To remove everything afterwards: rm -rf ~/cluaiz-mactest, and the '# Cluaiz' PATH lines in ~/.zshrc"
