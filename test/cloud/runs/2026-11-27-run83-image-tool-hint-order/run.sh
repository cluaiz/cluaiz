#!/usr/bin/env bash
# Run 83 driver on the VM: bash run.sh  (isolated home ~/run83/home, port 8014)
set -uo pipefail
cd ~/run83; EXE=~/cluaiz/target/debug/cluaiz; export CLUAIZ_NONINTERACTIVE=1
H=~/run83/home; mkdir -p $H raw
for spec in "q08 unsloth/Qwen3.5-0.8B-GGUF:Q4_K_M" "q4 unsloth/Qwen3.5-4B-GGUF:Q4_K_M"; do
  set -- $spec
  CLUAIZ_HOME=$H $EXE pull $2 2>&1 | tr '\r' '\n' | grep -a "Registered ID" > raw/$1-pull.txt
  ID=$(awk '{print $NF}' raw/$1-pull.txt)
  ls -la $H/models/chat/$ID > raw/$1-files.txt
  python3 hint_order.py --exe $EXE --home $H --model "$ID" --runs 5 --out raw --tag $1 > raw/$1.console.txt 2>&1
  echo "$1 rc=$?" >> raw/steps.txt
done
echo done >> raw/steps.txt
