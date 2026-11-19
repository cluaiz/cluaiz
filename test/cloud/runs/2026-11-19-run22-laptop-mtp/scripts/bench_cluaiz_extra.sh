#!/bin/bash
S="${S:?set S to a work folder holding models/ and llamacpp-b12534/}"
export CLUAIZ_HOME="$S/home-mtp" CLUAIZ_PORT=8011
cd <home>/my/Cluaiz-workspace/Cluaiz-Technologies/cluaiz
./target/release/cluaiz.exe serve -v > "$S/cluaiz-x$1.log" 2>&1 &
PID=$!
for i in $(seq 1 90); do curl -s -o /dev/null -w '%{http_code}' http://127.0.0.1:8011/v1/models | grep -q 200 && break; sleep 1; done
ask() { curl -s http://127.0.0.1:8011/v1/chat/completions -H 'Content-Type: application/json' -d "$2" | python -c "
import sys,json
d=json.loads(sys.stdin.read())
if 'choices' not in d: print('err',str(d)[:300]); sys.exit()
m=d['choices'][0]['message']; u=d.get('usage',{})
print(sys.argv[2], 'gen',u.get('completion_tokens'),'reason',u.get('completion_tokens_details',{}).get('reasoning_tokens'),'t/s',u.get('tokens_per_second'),'finish',d['choices'][0].get('finish_reason'))
open(sys.argv[1],'w',encoding='utf-8').write((m.get('reasoning_content') or '')+'\n---\n'+(m.get('content') or ''))" "$S/x_$1_$3.txt" "$3"; }
ask $1 '{"model":"qwen3.5-2b-mtp-gguf-Q4_K_M","messages":[{"role":"user","content":"What is 17 * 23? Think briefly."}],"temperature":0,"max_tokens":600,"think_mode":"low"}' think
ask $1 '{"model":"qwen3.5-2b-mtp-gguf-Q4_K_M","messages":[{"role":"user","content":"Give a JSON object for a book with title, author and year."}],"temperature":0,"max_tokens":200,"think_mode":"off","response_format":{"type":"json_schema","json_schema":{"name":"book","schema":{"type":"object","properties":{"title":{"type":"string"},"author":{"type":"string"},"year":{"type":"integer"}},"required":["title","author","year"]}}}}' json
kill $PID; sleep 2
grep -a "MTP\]" "$S/cluaiz-x$1.log" | cut -c29-
