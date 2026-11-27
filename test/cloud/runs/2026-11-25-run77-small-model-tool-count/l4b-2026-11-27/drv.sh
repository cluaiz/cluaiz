#!/usr/bin/env bash
# Run 77 completion on cluaiz-l4b: qwen3508 (missing cells) and qwen352 (all cells).
set -uo pipefail
cd ~/run77
EXE=~/cluaiz/target/debug/cluaiz
export CLUAIZ_NONINTERACTIVE=1
mkdir -p homeA home raw
CLUAIZ_HOME=~/run77/home $EXE plugin install math --yes > raw/setupB_plugin.txt 2>&1; echo "exit=$?" >> raw/setupB_plugin.txt
CLUAIZ_HOME=~/run77/home $EXE mcp install filesystem --yes > raw/setupB_mcp.txt 2>&1; echo "exit=$?" >> raw/setupB_mcp.txt
for spec in "qwen3508 unsloth/Qwen3.5-0.8B-GGUF:Q4_K_M" "qwen352 unsloth/Qwen3.5-2B-GGUF:Q4_K_M"; do
  set -- $spec; L=$1; R=$2
  for h in homeA home; do CLUAIZ_HOME=~/run77/$h $EXE pull $R 2>&1 | tr '\r' '\n' | grep -a "Registered ID" > raw/$L-pull-$h.txt; done
  ID=$(grep -ao "Registered ID:  *[^ ]*" raw/$L-pull-homeA.txt | awk '{print $NF}')
  echo "$L id=$ID" >> raw/progress.txt
  ls -la homeA/models/chat/$ID >> raw/progress.txt
  for mode in off on; do for s in A B C; do
    [ "$L" = qwen3508 ] && [ "$mode$s" = offA ] && continue
    H=homeA; [ $s = B ] && H=home
    d=raw/$L-$mode-$s; rm -rf $d
    python3 matrix_probe.py --exe $EXE --home ~/run77/$H --out $d --model "$ID" --setup $s --think $mode > $d.console.txt 2>&1
    echo "$L $mode $s rc=$? rows=$(wc -l < $d/raw.jsonl 2>/dev/null)" >> raw/progress.txt
  done; done
done
echo done >> raw/progress.txt
