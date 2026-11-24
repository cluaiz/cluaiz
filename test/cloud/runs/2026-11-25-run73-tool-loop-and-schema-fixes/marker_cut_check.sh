#!/bin/sh
# Run 70's two half-call requests (marker case i = 3 and i = 9, Engine tools, max_tokens 48), before and after
# usage: sh marker_cut_check.sh <work dir with before/, t2after/, home/, loop_probe.py> <after dir name>
W="$1"; A="${2:-t2after}"; M=qwen3.5-0.8b-gguf-Q4_K_M
cd "$W" || exit 1
for v in before "$A"; do
  for i in 3 9; do
    python -B loop_probe.py --exe "$W/$v/cluaiz.exe" --home "$W/home" --port 8013 --out "$W/raw/${v}_marker_cut" --model $M --case marker --start $i --n 1 --label ${v}_marker_cut
  done
done
