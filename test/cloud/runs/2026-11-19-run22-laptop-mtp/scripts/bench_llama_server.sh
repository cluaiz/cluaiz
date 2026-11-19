#!/bin/bash
# usage: bench_llama_server.sh <extra args...>; prints timings of 3 greedy runs
S="${S:?set S to a work folder holding models/ and llamacpp-b12534/}"
"$S/llamacpp-b12534/llama-server.exe" -m "$S/models/Qwen3.5-2B-Q4_K_M.gguf" -ngl 99 -c 4096 --port 8012 --host 127.0.0.1 "$@" > "$S/llama-server.log" 2>&1 &
PID=$!
for i in $(seq 1 90); do curl -s http://127.0.0.1:8012/health | grep -q ok && break; sleep 1; done
for r in 1 2 3; do
curl -s http://127.0.0.1:8012/v1/chat/completions -H 'Content-Type: application/json' -d '{"messages":[{"role":"user","content":"Write a Python function that checks whether a number is prime, then explain it in three sentences."}],"temperature":0,"max_tokens":256,"chat_template_kwargs":{"enable_thinking":false}}' | python -c "import sys,json;d=json.load(sys.stdin);t=d.get('timings',{});print('gen',t.get('predicted_n'),'tok',round(t.get('predicted_per_second',0),1),'t/s','draft',t.get('draft_n'),'acc',t.get('draft_n_accepted'));open(sys.argv[1],'w',encoding='utf-8').write(d['choices'][0]['message']['content'])" "$S/out_llama_$r.txt"
done
kill $PID
