#!/usr/bin/env bash
# Run 59: the prompt-speed (pp) gap between `cluaiz bench` and llama-bench on run 48's shape
# (g2-standard-8, ~8 GB VRAM left by vram_hog.py, 12 GB RAM cgroup, Qwen3.6-35B-A3B UD-IQ4_XS, experts of
# all 41 blocks in RAM, no MoE cache). Same helpers as run_case_g.sh, plus the cgroup's memory.stat and the
# main process's RssAnon / RssFile every second, so the memory each tool leaves to the page cache is known.
#
# Configurations (CONFIGS picks; each run: page cache dropped, own cgroup scope):
#   LF        fork llama-bench -ngl 99 -ncmoe 41 -fa auto -t 4 (run 48's LF-bench)
#   C         cluaiz bench --n-gpu-layers all --n-cpu-moe 41 -c 16384 (run 48's C-pin), prefetch off
#   LF-b<MiB> LF with a second process in the same cgroup holding <MiB> of anonymous memory (balloon.py)
#   C-c<N>    C with -c N (llama-bench sizes its context to each test: n_prompt + n_gen)
#   C-b<N>    C with -b N (llama-bench's default n_batch is 2048, Cluaiz's 512; n_ubatch is 512 on both)
#   LFB<N>    LF with -b N
#   C-nw      C with CLUAIZ_TEST_NO_WARMUP=1 (only a test binary built with that switch skips the load warmup)
#   CN        C with CLUAIZ_ENV (any env assignments) for a fix candidate
#   CB        C with the binary CLUAIZ_BASE (before / after a fix in one session)
#
# Usage on the VM: PP=512,2048 REPS=2 ROUNDS=2 CONFIGS="LF C" bash pp_gap.sh <out_dir>
set -uo pipefail
OUT="${1:-$HOME/results-run59}"; mkdir -p "$OUT"
HERE="$(cd "$(dirname "$0")" && pwd)"
BENCH="${FORK_BENCH:-$HOME/fork-bench-build/bin/llama-bench}"
CLUAIZ="${CLUAIZ:-$HOME/cluaiz-pf/target/release/cluaiz}"
HOG="${HOG:-$HERE/vram_hog.py}"
BASE_HOME="$HOME/ch-run23"
HOMES="${HOMES:-$HOME/ch-run59}"
ID=qwen3.6-35b-a3b-mtp-gguf-UD-IQ4_XS
ROUNDS="${ROUNDS:-2}"
CONFIGS="${CONFIGS:-LF C}"
PP="${PP:-512,2048}"
TG="${TG:-0}"
REPS="${REPS:-2}"
THREADS=4
CAP_MB=12288
step() { echo "== $(date -u +%FT%TZ) $*" | tee -a "$OUT/steps.log"; }
drop() { sync; echo 3 | sudo tee /proc/sys/vm/drop_caches > /dev/null; }

