#!/usr/bin/env bash
# Run 63: the exact command on the VM. pp_gap.sh, balloon.py and analyze_pp_gap.py are the files in
# test/cloud/remote/ssd_prefetch/ at the Engine commit of this run (same md5 on the VM, checked).
cd ~/ssdpf
CLUAIZ=$HOME/cluaiz-pf/target/release/cluaiz HOMES=$HOME/ch-run63 PP=512,2048 TG=0 REPS=2 ROUNDS=5 CONFIGS="C LF" \
  bash pp_gap.sh $HOME/results-run63 > $HOME/run63.out 2>&1
python3 analyze_pp_gap.py $HOME/results-run63 > $HOME/results-run63/phases.md
