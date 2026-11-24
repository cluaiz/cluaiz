#!/bin/sh
# Terminal agent at the step limit: a task that needs 6 file reads, --max-steps 2, headless JSON result.
# usage: sh cli_limit_check.sh <work dir with <bin>/cluaiz.exe and home/> <bin dir name> <out dir> <runs>
W="$1"; B="$2"; OUT="$3"; N="${4:-3}"; M=qwen3.5-0.8b-gguf-Q4_K_M
mkdir -p "$OUT"
for r in $(seq 1 "$N"); do
  D="$OUT/ws_$r"; rm -rf "$D"; mkdir -p "$D"
  i=0; for f in a b c d e f; do i=$((i+1)); echo "$((i*7+r))" > "$D/$f.txt"; done
  CLUAIZ_HOME="$W/home" "$W/$B/cluaiz.exe" agent --local --ui plain -y --output-format json --max-steps 2 --think off \
    -m $M -C "$D" -p "Read the files a.txt, b.txt, c.txt, d.txt, e.txt and f.txt one at a time with read_file, then tell me the sum of the numbers in them." \
    > "$OUT/run_$r.json" 2> "$OUT/run_$r.err"
  echo "run $r exit=$? result=$(python -c 'import json,sys;d=json.loads(open(sys.argv[1]).read().strip().splitlines()[-1]);print(json.dumps({k:d.get(k) for k in ("result","exit_code","error")}))' "$(cygpath -w "$OUT/run_$r.json")" 2>&1 | cut -c1-300)"
done
