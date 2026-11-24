#!/usr/bin/env bash
# Run 66: show where the running engine sits (cgroup, weight, cpuset, nice, CPU) and the website's processes.
# Usage (root): bash check_scope.sh <scope unit, e.g. vps26-cluaiz.scope>
u="${1:-vps26-cluaiz.scope}"
date -u +%FT%TZ
systemctl show "$u" -p ControlGroup -p AllowedCPUs -p CPUWeight -p MemoryMax
cg=/sys/fs/cgroup$(systemctl show "$u" -p ControlGroup --value)
echo "cpuset.cpus.effective: $(cat "$cg/cpuset.cpus.effective")  cpu.weight: $(cat "$cg/cpu.weight" 2>/dev/null || echo 'no cpu controller')"
echo "system.slice subtree_control: $(cat /sys/fs/cgroup/system.slice/cgroup.subtree_control)"
for p in $(cat "$cg/cgroup.procs"); do ps -L -o pid,tid,ni,psr,comm -p "$p" | head -4; done
ps -eo pid,ni,psr,cgroup:60,comm | grep -E "postgres|python3|ollama|llama-server" | grep -v grep | head -12
