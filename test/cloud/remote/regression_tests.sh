#!/usr/bin/env bash
# test/engine_regression/run.py on a cloud GPU VM, the same check as on the owner's PC: the installed
# release (install.sh) against plain llama.cpp (llama-bench built from the same fork commit, CUDA) for
# every chat model of a models file and every placement mode. Everything goes to ~/results-regression;
# a second call (other models or run.py arguments) adds its own report and logs beside the first.
# Usage: regression_tests.sh <cluaiz commit of the release> <CUDA arch, e.g. 75> <models file> <name> [run.py args]
set -uo pipefail
COMMIT="$1"; ARCH="$2"; MODELS="$3"; NAME="$4"; shift 4
OUT="$HOME/results-regression"; mkdir -p "$OUT"
# CLUAIZ_BIN: an engine built from source instead of the installed release
BIN="${CLUAIZ_BIN:-$HOME/.cluaiz/bin/cluaiz}"
if [ ! -x "$BIN" ]; then
  curl -fsSL https://raw.githubusercontent.com/cluaiz/cluaiz/main/install.sh | sh > "$OUT/install.log" 2>&1
fi
[ -x "$BIN" ] || { echo "install failed"; exit 1; }
{ command -v cmake && command -v g++; } >/dev/null || { sudo apt-get -qq update && sudo DEBIAN_FRONTEND=noninteractive apt-get -qq install -y cmake g++; } >/dev/null 2>&1
unset LD_LIBRARY_PATH

{
  echo "== hardware"; lscpu | grep -E "Model name|^CPU\(s\)"; free -g; nvidia-smi; df -h /
  echo "== disk read speed"; sudo hdparm -t "$(findmnt -no SOURCE /)" 2>&1 | tail -1
} > "$OUT/machine_$NAME.txt" 2>&1

# Source at the release's commit: run.py, and llama.cpp for llama-bench
if [ ! -d "$HOME/cluaiz" ]; then
  git clone -q https://github.com/cluaiz/cluaiz.git "$HOME/cluaiz"
fi
git -C "$HOME/cluaiz" fetch -q origin && git -C "$HOME/cluaiz" checkout -q "$COMMIT"
git -C "$HOME/cluaiz" submodule update -q --init engine/llama/llama.cpp
LB="$HOME/cluaiz/engine/llama/llama.cpp/build-bench/bin/llama-bench"
# A build left running by an earlier call of this script finishes first
while pgrep -f "[c]make --build build-bench" >/dev/null; do sleep 10; done
if [ ! -x "$LB" ]; then
  echo "== llama-bench build" | tee -a "$OUT/llama_bench_build.log"
  (
    cd "$HOME/cluaiz/engine/llama/llama.cpp"
    export PATH="/usr/local/cuda/bin:$PATH"
    cmake -B build-bench -DCMAKE_BUILD_TYPE=Release -DGGML_CUDA=ON -DCMAKE_CUDA_ARCHITECTURES="$ARCH" \
      -DGGML_CUDA_CCCL_VERSION=v3.4.3 -DLLAMA_BUILD_TESTS=OFF -DLLAMA_BUILD_SERVER=OFF -DLLAMA_OPENSSL=OFF &&
    cmake --build build-bench -j "$(nproc)" --target llama-bench
  ) >> "$OUT/llama_bench_build.log" 2>&1 &
  BUILD=$!
fi

export CLUAIZ_HOME="$HOME/ch-reg" CLUAIZ_PORT=8011 CLUAIZ_HOST=127.0.0.1 CLUAIZ_NONINTERACTIVE=1
mkdir -p "$CLUAIZ_HOME"
echo "== pulls ($NAME)" | tee -a "$OUT/pulls.log"
grep -vE "^#|^\s*$" "$MODELS" | while read -r id; do
  start=$(date +%s)
  "$BIN" pull "$id" </dev/null > "$OUT/pull_$(echo "$id" | tr '/:' '__').log" 2>&1
  echo "$id exit $? $(( $(date +%s) - start ))s" | tee -a "$OUT/pulls.log"
done
[ -n "${BUILD:-}" ] && wait "$BUILD"
[ -x "$LB" ] || { echo "llama-bench did not build"; tail -30 "$OUT/llama_bench_build.log"; exit 1; }

"$BIN" serve > "$OUT/server_$NAME.log" 2>&1 &
SERVER=$!
for i in $(seq 1 120); do curl -s -m 2 http://127.0.0.1:8011/health >/dev/null && break; sleep 1; done

python3 -u "$HOME/cluaiz/test/engine_regression/run.py" --api http://127.0.0.1:8011 \
  --token-file "$CLUAIZ_HOME/session.token" --models-dir "$CLUAIZ_HOME/models" --llama-bench "$LB" \
  --report "$OUT/RESULTS_$NAME.md" "$@" 2>&1 | tee "$OUT/run_$NAME.log"

kill "$SERVER" 2>/dev/null
sed 's/\x1b\[[0-9;]*m//g' "$OUT/server_$NAME.log" | grep -aE "^║|^╔|^╚|^╠|Loading model:|Measured on this GPU|over its estimate|VRAM planned" \
  > "$OUT/planner_$NAME.log"
echo "== done $NAME"
