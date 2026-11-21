#!/bin/bash
# Thinking budget closes at a line end with a wrap-up sentence. Isolated home, port 8011.
S="<scratch>"; H="$S/api-home"
export CLUAIZ_HOME=$(cygpath -w "$H") CLUAIZ_NONINTERACTIVE=1 CLUAIZ_PORT=8011
KEY=$(grep -o '"sk-cluaiz-[^"]*"' "$H/config/permission.json" | head -1 | tr -d '"')
./target/debug/cluaiz.exe serve > "$S/soft-serve.log" 2>&1 &
for i in $(seq 1 90); do curl -s http://localhost:8011/health >/dev/null && break; sleep 1; done
M=qwen3-0.6b-gguf-Q4_K_M
ask(){ curl -s -m 300 http://localhost:8011/v1/chat/completions -H "Authorization: Bearer $KEY" -H "Content-Type: application/json" \
  -d "{\"model\":\"$M\",\"messages\":[{\"role\":\"user\",\"content\":\"$2\"}],\"think_mode\":\"on\",\"reasoning_effort\":\"$1\",\"max_tokens\":600}" > "$S/soft-$3.json"; }
ask low "Find all integer solutions of x^3 + y^3 = z^3 + 1 with |x|,|y|,|z| below 20. Check every case carefully." a
ask low "Prove that the sum of the first n odd numbers is n squared, then compute the sum of odd numbers from 1 to 999 and from 1001 to 1999." b
ask 300 "Find all integer solutions of x^3 + y^3 = z^3 + 1 with |x|,|y|,|z| below 20. Check every case carefully." c
grep -a "closed at the end of a line\|llama.cpp timings" "$S/soft-serve.log"
for p in $(netstat -ano | grep ":8011 " | grep LISTENING | awk '{print $5}' | sort -u); do taskkill //PID $p //F; done
