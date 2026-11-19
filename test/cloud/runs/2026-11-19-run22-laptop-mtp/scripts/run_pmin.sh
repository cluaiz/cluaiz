S="${S:?set S to a work folder}"
export CLUAIZ_HOME=$S/home-mtp CLUAIZ_PORT=8011
cd <home>/my/Cluaiz-workspace/Cluaiz-Technologies/cluaiz
OUT=$S/matrix/pmin.txt; : > $OUT
for p in 0.5 0.75; do
  echo "== gpu draft3 p_min $p" >> $OUT
  CLUAIZ_MTP_PMIN=$p ./target/release/cluaiz.exe bench qwen3.5-2b-mtp-gguf-Q4_K_M --mtp -r 2 2>&1 | grep '^|\|^qwen' >> $OUT
done
for p in 0.5 0.75; do
  echo "== cpu draft3 p_min $p" >> $OUT
  CLUAIZ_MTP_PMIN=$p ./target/release/cluaiz.exe bench qwen3.5-2b-mtp-gguf-Q4_K_M --mtp -r 1 --n-gpu-layers 0 2>&1 | grep '^|\|^qwen' >> $OUT
done
echo done >> $OUT
