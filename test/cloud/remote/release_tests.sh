#!/usr/bin/env bash
# A GitHub release on a clean VM, the way a user gets it: the one-command install (install.sh), then
# the same isolated-server checks as linux_tests.sh with the installed binary, and which GPU
# libraries the server really loaded. Everything goes to ~/results (pulled back to test/cloud/runs/).
# Needs checks.py and models.txt beside it.
# Usage: release_tests.sh [version, default the newest release] [mixed rounds, default 5]
set -uo pipefail
VERSION="${1:-}"
ROUNDS="${2:-5}"
HERE="$(cd "$(dirname "$0")" && pwd)"
OUT="$HOME/results"; mkdir -p "$OUT"

{
  echo "== hardware"; lscpu | grep -E "Model name|^CPU\(s\)"; free -g; nvidia-smi || echo "no NVIDIA GPU"; df -h /
  echo "== os"; (. /etc/os-release; echo "$PRETTY_NAME"); ldd --version | head -1
  # The image ships a CUDA toolkit; the release must not need it, so nothing points at it
  echo "== LD_LIBRARY_PATH=${LD_LIBRARY_PATH:-<unset>}"
} > "$OUT/machine.txt" 2>&1
unset LD_LIBRARY_PATH

echo "== install" | tee "$OUT/install.log"
start=$(date +%s)
# An install.sh beside this script (an installer fix not pushed yet) replaces the published one
if [ -f "$HERE/install.sh" ]; then
  CLUAIZ_VERSION="$VERSION" sh "$HERE/install.sh" 2>&1 | tee -a "$OUT/install.log"
else
  curl -fsSL https://raw.githubusercontent.com/cluaiz/cluaiz/main/install.sh | CLUAIZ_VERSION="$VERSION" sh 2>&1 | tee -a "$OUT/install.log"
fi
echo "install took $(( $(date +%s) - start ))s" | tee -a "$OUT/install.log"
BIN="$HOME/.cluaiz/bin/cluaiz"
[ -x "$BIN" ] || { echo "install failed"; exit 1; }
ls -la "$HOME/.cluaiz/bin" > "$OUT/installed_files.txt"
du -sh "$HOME/.cluaiz/bin" >> "$OUT/installed_files.txt"

export CLUAIZ_HOME="$HOME/ch" CLUAIZ_PORT=8011 CLUAIZ_HOST=127.0.0.1 CLUAIZ_NONINTERACTIVE=1
mkdir -p "$CLUAIZ_HOME"
"$BIN" status > "$OUT/status.txt" 2>&1

echo "== pulls" | tee "$OUT/pulls.log"
grep -vE "^#|^\s*$" "$HERE/models.txt" | while read -r kind id; do
  start=$(date +%s)
  "$BIN" pull "$id" </dev/null > "$OUT/pull_$(echo "$id" | tr '/:' '__').log" 2>&1
  echo "$kind $id exit $? $(( $(date +%s) - start ))s" | tee -a "$OUT/pulls.log"
done

"$BIN" serve > "$OUT/server.log" 2>&1 &
SERVER=$!
for i in $(seq 1 120); do curl -s -m 2 http://127.0.0.1:8011/health >/dev/null && break; sleep 1; done
python3 "$HERE/checks.py" http://127.0.0.1:8011 "$CLUAIZ_HOME" "$ROUNDS" "$OUT" 2>&1 | tee "$OUT/checks.log"

# Which GPU libraries the server has open, and from where (must be ~/.cluaiz/bin, not the toolkit)
grep -oE "/[^ ]*(ggml|llama|cudart|cublas|cudnn|onnxruntime)[^ ]*\.so[^ ]*" "/proc/$SERVER/maps" | sort -u > "$OUT/loaded_libs.txt"
kill "$SERVER" 2>/dev/null
cp "$CLUAIZ_HOME/engine/vram_margins.json" "$OUT/" 2>/dev/null
grep -aE "Backends loaded|PLAN|CANDIDATES|Full GPU|Freed idle|ONNX on|CUDAExecutionProvider|cuDNN|📏|fell back|could not load|out of memory|ERROR|panicked" "$OUT/server.log" | cut -c1-300 > "$OUT/server_highlights.log"
echo "== done"
