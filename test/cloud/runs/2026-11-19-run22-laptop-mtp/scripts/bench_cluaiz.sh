#!/bin/bash
# usage: bench_cluaiz.sh <tag>; env CLUAIZ_MTP passes through
S="${S:?set S to a work folder holding models/ and llamacpp-b12534/}"
export CLUAIZ_HOME="$S/home-mtp" CLUAIZ_PORT=8011
cd <home>/my/Cluaiz-workspace/Cluaiz-Technologies/cluaiz
./target/release/cluaiz.exe serve -v > "$S/cluaiz-$1.log" 2>&1 &
PID=$!
for i in $(seq 1 90); do curl -s -o /dev/null -w '%{http_code}' http://127.0.0.1:8011/v1/models | grep -q 200 && break; sleep 1; done
for r in 0 1 2 3; do
curl -s http://127.0.0.1:8011/v1/chat/completions -H 'Content-Type: application/json' -d '{"model":"qwen3.5-2b-mtp-gguf-Q4_K_M","messages":[{"role":"user","content":"Write a Python function that checks whether a number is prime, then explain it in three sentences."}],"temperature":0,"max_tokens":256,"think_mode":"off"}' | python -c "
import sys,json
raw=sys.stdin.read()
try: d=json.loads(raw)
except Exception: print('bad', raw[:300]); sys.exit()
if 'choices' not in d: print('err', raw[:300]); sys.exit()
u=d.get('usage',{})
print('run',sys.argv[2],'gen',u.get('completion_tokens'),'t/s',u.get('tokens_per_second'),'total_ms',u.get('total_time_ms'))
open(sys.argv[1],'w',encoding='utf-8').write(d['choices'][0]['message']['content'] or '')" "$S/out_cluaiz_$1_$r.txt" $r
done
kill $PID; sleep 2
grep -a "MTP" "$S/cluaiz-$1.log" | head -12
