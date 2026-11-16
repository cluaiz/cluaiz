#!/usr/bin/env bash
# Run 18: everything else a 1 vCPU / 3.8 GB VPS without a GPU can run, on release v0.1.0 (re-published).
#   1 CLI: status, list, show (plan)
#   2 pull the small speech and embedding models: Kokoro-82M (TTS), moonshine-tiny (STT), embeddinggemma-300M
#   3 server on 127.0.0.1:8011: health, models, streamed chat (time to first token)
#   4 run 11's feature checks (features11.py) on the small chat models: tool call and the answer after it,
#     thinking, second turn, vision (Ternary-Bonsai-2-4B has an mmproj), embeddings, TTS, STT plain and streamed
#   5 ingest a document; MCP server over stdio (initialize, tools/list); the coding agent fixes a one-line bug
# The Cluaiz processes run in a 2.9 GB memory scope so the website container on this VPS keeps its memory.
# Usage: vps_run18_features.sh <dir with features11.py>   (results in ~/results-run18)
set -uo pipefail
HERE="$1"; OUT="$HOME/results-run18"; mkdir -p "$OUT"
step() { echo "== $(date +%H:%M:%S) $*" | tee -a "$OUT/steps.log"; }
export CLUAIZ_PORT=8011 CLUAIZ_HOST=127.0.0.1 CLUAIZ_NONINTERACTIVE=1
BIN="$HOME/.cluaiz/bin/cluaiz"
scoped() { systemd-run --scope --quiet -p MemoryMax=2900M -p MemorySwapMax=0 "$@"; }
B=http://127.0.0.1:8011

step "1 CLI"
"$BIN" --version | tee -a "$OUT/steps.log"
"$BIN" status > "$OUT/status.txt" 2>&1; "$BIN" list > "$OUT/list.txt" 2>&1
for id in $(ls "$HOME/.cluaiz/models/chat"); do "$BIN" show "$id" > "$OUT/show_$id.txt" 2>&1; done
grep -h "placement" "$OUT"/show_*.txt | tee -a "$OUT/steps.log"

step "2 pull speech and embedding models"
for id in onnx-community/Kokoro-82M-v1.0-ONNX:uint8 onnx-community/moonshine-tiny-ONNX:int8 ggml-org/embeddinggemma-300M-GGUF; do
  log="$OUT/pull_$(echo "$id" | tr '/:' '__').log"
  start=$(date +%s); "$BIN" pull "$id" < /dev/null > "$log" 2>&1
  echo "$id exit $? $(( $(date +%s) - start ))s" | tee -a "$OUT/steps.log"
done
df -h / | tail -1 | tee -a "$OUT/steps.log"

step "3 server"
scoped "$BIN" serve > "$OUT/server.log" 2>&1 & SERVER=$!
for _ in $(seq 1 120); do curl -s -m 2 "$B/health" > "$OUT/health.json" && break; sleep 1; done
cat "$OUT/health.json" | head -c 300; echo
TOKEN=$(cat "$HOME/.cluaiz/session.token")
AUTH=(-H "authorization: Bearer $TOKEN" -H "content-type: application/json")
curl -s "${AUTH[@]}" "$B/v1/models" > "$OUT/models.json"
python3 -c 'import json,sys; print("models:", [m["id"] for m in json.load(open(sys.argv[1]))["data"]])' "$OUT/models.json" | tee -a "$OUT/steps.log"
python3 - "$B" "$TOKEN" "$OUT" <<'EOF' | tee -a "$OUT/steps.log"
import json, sys, time, urllib.request
base, token, out = sys.argv[1:4]
body = {"model": "llama-3.2-3b-instruct-gguf-Q4_K_M", "stream": True, "max_tokens": 48, "temperature": 0,
        "temporary_chat": "strict", "messages": [{"role": "user", "content": "Write one sentence about rivers."}]}
req = urllib.request.Request(base + "/v1/chat/completions", json.dumps(body).encode(),
                             {"content-type": "application/json", "authorization": "Bearer " + token})
for label in ("cold (model load)", "warm"):
    start = time.time(); first = None; text = ""
    with urllib.request.urlopen(req, timeout=1800) as r:
        for raw in r:
            line = raw.decode(errors="replace").strip()
            if not line.startswith("data:") or line == "data: [DONE]":
                continue
            d = json.loads(line[5:])
            piece = ((d.get("choices") or [{}])[0].get("delta") or {}).get("content") or ""
            if piece and first is None:
                first = time.time() - start
            text += piece
    print(json.dumps({"check": f"streamed chat, {label}", "first_token_s": round(first or -1, 2),
                      "total_s": round(time.time() - start, 2), "text": text[:120]}))
EOF

step "4 features (features11.py)"
FEATURE_CHAT="llama-3.2-3b,bonsai-4b,ternary-bonsai" FEATURE_VISION="ternary-bonsai" FEATURE_TTS="kokoro" \
  python3 -u "$HERE/features11.py" "$B" "$HOME/.cluaiz" "$OUT/features" > "$OUT/features.log" 2>&1
tail -1 "$OUT/features.log" | tee -a "$OUT/steps.log"

step "5a ingest"
printf '# Rivers\n\nRivers carry water from the mountains to the sea. The Ganga is 2,525 km long.\n\n## Lakes\n\nLakes hold still water. Lake Baikal is the deepest lake on Earth.\n' > "$OUT/doc.md"
( time "$BIN" ingest "$OUT/doc.md" ) > "$OUT/ingest.log" 2>&1; echo "ingest exit $?" | tee -a "$OUT/steps.log"; tail -5 "$OUT/ingest.log"

step "5b MCP server (stdio)"
printf '%s\n' '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"run18","version":"1"}}}' \
  '{"jsonrpc":"2.0","method":"notifications/initialized"}' '{"jsonrpc":"2.0","id":2,"method":"tools/list"}' \
  | timeout 60 "$BIN" mcp-server > "$OUT/mcp.jsonl" 2> "$OUT/mcp.err"
echo "mcp exit $?" | tee -a "$OUT/steps.log"
python3 -c 'import json,sys
for l in open(sys.argv[1]):
    d=json.loads(l); r=d.get("result",{})
    print("mcp id",d.get("id"),"server" if "serverInfo" in r else "", len(r.get("tools",[])) if "tools" in r else "", d.get("error",""))' "$OUT/mcp.jsonl" | tee -a "$OUT/steps.log"

step "5c coding agent"
W="$OUT/agent-work"; rm -rf "$W"; mkdir -p "$W"
printf 'def add(a, b):\n    return a - b\n\n\nif __name__ == "__main__":\n    print(add(2, 3))\n' > "$W/add.py"
( time timeout 2400 "$BIN" agent -C "$W" -m llama-3.2-3b-instruct-gguf-Q4_K_M --mode accept-edits --ui plain --output-format json \
    -p "add.py has a bug: add(2, 3) must return 5. Fix the function in add.py. Do not run commands." ) > "$OUT/agent.log" 2>&1
echo "agent exit $?; add.py now prints: $(python3 "$W/add.py" 2>&1)" | tee -a "$OUT/steps.log"

kill "$SERVER" 2> /dev/null; wait "$SERVER" 2> /dev/null
free -m | head -2 | tee -a "$OUT/steps.log"
step "all done"
