#!/usr/bin/env bash
# Run 66: does a lower CPU priority for the engine make the website next to it faster?
# One CPU (cpu 2 of the VM) holds everything, like a 1 vCPU VPS: this harness (run 26's load client,
# per-second sampler and website checker, and the website neighbour_app.py), PostgreSQL, and the engine.
# Each arm is run 26's Qwen3 1.7B W1 soak (32 requests in flight, one slot, 1 thread, ctx 4096, Q8_0 KV,
# temperature 0, engine memory limit 3,515 MB), shortened to SECS seconds, with one engine priority.
#
# Started on the VM as root, as a system service so it sits in system.slice beside the engine and
# PostgreSQL (a website on a VPS is a service too):
#   systemd-run --unit=run66 -p AllowedCPUs=2 --setenv=CUDA_VISIBLE_DEVICES= \
#     bash /root/r66/run66.sh /root/results-run66
# ARMS="A1:cluaiz:none B:cluaiz:nice ..." picks the arms; default A, B, C, A again with Cluaiz.
set -uo pipefail
OUT="${1:-/root/results-run66}"
HERE=/root/r66/vps26
CPU="${CPU:-2}"
SECS="${SECS:-1200}"
ARMS="${ARMS:-A1:cluaiz:none B:cluaiz:nice C:cluaiz:cpuweight A2:cluaiz:none}"
PG_UNIT="${PG_UNIT:-postgresql@17-main.service}"
mkdir -p "$OUT"
[ -f "$OUT/inputs/w1_tickets.json" ] || cp -r /root/r66/inputs "$OUT/inputs"

log() { echo "$(date -u +%FT%TZ) $*" | tee -a "$OUT/run66.log"; }
# CPU state: this CPU's /proc/stat line (field 9 is steal), the cgroup controllers that decide whether
# nice and CPUWeight reach across units, and where the harness runs
cpu_note() {
  {
    echo "$(date -u +%FT%TZ) $1"
    grep -E "^cpu $|^cpu ${CPU} |^cpu${CPU} " /proc/stat
    echo "root subtree_control: $(cat /sys/fs/cgroup/cgroup.subtree_control)"
    echo "system.slice subtree_control: $(cat /sys/fs/cgroup/system.slice/cgroup.subtree_control)"
    echo "harness cgroup: $(cat /proc/self/cgroup) affinity: $(taskset -pc $$ | sed 's/.*: //')"
    echo "postgres AllowedCPUs: $(systemctl show -p AllowedCPUs --value "$PG_UNIT")"
  } >> "$OUT/cpu_stat.log"
}

systemctl set-property --runtime "$PG_UNIT" "AllowedCPUs=$CPU"
systemctl restart "$PG_UNIT"
sleep 3
log "postgres pid $(systemctl show -p MainPID --value "$PG_UNIT") on cpus $(systemctl show -p AllowedCPUs --value "$PG_UNIT"); sched_autogroup_enabled=$(cat /proc/sys/kernel/sched_autogroup_enabled)"
{ lscpu; echo; nproc; free -m; uname -a; cat /etc/os-release; } > "$OUT/machine.txt" 2>&1

# Two minutes of the website alone on this CPU, no engine, as the baseline
cpu_note baseline_start
mkdir -p "$OUT/baseline"
python3 -u "$HERE/neighbour_app.py" --port 3100 --dbname neighbour_app > "$OUT/baseline/neighbour_app.log" 2>&1 &
NB=$!
sleep 3
echo baseline_website_alone > "$OUT/baseline/phase"; echo none > "$OUT/baseline/target"; : > "$OUT/baseline/cgroup_path"
python3 -u "$HERE/sampler.py" --out-dir "$OUT/baseline" > "$OUT/baseline/sampler.log" 2>&1 &
SP=$!
sleep 120
touch "$OUT/baseline/stop"; wait "$SP"; kill "$NB"; wait "$NB" 2>/dev/null
python3 "$HERE/analyze.py" --out-dir "$OUT/baseline" > /dev/null
cpu_note baseline_end

for arm in $ARMS; do
  IFS=: read -r name eng prio <<< "$arm"
  log "== arm $name engine=$eng priority=$prio secs=$SECS"
  cpu_note "${name}_start"
  CLUAIZ_COMMIT="${CLUAIZ_COMMIT:-}" bash "$HERE/run26.sh" --mode native --engines "$eng" --models qwen3-1.7b \
    --workloads W1 --levels 32 --step-secs "$SECS" --soak-secs 0 --phase-kind soak \
    --threads 1 --ctx 4096 --kv q8_0 --slots 1 --mem-max-mb 3515 \
    --priority "$prio" --engine-cpus "$CPU" \
    --out-dir "$OUT/${name}_${eng}_${prio}" --inputs-dir "$OUT/inputs" --skip-inputs \
    > "$OUT/${name}_${eng}_${prio}.out" 2>&1
  log "== arm $name exit $?"
  cpu_note "${name}_end"
  sleep 10
done
log ALL_DONE
