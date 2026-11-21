#!/bin/bash
# Auto reasoning_effort: the ceiling follows the measured decode speed. Isolated home, port 8011.
S="<scratch>"; H="$S/api-home"
export CLUAIZ_HOME=$(cygpath -w "$H") CLUAIZ_NONINTERACTIVE=1 CLUAIZ_PORT=8011
KEY=$(grep -o '"sk-cluaiz-[^"]*"' "$H/config/permission.json" | head -1 | tr -d '"')
./target/debug/cluaiz.exe serve > "$S/auto-serve.log" 2>&1 &
for i in $(seq 1 90); do curl -s http://localhost:8011/health >/dev/null && break; sleep 1; done
M=qwen3-0.6b-gguf-Q4_K_M
ask(){ curl -s -m 300 http://localhost:8011/v1/chat/completions -H "Authorization: Bearer $KEY" -H "Content-Type: application/json" \
  -d "{\"model\":\"$M\",\"messages\":[{\"role\":\"user\",\"content\":\"$1\"}],\"think_mode\":\"on\",\"reasoning_effort\":\"auto\",\"max_tokens\":400}" \
  | python -c "import sys,json;d=json.load(sys.stdin);u=d.get('usage',{});print('usage',u.get('completion_tokens'),'reasoning',u.get('completion_tokens_details',{}).get('reasoning_tokens'),'tps',u.get('tokens_per_second'))"; }
ask "What is 2+2?"; ask "What is 17 times 23? Show the steps."; ask "A train leaves at 9:40 and arrives at 13:15. How long is the trip?"
grep -a "Auto thinking ceiling\|llama.cpp timings" "$S/auto-serve.log"
# stop only the server on 8011 (by PID)
for p in $(netstat -ano | grep ":8011 " | grep LISTENING | awk '{print $5}' | sort -u); do taskkill //PID $p //F; done
