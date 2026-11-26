#!/usr/bin/env bash
# Run 75: run 71's pending pp2048 follow-ups, with run 71's own harness and binaries (unchanged):
# test/cloud/remote/ssd_prefetch/pp_gap.sh (copy in ~/ppcause), Cluaiz = ~/ppcause/bin/cluaiz (run 71's build),
# llama-bench = the fork build of runs 48/59/63/71, VRAM hog 14,424 MiB, 12 GB RAM cgroup, page cache dropped
# before each run, 3 rounds with the order rotated.
#   set p2048:     -p 2048 alone on both tools (no pp512 phase before it), C and LF
#   set p512_2048: -p 512,2048 as in run 71, C and LF
# mincore_watch.py runs beside each set and writes <tag>.mincore.json at the first pp2048 phase mark of each run.
# Usage on the VM: bash run.sh <out root>
set -uo pipefail
ROOT="${1:-$HOME/run75}"; HERE="$(cd "$(dirname "$0")" && pwd)"
GGUF=$(ls "$HOME/ch-run23/models/chat/qwen3.6-35b-a3b-mtp-gguf-UD-IQ4_XS"/*.gguf | grep -vi mmproj | head -1)
mkdir -p "$ROOT"
md5sum "$HOME/ppcause/pp_gap.sh" "$HOME/ppcause/bin/cluaiz" "$HOME/fork-bench-build/bin/llama-bench" > "$ROOT/inputs.md5"
for set in p2048 p512_2048; do
  OUT="$ROOT/$set"; mkdir -p "$OUT"
  [ "$set" = p2048 ] && PPV=2048 || PPV=512,2048
  python3 "$HERE/mincore_watch.py" "$OUT" "$GGUF" > "$OUT/mincore_watch.log" 2>&1 & W=$!
  (cd "$HOME/ppcause" && CLUAIZ="$HOME/ppcause/bin/cluaiz" HOMES="$HOME/ch-run75" PP="$PPV" TG=0 REPS=2 ROUNDS=3 \
    CONFIGS="C LF" bash pp_gap.sh "$OUT" > "$OUT.out" 2>&1)
  touch "$OUT/mincore.stop"; sleep 1; kill "$W" 2>/dev/null
  python3 "$HOME/ppcause/analyze_pp_gap.py" "$OUT" > "$OUT/phases.md"
done
# pp_gap.sh starts the VRAM hog and leaves it running; stop only that hog
pkill -f "vram_hog.py 0 8" && echo "hog stopped" >> "$ROOT/steps.txt"
echo "== run75 done" >> "$ROOT/steps.txt"
