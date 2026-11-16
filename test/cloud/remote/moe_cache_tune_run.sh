#!/usr/bin/env bash
# Run 13b: official llama.cpp's own tuner (scripts/moe-cache-tune.py, #30966) picks --moe-cache-mib and
# --moe-cache-cpu-share for each case, instead of sizes and shares chosen by hand (run 13).
# Same server arguments as run 13 (-c 8192 -t 4 -np 1). Each case with mmap'd experts (the default) and,
# where the model fits in RAM, with pinned experts (-lm none), as docs/moe-cache.md advises.
# gpt-oss-120b runs with -cmoe: with -ncmoe 25 the GPU has no room for the tuner's 10% cache.
# Usage: moe_cache_tune_run.sh   (results in ~/results-run13b)
set -uo pipefail
OUT="$HOME/results-run13b"; mkdir -p "$OUT"
SRC="$HOME/llama.cpp-official"
SERVER="$SRC/build/bin/llama-server"
MODELS="$HOME/ch-reg/models/chat"
step() { echo "== $(date +%H:%M:%S) $*" | tee -a "$OUT/steps.log"; }
gguf() { ls "$MODELS/$1"/*.gguf | grep -vi mmproj | head -1; }
git -C "$SRC" log -1 --format="official llama.cpp %h %ad %s" --date=short | tee -a "$OUT/steps.log"

# tune <case> <model dir> <llama-server args...>
tune() {
  local case="$1" dir="$2"; shift 2
  [ -f "$OUT/$case.md" ] && return
  step "$case"
  python3 "$SRC/scripts/moe-cache-tune.py" --out "$OUT/runs-$case" --markdown -- \
    "$SERVER" -m "$(gguf "$dir")" -c 8192 -t 4 -np 1 "$@" > "$OUT/$case.md" 2> "$OUT/$case.err"
  echo "exit $?" >> "$OUT/$case.err"
}

# one plain generation without the cache, for the 120B -cmoe baseline
plain() {
  local case="$1" dir="$2"; shift 2
  [ -f "$OUT/$case.json" ] && return
  step "$case"
  "$SERVER" -m "$(gguf "$dir")" --port 8090 -c 8192 -t 4 -np 1 "$@" > "$OUT/$case.log" 2>&1 & local pid=$!
  for i in $(seq 1 900); do curl -s -m 2 http://127.0.0.1:8090/health | grep -q ok && break; sleep 1; done
  for k in 0 1 2; do
    curl -s -m 3600 http://127.0.0.1:8090/completion -H 'Content-Type: application/json' \
      -d "{\"prompt\":\"Note $RANDOM. Write a long story about a lighthouse keeper.\",\"n_predict\":512,\"ignore_eos\":true,\"cache_prompt\":false}" |
      python3 -c 'import json,sys; t=json.load(sys.stdin)["timings"]; print(json.dumps({"gen_n":t["predicted_n"],"gen_tps":round(t["predicted_per_second"],2)}))'
  done > "$OUT/$case.json"
  kill $pid; wait $pid 2>/dev/null
}

tune qwen30b-cmoe-mmap   qwen3-30b-a3b-instruct-2507-gguf-Q4_K_M -ngl 99 -cmoe
tune qwen30b-cmoe-pinned qwen3-30b-a3b-instruct-2507-gguf-Q4_K_M -ngl 99 -cmoe -lm none
tune gptoss20-cmoe-mmap   gpt-oss-20b-gguf-FP4 -ngl 99 -cmoe
tune gptoss20-cmoe-pinned gpt-oss-20b-gguf-FP4 -ngl 99 -cmoe -lm none
plain gptoss120-cmoe-nocache gpt-oss-120b-gguf-FP4 -ngl 99 -cmoe
tune gptoss120-cmoe-mmap gpt-oss-120b-gguf-FP4 -ngl 99 -cmoe
step "all done"
