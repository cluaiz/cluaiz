#!/usr/bin/env bash
# Run 72: Linux resource sampler beside test/release_soak/soak.py (its own sampler reads process RAM only on
# Windows). Every 10 s one JSON line: the `cluaiz serve` process of this package (pid, VmRSS, VmHWM, threads,
# open files), its GPU memory from nvidia-smi, and the whole GPU's used/free.
# Usage: sampler.sh <package dir> <out file>   (stop it with kill)
PKG="$1" OUT="$2"
while true; do
  pid=$(pgrep -f "^$PKG/cluaiz serve" | head -1)
  rss=null hwm=null thr=null fds=null gpu_proc=null
  if [ -n "$pid" ] && [ -r "/proc/$pid/status" ]; then
    rss=$(awk '/^VmRSS/{print $2}' "/proc/$pid/status"); hwm=$(awk '/^VmHWM/{print $2}' "/proc/$pid/status")
    thr=$(awk '/^Threads/{print $2}' "/proc/$pid/status"); fds=$(ls "/proc/$pid/fd" 2>/dev/null | wc -l)
    gpu_proc=$(nvidia-smi --query-compute-apps=pid,used_memory --format=csv,noheader,nounits | awk -F', ' -v p="$pid" '$1==p{print $2}')
    [ -z "$gpu_proc" ] && gpu_proc=null
  fi
  gpu=$(nvidia-smi --query-gpu=memory.used,memory.free,utilization.gpu --format=csv,noheader,nounits | tr -d ' ')
  echo "{\"time\": $(date +%s), \"pid\": ${pid:-null}, \"rss_kib\": ${rss:-null}, \"hwm_kib\": ${hwm:-null}, \"threads\": ${thr:-null}, \"fds\": ${fds:-null}, \"gpu_proc_mib\": $gpu_proc, \"gpu_used_free_util\": \"$gpu\"}" >> "$OUT"
  sleep 10
done
