#!/usr/bin/env bash
# Wait until the other agent's test is over: none of its processes for 10 minutes in a row, checked every 30 s.
# Then build (buildcause.sh) and run (runcause.sh).
cd ~/ppcause
quiet=0
while [ $quiet -lt 20 ]; do
  if pgrep -af "[v]ps26|[w]orkloads|[n]eighbour|[c]luaiz serve|[c]luaiz bench|[l]lama-server|[l]lama-bench|[o]llama runner|[r]ustc|[c]argo" > /dev/null; then quiet=0; else quiet=$((quiet + 1)); fi
  sleep 30
done
echo "== quiet since $(date -u +%FT%TZ)" > ~/ppcause/wait.log
bash ~/ppcause/buildcause.sh > ~/ppcause/build.log 2>&1 || { echo "build failed" >> ~/ppcause/wait.log; exit 1; }
grep -q "build exit 0" ~/ppcause/build.log || { echo "build failed" >> ~/ppcause/wait.log; exit 1; }
echo "== run start $(date -u +%FT%TZ)" >> ~/ppcause/wait.log
bash ~/ppcause/runcause.sh
echo "== run end $(date -u +%FT%TZ)" >> ~/ppcause/wait.log
