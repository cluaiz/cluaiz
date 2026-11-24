#!/usr/bin/env bash
# pp2048 cause run: the exact command on the VM (pp_gap.sh, balloon.py, vram_hog.py, analyze_pp_gap.py copied
# from test/cloud/remote/ssd_prefetch/ of this commit into ~/ppcause).
cd ~/ppcause
CLUAIZ=$HOME/ppcause/bin/cluaiz HOMES=$HOME/ch-cause PP=512,2048 TG=0 REPS=2 ROUNDS=${ROUNDS:-3} \
  CONFIGS="${CONFIGS:-C LF C-b2048 LFB512 C-c2048 C-nw}" bash pp_gap.sh "${OUTDIR:-$HOME/results-cause}" > "${OUTDIR:-$HOME/results-cause}.out" 2>&1
python3 analyze_pp_gap.py "${OUTDIR:-$HOME/results-cause}" > "${OUTDIR:-$HOME/results-cause}/phases.md"
