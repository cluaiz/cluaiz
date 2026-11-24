#!/usr/bin/env bash
# Run 64 pre-check: does a fresh isolated home give the tool hint on an image request?
B=~/cluaiz-pf/target/release/cluaiz
export CLUAIZ_HOME=~/ch-run64 CLUAIZ_PORT=8011 CLUAIZ_HOST=127.0.0.1 CLUAIZ_NONINTERACTIVE=1 NO_COLOR=1
mkdir -p ~/results-run64
$B -vv serve > ~/results-run64/precheck_server.log 2>&1 & P=$!
for i in $(seq 1 120); do curl -s -o /dev/null -w "%{http_code}" http://127.0.0.1:8011/health | grep -q 200 && break; sleep 1; done
python3 - <<'PY' > ~/results-run64/precheck_prompt.json
import base64, json, urllib.request, sys
sys.path.insert(0, "/home/" + __import__("os").environ["USER"] + "/ssdpf")
from vision_hint import IMAGE, TEXT
body = {"model": "qwen3.5-0.8b-gguf-Q4_K_M", "messages": [{"role": "user", "content": [TEXT, IMAGE]}],
        "temperature": 0, "max_tokens": 32, "think_mode": "off", "return_prompt": True}
req = urllib.request.Request("http://127.0.0.1:8011/v1/chat/completions", json.dumps(body).encode(), {"Content-Type": "application/json"})
print(urllib.request.urlopen(req, timeout=300).read().decode())
PY
kill $P; wait $P 2>/dev/null
grep -o "tools_hint" ~/results-run64/precheck_prompt.json | wc -l
python3 -c "import json;d=json.load(open('$HOME/results-run64/precheck_prompt.json'));print(d['choices'][0]['message']['content']);print(json.dumps(d.get('usage')));print([k for k in d])"
