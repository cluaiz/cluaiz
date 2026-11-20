#!/usr/bin/env bash
# Run 23: MTP off and on in every placement the planner can choose, on a Linux L4 VM (Google Cloud
# g2-standard-8: L4 23 GB, 8 vCPU, 31 GB RAM). Every case is `cluaiz bench --mtp-compare`, which
# alternates MTP off and on per round on the same pinned placement.
#
# Cases (the laptop run 22 measured dense full GPU, split and CPU on an RTX 3050 4 GB):
#   A  dense 2B, every placement, draft 3 and draft 1
#   B  dense 2B, quant types Q4_0 / Q8_0 / IQ4_XS (other GPU kernels for 2-4 rows), full GPU
#   C  dense 2B, temperature 0.7, full GPU
#   D  dense 2B, a 3000-token prompt, full GPU and CPU only
#   E  MoE 35B-A3B IQ4_XS (18.2 GB), every placement (full GPU, MoE split, CPU only)
#   F  same MoE with 8 GB of VRAM left (another process holds the rest): the planner's choice
#   G  same MoE, 8 GB VRAM and RAM limited to 12 GB (cgroup): experts read from the SSD
#   H  dense 2B CPU only with RAM limited below the file size: weights read from the SSD
#
# Usage on the VM: run23.sh [case letters, default all], e.g. run23.sh AB
set -uo pipefail
HERE="$(cd "$(dirname "$0")" && pwd)"
BIN=~/cluaiz/target/release/cluaiz
OUT=~/results-run23; mkdir -p "$OUT"
export CLUAIZ_HOME=~/ch-run23 CLUAIZ_PORT=8011 CLUAIZ_HOST=127.0.0.1 CLUAIZ_NONINTERACTIVE=1
CASES="${1:-ABCDEFGH}"
DENSE=qwen3.5-2b-mtp-gguf-Q4_K_M
PROMPTS3K="$HERE/prompts_long3k.json"
log() { echo "[$(date +%H:%M:%S)] $*" | tee -a "$OUT/steps.log"; }

# bench <name> <args...>: Markdown and JSON of one case, stderr kept, GPU state before and after
bench() {
  local name="$1"; shift
  log "start $name: $*"
  nvidia-smi --query-gpu=clocks.sm,power.draw,temperature.gpu,memory.used --format=csv,noheader >> "$OUT/$name.gpu" 2>&1
  free -m | sed -n 2p >> "$OUT/$name.gpu"
  # disk reads during the case: major page faults and sectors read (all disks)
  local faults0 sectors0
  faults0=$(awk '/^pgmajfault /{print $2}' /proc/vmstat)
  sectors0=$(awk '$3 ~ /^(nvme[0-9]+n[0-9]+|sd[a-z])$/ {s+=$6} END {print s}' /proc/diskstats)
  echo "cgroup $(cat /proc/self/cgroup) memory.max $(cat /sys/fs/cgroup$(cut -d: -f3 /proc/self/cgroup)/memory.max 2>/dev/null)" >> "$OUT/$name.gpu"
  "$BIN" bench "$@" --mtp-compare -o json > "$OUT/$name.json" 2> "$OUT/$name.err"
  local rc=$?
  echo "major faults $(( $(awk '/^pgmajfault /{print $2}' /proc/vmstat) - faults0 )), read $(( ($(awk '$3 ~ /^(nvme[0-9]+n[0-9]+|sd[a-z])$/ {s+=$6} END {print s}' /proc/diskstats) - sectors0) / 2048 )) MiB from disk" >> "$OUT/$name.gpu"
  nvidia-smi --query-gpu=clocks.sm,power.draw,temperature.gpu,memory.used --format=csv,noheader >> "$OUT/$name.gpu" 2>&1
  python3 "$HERE/to_md.py" "$OUT/$name.json" > "$OUT/$name.md" 2>/dev/null
  log "end $name exit $rc"
}

# limited <MiB> <case> <args...>: one bench case with RAM (page cache included) capped by a cgroup,
# so weights that do not fit are read from the SSD again when used
limited() {
  local mb="$1"; shift
  # Page cache is charged to the cgroup that first read a page: a model cached by an earlier case
  # would stay in RAM outside the limit, so the cache is dropped first
  sync; echo 3 | sudo tee /proc/sys/vm/drop_caches > /dev/null
  sudo systemd-run --scope --quiet -p MemoryMax="${mb}M" -p MemorySwapMax=0 sudo -u "$USER" env HOME="$HOME" PATH="$PATH" CLUAIZ_HOME="$CLUAIZ_HOME" CLUAIZ_PORT="$CLUAIZ_PORT" CLUAIZ_HOST="$CLUAIZ_HOST"     CLUAIZ_NONINTERACTIVE=1 BIN="$BIN" OUT="$OUT" HERE="$HERE"     bash -c "$(declare -f bench log); bench \"\$@\"" bench "$@"
}

pull() { "$BIN" pull "$1" </dev/null >> "$OUT/pulls.log" 2>&1; log "pull $1 exit $?"; }

case "$CASES" in *[ABCDH]*)
  pull unsloth/Qwen3.5-2B-MTP-GGUF:Q4_K_M ;;
esac
case "$CASES" in *[EFG]*)
  pull unsloth/Qwen3.6-35B-A3B-MTP-GGUF:UD-IQ4_XS ;;
esac
"$BIN" list > "$OUT/list.txt" 2>&1
MOE="$(awk '/35b-a3b/ && /IQ4_XS/ {print $1; exit}' IGNORECASE=1 "$OUT/list.txt")"
log "MoE id: ${MOE:-not installed}"

[[ "$CASES" == *A* ]] && {
  bench A_dense_placements_d3 "$DENSE" --placements -r 2
  CLUAIZ_MTP_DRAFT=1 bench A_dense_placements_d1 "$DENSE" --placements -r 2
}
[[ "$CASES" == *B* ]] && for q in Q4_0 Q8_0 IQ4_XS; do
  pull "unsloth/Qwen3.5-2B-MTP-GGUF:$q"
  bench "B_dense_${q}" "qwen3.5-2b-mtp-gguf-$q" -r 2
done
[[ "$CASES" == *C* ]] && bench C_dense_temp07 "$DENSE" --temperature 0.7 -r 3
[[ "$CASES" == *D* ]] && {
  bench D_dense_long3k_gpu "$DENSE" --prompts "$PROMPTS3K" -r 2
  bench D_dense_long3k_cpu "$DENSE" --prompts "$PROMPTS3K" --n-gpu-layers 0 -r 1
}
if [ -n "$MOE" ]; then
  [[ "$CASES" == *E* ]] && bench E_moe_placements "$MOE" --placements -r 2
  # F and G: another process holds all but 8 GB of VRAM, so the planner keeps experts in RAM;
  # G also caps RAM at 12 GB, below the experts' size, so part of them is read from the SSD
  if [[ "$CASES" == *[FG]* ]]; then
    python3 "$HERE/vram_hog.py" 0 8 7200 > "$OUT/FG_hog.log" 2>&1 & HOG=$!
    sleep 20
    [[ "$CASES" == *F* ]] && bench F_moe_vram8 "$MOE" -r 2
    [[ "$CASES" == *G* ]] && limited 12288 G_moe_vram8_ram12 "$MOE" -r 1
    kill $HOG 2>/dev/null; wait $HOG 2>/dev/null
  fi
fi
[[ "$CASES" == *H* ]] && limited 1400 H_dense_cpu_ram1400 "$DENSE" --n-gpu-layers 0 -r 1
log "all done"
