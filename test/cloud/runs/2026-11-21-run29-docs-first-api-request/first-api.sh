#!/bin/bash
# First API request, on an isolated home and port 8011 (never the owner's 8000 server or ~/.cluaiz)
S="<scratch>"
E="./target/release/cluaiz.exe"
H="$S/api-home"; mkdir -p "$H"
export CLUAIZ_HOME=$(cygpath -w "$H") CLUAIZ_NONINTERACTIVE=1 CLUAIZ_PORT=8011
echo "### pull"; "$E" pull unsloth/Qwen3-0.6B-GGUF:Q4_K_M 2>&1 | tail -3
echo "### list"; "$E" list 2>&1
"$E" serve > "$S/api-serve.log" 2>&1 & SERVER=$!
for i in $(seq 1 60); do curl -s http://localhost:8011/health >/dev/null && break; sleep 1; done
echo "### health"; curl -s http://localhost:8011/health; echo
echo "### models"; curl -s http://localhost:8011/v1/models; echo
MODEL=$("$E" list 2>/dev/null | grep -io 'qwen3-0.6b[^ ]*' | head -1); echo "### model id: $MODEL"
echo "### chat"; curl -s http://localhost:8011/v1/chat/completions -H "Content-Type: application/json" -d "{\"model\":\"$MODEL\",\"messages\":[{\"role\":\"user\",\"content\":\"Name the capital of France in one word.\"}],\"think_mode\":\"off\",\"max_tokens\":20}"; echo
echo "### stream"; curl -sN http://localhost:8011/v1/chat/completions -H "Content-Type: application/json" -d "{\"model\":\"$MODEL\",\"messages\":[{\"role\":\"user\",\"content\":\"Say hi.\"}],\"think_mode\":\"off\",\"max_tokens\":10,\"stream\":true,\"stream_options\":{\"include_usage\":true}}" | head -c 3000; echo
echo "### wrong model"; curl -s -w "\nHTTP %{http_code}" http://localhost:8011/v1/chat/completions -H "Content-Type: application/json" -d '{"model":"no-such-model","messages":[{"role":"user","content":"hi"}]}'; echo
echo "### token"; "$E" token create 2>&1 | sed 's/sk-cluaiz-[a-f0-9]*/<key>/'; KEY=$(grep -o '"sk-cluaiz-[^"]*"' "$H/config/permission.json" | head -1 | tr -d '"')
echo "### no key"; curl -s -w "\nHTTP %{http_code}" http://localhost:8011/v1/models; echo
echo "### with key"; curl -s -o /dev/null -w "HTTP %{http_code}" http://localhost:8011/v1/models -H "Authorization: Bearer $KEY"; echo
kill $SERVER; sleep 2; "$E" serve >> "$S/api-serve.log" 2>\&1 \& SERVER=$!
for i in $(seq 1 60); do curl -s http://localhost:8011/health >/dev/null \&\& break; sleep 1; done
echo "### after restart, no key"; curl -s -w "
HTTP %{http_code}" http://localhost:8011/v1/models; echo
echo "### after restart, with key"; curl -s -o /dev/null -w "HTTP %{http_code}" http://localhost:8011/v1/models -H "Authorization: Bearer $KEY"; echo
echo "### health without key"; curl -s -o /dev/null -w "HTTP %{http_code}" http://localhost:8011/health; echo
kill $SERVER; sleep 1; curl -s -m 2 http://localhost:8011/health >/dev/null && echo "server still up" || echo "### server stopped"
