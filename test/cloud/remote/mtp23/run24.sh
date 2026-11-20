#!/usr/bin/env bash
# Run 24: MTP Auto (load rules + per-answer guard) against off and on, one final run on the L4 VM.
# A fresh CLUAIZ_HOME, so no stored `bench --mtp-compare` result decides Auto (rule 2): the guard is
# what is tested. Models are linked from run 23's home.
#   A  dense 2B, every placement
#   D  dense 2B, a 3000-token prompt, full GPU
#   E  MoE 35B-A3B IQ4_XS, every placement
#   F  MoE with 8 GB of VRAM left (experts in RAM)
#   G  MoE with 8 GB of VRAM left and RAM capped at 12 GB (experts partly from the SSD)
# Usage on the VM: run24.sh [case letters, default ADEFG]
set -uo pipefail
HERE="$(cd "$(dirname "$0")" && pwd)"
BIN=~/cluaiz/target/release/cluaiz
OUT=~/results-run24; mkdir -p "$OUT"
export CLUAIZ_HOME=~/ch-run24 CLUAIZ_PORT=8011 CLUAIZ_HOST=127.0.0.1 CLUAIZ_NONINTERACTIVE=1
mkdir -p "$CLUAIZ_HOME/models/chat"
for m in ~/ch-run23/models/chat/*; do ln -sfn "$m" "$CLUAIZ_HOME/models/chat/"; done
# the installed-model list, without run 23's stored speeds (vram_margins.json stays fresh)
mkdir -p "$CLUAIZ_HOME/config"; cp ~/ch-run23/config/model_registry.json "$CLUAIZ_HOME/config/"
CASES="${1:-ADEFG}"
DENSE=qwen3.5-2b-mtp-gguf-Q4_K_M
MOE=qwen3.6-35b-a3b-mtp-gguf-UD-IQ4_XS
log() { echo "[$(date +%H:%M:%S)] $*" | tee -a "$OUT/steps.log"; }

bench() {
  local name="$1"; shift
  log "start $name: $*"
  nvidia-smi --query-gpu=clocks.sm,power.draw,temperature.gpu,memory.used --format=csv,noheader >> "$OUT/$name.gpu" 2>&1
  free -m | sed -n 2p >> "$OUT/$name.gpu"
  local faults0 sectors0
  faults0=$(awk '/^pgmajfault /{print $2}' /proc/vmstat)
  sectors0=$(awk '$3 ~ /^(nvme[0-9]+n[0-9]+|sd[a-z])$/ {s+=$6} END {print s}' /proc/diskstats)
  echo "cgroup $(cat /proc/self/cgroup) memory.max $(cat /sys/fs/cgroup$(cut -d: -f3 /proc/self/cgroup)/memory.max 2>/dev/null)" >> "$OUT/$name.gpu"
  "$BIN" bench "$@" --mtp-compare -v -o json > "$OUT/$name.json" 2> "$OUT/$name.err"
  local rc=$?
  echo "major faults $(( $(awk '/^pgmajfault /{print $2}' /proc/vmstat) - faults0 )), read $(( ($(awk '$3 ~ /^(nvme[0-9]+n[0-9]+|sd[a-z])$/ {s+=$6} END {print s}' /proc/diskstats) - sectors0) / 2048 )) MiB from disk" >> "$OUT/$name.gpu"
  python3 "$HERE/to_md.py" "$OUT/$name.json" > "$OUT/$name.md" 2>/dev/null
  log "end $name exit $rc"
}

limited() {
  local mb="$1"; shift
  sync; echo 3 | sudo tee /proc/sys/vm/drop_caches > /dev/null
  sudo systemd-run --scope --quiet -p MemoryMax="${mb}M" -p MemorySwapMax=0 sudo -u "$USER" env HOME="$HOME" PATH="$PATH" \
    CLUAIZ_HOME="$CLUAIZ_HOME" CLUAIZ_PORT="$CLUAIZ_PORT" CLUAIZ_HOST="$CLUAIZ_HOST" CLUAIZ_NONINTERACTIVE=1 \
    BIN="$BIN" OUT="$OUT" HERE="$HERE" bash -c "$(declare -f bench log); bench \"\$@\"" bench "$@"
}

[[ "$CASES" == *A* ]] && bench A_dense_placements "$DENSE" --placements -r 2
[[ "$CASES" == *D* ]] && bench D_dense_long3k_gpu "$DENSE" --prompts "$HERE/prompts_long3k.json" -r 2
[[ "$CASES" == *E* ]] && bench E_moe_placements "$MOE" --placements -r 2
if [[ "$CASES" == *[FG]* ]]; then
  python3 "$HERE/vram_hog.py" 0 8 9000 > "$OUT/FG_hog.log" 2>&1 & HOG=$!
  sleep 20
  [[ "$CASES" == *F* ]] && bench F_moe_vram8 "$MOE" -r 2
  [[ "$CASES" == *G* ]] && limited 12288 G_moe_vram8_ram12 "$MOE" -r 1
  kill $HOG 2>/dev/null; wait $HOG 2>/dev/null
fi
# the Auto decision and guard lines of every load and answer
grep -h "MTP\] Auto\|MTP\] Guard" "$OUT"/*.err > "$OUT/auto_and_guard.log" 2>/dev/null
log "all done"
