#!/usr/bin/env bash
# After linux_build.sh: unit tests, then an isolated server with the models of models.txt and
# checks.py. Everything goes to ~/results (pulled back to test/cloud/runs/).
# Usage: linux_tests.sh [mixed rounds, default 5]
set -uo pipefail
ROUNDS="${1:-5}"
OUT="$HOME/results"; mkdir -p "$OUT"
. "$HOME/.cargo/env"
cd "$HOME/cluaiz"
export CUDA_PATH=/usr/local/cuda PATH="/usr/local/cuda/bin:$PATH" LD_LIBRARY_PATH="/usr/local/cuda/lib64:${LD_LIBRARY_PATH:-}"

{
  echo "== hardware"; lscpu | grep -E "Model name|^CPU\(s\)"; free -g; nvidia-smi || echo "no NVIDIA GPU"; df -h /
} > "$OUT/machine.txt" 2>&1

echo "== unit tests" | tee "$OUT/unit_tests.log"
for p in cluaiz-core cluaiz-plan cluaiz-models cluaiz-media cluaiz-context cluaiz-agents cluaiz-cep cluaiz-runtime onnx-engine cluaiz-api; do
  r=$(cargo test --release -j 4 -p "$p" 2>&1 | tee -a "$OUT/unit_tests_full.log" | grep -E "test result" | awk '{p+=$4; f+=$6} END {print "passed",p+0,"failed",f+0}')
  echo "$p: $r" | tee -a "$OUT/unit_tests.log"
done
grep -E "^test .* FAILED|panicked at" "$OUT/unit_tests_full.log" | head -40 >> "$OUT/unit_tests.log"

export CLUAIZ_HOME="$HOME/ch" CLUAIZ_PORT=8011 CLUAIZ_HOST=127.0.0.1 CLUAIZ_NONINTERACTIVE=1
mkdir -p "$CLUAIZ_HOME"
BIN="$HOME/cluaiz/target/release/cluaiz"
echo "== pulls" | tee "$OUT/pulls.log"
grep -vE "^#|^\s*$" "$HOME/cluaiz/test/cloud/remote/models.txt" | while read -r kind id; do
  start=$(date +%s)
  "$BIN" pull "$id" </dev/null > "$OUT/pull_$(echo "$id" | tr '/:' '__').log" 2>&1
  echo "$kind $id exit $? $(( $(date +%s) - start ))s" | tee -a "$OUT/pulls.log"
done

"$BIN" serve > "$OUT/server.log" 2>&1 &
for i in $(seq 1 120); do curl -s -m 2 http://127.0.0.1:8011/health >/dev/null && break; sleep 1; done
python3 "$HOME/cluaiz/test/cloud/remote/checks.py" http://127.0.0.1:8011 "$CLUAIZ_HOME" "$ROUNDS" "$OUT" 2>&1 | tee "$OUT/checks.log"
kill %1 2>/dev/null
cp "$CLUAIZ_HOME/engine/vram_margins.json" "$OUT/" 2>/dev/null
grep -aE "PLAN|CANDIDATES|Full GPU|Freed idle|ONNX on|📏|fell back|could not load|out of memory|ERROR|panicked" "$OUT/server.log" | cut -c1-300 > "$OUT/server_highlights.log"
echo "== done"
