#!/usr/bin/env bash
# Cluaiz on a Mac with Apple Silicon, in about an hour: the release installed by its one command, then
# test/engine_regression/run.py against plain llama.cpp (llama-bench built with Metal from the same
# llama.cpp commit as the release), every model and placement mode, and every log packed into one archive
# on the Desktop. Everything else stays in ~/cluaiz-mactest (delete it afterwards).
#
# Usage, in Terminal with the charger plugged in (not "curl | bash": the commands inside would read the
# rest of the script from that pipe):
#   bash -c "$(curl -fsSL https://raw.githubusercontent.com/cluaiz/cluaiz/main/test/mac/mac_test.sh)"
#   bash mac_test.sh        (a copy; run.py is taken from beside it, else downloaded)
# Environment (optional):
#   BIG=1   also Gemma 4 31B Q4 (17 GB) when it is bigger than the RAM: read from the SSD while it runs
#   RUNS=2  requests per model and mode (default 3; the first is the cold one)
set -uo pipefail
# "$0" is "bash" when the script comes from curl: then nothing is beside it
HERE="$(cd "$(dirname "$0")" 2>/dev/null && pwd)"
W="$HOME/cluaiz-mactest"
OUT="$W/results"
BIN="$W/bin/cluaiz"
LLAMA_COMMIT=8f9842cd1faebd06413805eb495bdf1c7e80660f # engine/llama/llama.cpp in release v0.1.0
mkdir -p "$OUT"
exec > >(tee -a "$OUT/mac_test.log") 2>&1
T0=$(date +%s)
step() { echo; echo "== [$(( ($(date +%s) - T0) / 60 )) min] $*"; }

# The archive is written however the run ends (also after Ctrl+C)
pack() {
  [ -n "${SERVER:-}" ] && kill "$SERVER" 2>/dev/null
  local archive="$HOME/Desktop/cluaiz-mac-results-$(date +%Y%m%d-%H%M).tar.gz"
  tar -czf "$archive" -C "$W" results 2>/dev/null && echo "== results: $archive (send this file)"
}
trap pack EXIT

step "tools"
# clang, git and python3 come with the Xcode Command Line Tools
if ! xcode-select -p >/dev/null 2>&1; then
  echo "The Xcode Command Line Tools are needed: a window opens to install them; run this script again after"
  xcode-select --install
  exit 1
fi
if [ "$(uname -m)" != arm64 ]; then echo "This test is for Apple Silicon (arm64), found $(uname -m)"; exit 1; fi

step "machine"
{
  sw_vers; uname -a
  sysctl -n machdep.cpu.brand_string
  sysctl hw.memsize hw.ncpu hw.perflevel0.physicalcpu hw.perflevel1.physicalcpu 2>/dev/null
  system_profiler SPHardwareDataType SPDisplaysDataType 2>/dev/null
  df -h "$HOME"; pmset -g batt; pmset -g therm 2>/dev/null
  echo "== SSD write, 2 GB"; dd if=/dev/zero of="$W/ddtest" bs=1m count=2048 2>&1 | tail -1; rm -f "$W/ddtest"
} > "$OUT/machine.txt" 2>&1
RAM_GB=$(( $(sysctl -n hw.memsize) / 1073741824 ))
echo "$(sysctl -n machdep.cpu.brand_string), ${RAM_GB} GB RAM"

# Models by RAM, smallest first (a stopped run keeps the reports already written). Q4 only.
MODELS="unsloth/Qwen3-4B-GGUF:Q4_K_M unsloth/gemma-3-12b-it-GGUF:Q4_K_M"
[ "$RAM_GB" -ge 24 ] && MODELS="$MODELS unsloth/Mistral-Small-3.2-24B-Instruct-2506-GGUF:Q4_K_M"
if [ "$RAM_GB" -ge 32 ] || [ "${BIG:-0}" = 1 ]; then MODELS="$MODELS unsloth/gemma-4-31B-it-GGUF:Q4_K_M"; fi
echo "models: $MODELS"

step "install (the release's one command)"
curl -fsSL https://raw.githubusercontent.com/cluaiz/cluaiz/main/install.sh |
  CLUAIZ_INSTALL_DIR="$W/bin" sh > "$OUT/install.log" 2>&1
[ -x "$BIN" ] || { echo "install failed:"; tail -20 "$OUT/install.log"; exit 1; }
"$BIN" --version
ls -la "$W/bin" > "$OUT/installed_files.txt"

step "llama-bench (plain llama.cpp, Metal), built while the models download"
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

export CLUAIZ_HOME="$W/home" CLUAIZ_PORT=8011 CLUAIZ_HOST=127.0.0.1 CLUAIZ_NONINTERACTIVE=1
mkdir -p "$CLUAIZ_HOME"
step "downloads"
for id in $MODELS; do
  start=$(date +%s)
  "$BIN" pull "$id" </dev/null > "$OUT/pull_$(echo "$id" | tr '/:' '__').log" 2>&1
  echo "$id exit $? $(( $(date +%s) - start ))s" | tee -a "$OUT/pulls.log"
done
wait "$BUILD"
[ -x "$LB" ] || { echo "llama-bench did not build:"; tail -30 "$OUT/llama_bench_build.log"; exit 1; }

RUNPY="$HERE/run.py"
if [ ! -f "$RUNPY" ]; then
  RUNPY="$W/run.py"
  curl -fsSL https://raw.githubusercontent.com/cluaiz/cluaiz/main/test/engine_regression/run.py -o "$RUNPY"
fi

step "server"
"$BIN" serve > "$OUT/server.log" 2>&1 &
SERVER=$!
for i in $(seq 1 120); do curl -s -m 2 http://127.0.0.1:8011/health >/dev/null && break; sleep 1; done

for id in $MODELS; do
  # run.py's --only matches the registered ID, e.g. gemma-3-12b-it-gguf-Q4_K_M
  key=$(echo "${id#*/}" | tr 'A-Z' 'a-z' | sed 's/:/-/')
  name=$(echo "$key" | cut -d- -f1-3)
  modes="auto,gpu,split,cpu"
  extra=()
  if [ "$id" = unsloth/gemma-4-31B-it-GGUF:Q4_K_M ] && [ "$RAM_GB" -lt 32 ]; then
    modes="auto"; extra=(--allow-big) # bigger than the RAM: the planner's own placement, read from the SSD
  fi
  step "test $id ($modes)"
  python3 -u "$RUNPY" --api http://127.0.0.1:8011 --token-file "$CLUAIZ_HOME/session.token" \
    --models-dir "$CLUAIZ_HOME/models" --llama-bench "$LB" --only "$key" --modes "$modes" \
    --runs "${RUNS:-3}" ${extra[@]+"${extra[@]}"} --report "$OUT/RESULTS_$name.md" 2>&1 | tee "$OUT/run_$name.log"
done

step "logs"
lsof -p "$SERVER" 2>/dev/null | grep -E "\.dylib|\.metallib" > "$OUT/loaded_libs.txt"
kill "$SERVER" 2>/dev/null; SERVER=""
sed 's/\x1b\[[0-9;]*m//g' "$OUT/server.log" |
  grep -aE "^║|^╔|^╚|^╠|Loading model:|Measured on|over its estimate|VRAM planned|timings" > "$OUT/planner.log"
step "done in $(( ($(date +%s) - T0) / 60 )) min"
echo "To remove everything afterwards: rm -rf ~/cluaiz-mactest, and the '# Cluaiz' PATH lines in ~/.zshrc"