step "machine"
{ uname -a; nproc; free -m; nvidia-smi --query-gpu=name,memory.total,memory.used,driver_version --format=csv; } > "$OUT/machine.txt" 2>&1
"$CLUAIZ" --version 2>&1 | head -1 | tee -a "$OUT/steps.log"
GGUF=$(ls "$BASE_HOME/models/chat/$ID"/*.gguf 2>/dev/null | grep -vi mmproj | head -1)
[ -n "$GGUF" ] || { step "model missing"; exit 1; }
DEV=$(basename "$(df --output=source "$GGUF" | tail -1)")
cat /sys/block/"${DEV%[0-9]*}"/queue/read_ahead_kb 2> /dev/null | sed 's/^/read_ahead_kb /' | tee -a "$OUT/steps.log"
if [ ! -d "$HOMES/base" ]; then
  mkdir -p "$HOMES/base"
  (cd "$BASE_HOME" && tar --exclude=./models -cf - .) | tar -xf - -C "$HOMES/base"
  ln -s "$BASE_HOME/models" "$HOMES/base/models"
fi
home_for() { [ -d "$HOMES/$1" ] || cp -a "$HOMES/base" "$HOMES/$1"; echo "$HOMES/$1"; }

if ! pgrep -f vram_hog.py > /dev/null; then
  python3 "$HOG" 0 8 30000 > "$OUT/hog.log" 2>&1 &
  sleep 20
fi
HOGPID=$(pgrep -f vram_hog.py | head -1)
nvidia-smi --query-gpu=memory.used --format=csv,noheader | tee -a "$OUT/steps.log"
[ -n "$HOGPID" ] || { step "vram_hog not running"; exit 1; }

capped() {
  local unit="$1"; shift
  local envs=()
  while [ "$1" != "--" ]; do envs+=("$1"); shift; done; shift
  sudo systemd-run --scope --quiet --unit="$unit" -p MemoryMax="${CAP_MB}M" -p MemorySwapMax=0 \
    sudo -u "$USER" env -i HOME="$HOME" PATH="$PATH" USER="$USER" CLUAIZ_PORT=8011 CLUAIZ_HOST=127.0.0.1 \
    CLUAIZ_NONINTERACTIVE=1 CLUAIZ_MTP=off "${envs[@]}" "$@"
}

# sampler <csv> <cgroup dir> <process name>: one row per second until killed
sampler() {
  echo "t,cg_mib,anon_mib,file_mib,file_mapped_mib,kernel_mib,refault_file,rss_anon_mib,rss_file_mib,threads,sectors_read,majflt,proc_majflt" > "$1"
  while :; do
    local st="$2/memory.stat" pid
    pid=$(pgrep -x "$3" | head -1)
    local cg; cg=$(( $(cat "$2/memory.current" 2> /dev/null || echo 0) / 1048576 ))
    local ms; ms=$(awk '$1=="anon"{a=$2} $1=="file"{f=$2} $1=="file_mapped"{m=$2} $1=="kernel"{k=$2} $1=="workingset_refault_file"{r=$2} END{printf "%d,%d,%d,%d,%d", a/1048576, f/1048576, m/1048576, k/1048576, r}' "$st" 2> /dev/null)
    local ps="0,0,0"
    [ -n "$pid" ] && ps=$(awk '/^RssAnon/{a=$2} /^RssFile/{f=$2} /^Threads/{t=$2} END{printf "%d,%d,%d", a/1024, f/1024, t}' /proc/"$pid"/status 2> /dev/null)
    local s; s=$(awk -v d="$DEV" '$3==d{print $6}' /proc/diskstats)
    local mf; mf=$(awk '/^pgmajfault /{print $2}' /proc/vmstat)
    local pf=0
    [ -n "$pid" ] && pf=$(awk '{print $12}' /proc/"$pid"/stat 2> /dev/null)
    echo "$(date +%s.%N),$cg,${ms:-0,0,0,0,0},$ps,$s,$mf,${pf:-0}" >> "$1"
    sleep 1
  done
}
faults() { awk '/^pgmajfault /{print $2}' /proc/vmstat; }
sectors() { awk -v d="$DEV" '$3==d{print $6}' /proc/diskstats; }

run() {
  local tag="$1" pname="$2"; shift 2
  [ -f "$OUT/$tag.done" ] && return
  kill -0 "$HOGPID" 2> /dev/null || { step "vram_hog stopped before $tag"; exit 1; }
  step "$tag"
  drop
  local f0 s0 t0; f0=$(faults); s0=$(sectors); t0=$(date +%s.%N)
  sampler "$OUT/$tag.samples.csv" "/sys/fs/cgroup/system.slice/r55-$tag.scope" "$pname" & local sp=$!
  "$@" "$tag"
  local rc=$?
  kill "$sp" 2> /dev/null; wait "$sp" 2> /dev/null
  echo "rc $rc, major faults $(( $(faults) - f0 )), read $(( ($(sectors) - s0) / 2048 )) MiB from disk, $(awk -v a="$t0" -v b="$(date +%s.%N)" 'BEGIN{printf "%.0f", b - a}') s" > "$OUT/$tag.delta"
  cat "$OUT/$tag.delta" | tee -a "$OUT/steps.log"
  touch "$OUT/$tag.done"
}

# stamp: prefix each line with the epoch time (phase boundaries line up with the sampler)
stamp() { while IFS= read -r l; do printf '%s %s\n' "$(date +%s.%N)" "$l"; done; }
# l_bench <balloon MiB> <tag>
l_bench() {
  local mib="$1" tag="$2"
  if [ "$mib" = 0 ]; then
    capped "r55-$tag" -- /usr/bin/time -v -o "$OUT/$tag.time" "$BENCH" -m "$GGUF" -ngl 99 -ncmoe 41 -ctk f16 -ctv f16 \
      -fa auto -t "$THREADS" -p "$PP" -n "$TG" -r "$REPS" ${LX:-} --progress -o md 2>&1 > "$OUT/$tag.md" | stamp > "$OUT/$tag.log"
  else
    capped "r55-$tag" -- bash -c 'python3 "$1" "$2" & b=$!; sleep 8; shift 2; "$@"; rc=$?; kill $b; exit $rc' _ \
      "$HERE/balloon.py" "$mib" /usr/bin/time -v -o "$OUT/$tag.time" "$BENCH" -m "$GGUF" -ngl 99 -ncmoe 41 -ctk f16 -ctv f16 \
      -fa auto -t "$THREADS" -p "$PP" -n "$TG" -r "$REPS" --progress -o md 2>&1 > "$OUT/$tag.md" | stamp > "$OUT/$tag.log"
  fi
}
# c_bench <env...> -- <extra args...> <tag>
c_bench() {
  local envs=(); while [ "$1" != "--" ]; do envs+=("$1"); shift; done; shift
  local tag="${*: -1}"; local extra=("${@:1:$#-1}")
  local home; home=$(home_for "$tag")
  capped "r55-$tag" CLUAIZ_HOME="$home" CLUAIZ_EXPERT_PREFETCH=off "${envs[@]}" -- /usr/bin/time -v -o "$OUT/$tag.time" \
    "$CLUAIZ" bench "$ID" --flash-attn auto -t "$THREADS" -n "$TG" -r "$REPS" --n-gpu-layers all --n-cpu-moe 41 \
    --cache-type-k f16 --cache-type-v f16 "${extra[@]}" -vv 2>&1 > "$OUT/$tag.md" | stamp > "$OUT/$tag.log"
  return 0
}

read -r -a cfgs <<< "$CONFIGS"
for r in $(seq 1 "$ROUNDS"); do
  n=${#cfgs[@]}; off=$(( (r - 1) % n ))
  for i in $(seq 0 $(( n - 1 ))); do
    c="${cfgs[$(( (i + off) % n ))]}"; tag="r${r}_$c"
    case "$c" in
      LF) run "$tag" llama-bench l_bench 0 ;;
      LF-b*) run "$tag" llama-bench l_bench "${c#LF-b}" ;;
      LFB*) LX="-b ${c#LFB}" run "$tag" llama-bench l_bench 0 ;;
      C-b*) run "$tag" "$(basename "$CLUAIZ")" c_bench -- -p "$PP" -c 16384 -b "${c#C-b}" ;;
      C-nw) run "$tag" "$(basename "$CLUAIZ")" c_bench CLUAIZ_TEST_NO_WARMUP=1 -- -p "$PP" -c 16384 ;;
      C) run "$tag" "$(basename "$CLUAIZ")" c_bench -- -p "$PP" -c 16384 ;;
      C-c*) run "$tag" "$(basename "$CLUAIZ")" c_bench -- -p "$PP" -c "${c#C-c}" ;;
      CB) CLUAIZ_SAVE="$CLUAIZ"; CLUAIZ="${CLUAIZ_BASE:?}"; run "$tag" "$(basename "$CLUAIZ")" c_bench -- -p "$PP" -c 16384; CLUAIZ="$CLUAIZ_SAVE" ;;
      CN) read -r -a cenv <<< "${CLUAIZ_ENV:-}"; run "$tag" "$(basename "$CLUAIZ")" c_bench "${cenv[@]}" -- -p "$PP" -c 16384 ;;
      *) step "unknown config $c" ;;
    esac
  done
done
step "done"
