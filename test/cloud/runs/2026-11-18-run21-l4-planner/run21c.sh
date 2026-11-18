#!/usr/bin/env bash
# Run 21c: the margin key fix in a fresh home (Gemma 3 12B, gpt-oss), then step-down during a load
set -uo pipefail
BIN=~/cluaiz/target/release/cluaiz
OUT=~/results-run21c; mkdir -p "$OUT"
export CLUAIZ_HOME=~/ch-run21c CLUAIZ_PORT=8011 CLUAIZ_HOST=127.0.0.1 CLUAIZ_NONINTERACTIVE=1
mkdir -p "$CLUAIZ_HOME/models/chat"
ln -sfn ~/ch/models/chat/qwen3-30b-a3b-instruct-2507-gguf-Q4_K_M "$CLUAIZ_HOME/models/chat/"
ln -sfn ~/ch/models/chat/gpt-oss-20b-gguf-FP4 "$CLUAIZ_HOME/models/chat/"
gguf() { find -L "$CLUAIZ_HOME/models" -iname "*$1*.gguf" ! -iname '*mmproj*' ! -iname 'mtp-*' | head -1; }
"$BIN" pull unsloth/gemma-3-12b-it-GGUF:Q4_K_M </dev/null > "$OUT/pull_gemma3.log" 2>&1
for p in gemma-3-12b gpt-oss-20b; do
  f="$(gguf "$p")"
  "$BIN" bench "$f" --placements -p 512 -n 128 -r 1 -v > "$OUT/bench_$p.md" 2> "$OUT/bench_$p.err"
  echo "bench $p exit $?" | tee -a "$OUT/steps.log"
  grep -h "📐\|VRAM planned\|buffer kept free" "$OUT/bench_$p.err" | cut -c1-300 | tee -a "$OUT/steps.log"
done
f="$(gguf Qwen3-30B-A3B)"
python3 ~/run21/vram_hog.py load 1.5 180 > "$OUT/stepdown_hog.log" 2>&1 & HOG=$!
"$BIN" bench "$f" -p 64 -n 16 -r 1 -v > "$OUT/stepdown_bench.md" 2> "$OUT/stepdown.log"
echo "step-down bench exit $?" | tee -a "$OUT/steps.log"
wait $HOG
cat "$OUT/stepdown_hog.log" | tee -a "$OUT/steps.log"
grep -E "VRAM planned|left after|failed to load|retry|smaller|step|GPU is full|kept as|free after" "$OUT/stepdown.log" | cut -c1-300 | tee -a "$OUT/steps.log"
tail -3 "$OUT/stepdown_bench.md" | tee -a "$OUT/steps.log"
echo "== all done" | tee -a "$OUT/steps.log"
