#!/usr/bin/env bash
# Run 13c: on a GPU smaller than the model, is llama.cpp's own --fit placement (some expert layers in VRAM)
# faster or slower than the MoE cache? Official llama.cpp, llama-server. The L4 (22.4 GB free) acts as an
# 8 GB and a 4 GB GPU through --fit-target (the MiB --fit leaves free).
# Settings per model, budget and load mode:
#   fit         --fit (default on): llama.cpp picks the expert layers in VRAM, no cache
#   fit+cache   --fit with a cache of 10% of the experts (#29887): --fit counts the cache
#   cmoe+cache  -cmoe and a cache in what the budget leaves after attention and KV
# The CPU share comes from the rule drawn from run 13b: cache <= 10% of the experts 0.95, ~20% 0.9,
# ~30-40% 0.8, >= 50% 0; with locked (pinned) experts one step lower.
# Load modes: mmap (default) and mlock (the experts are pinned, docs/moe-cache.md).
# Per setting: 1 warm-up and 3 requests of 512 new tokens, llama.cpp's own timings.
# Usage: moe_cache_fit_test.sh   (results in ~/results-run13c)
set -uo pipefail
OUT="$HOME/results-run13c"; mkdir -p "$OUT"
SRC="$HOME/llama.cpp-official"
SERVER="$SRC/build/bin/llama-server"
MODELS="$HOME/ch-reg/models/chat"
step() { echo "== $(date +%H:%M:%S) $*" | tee -a "$OUT/steps.log"; }
gguf() { ls "$MODELS/$1"/*.gguf | grep -vi mmproj | head -1; }
git -C "$SRC" log -1 --format="official llama.cpp %h %ad %s" --date=short | tee -a "$OUT/steps.log"
ulimit -l unlimited 2>/dev/null || sudo prlimit --pid $$ --memlock=unlimited:unlimited
echo "memlock limit: $(ulimit -l)" | tee -a "$OUT/steps.log"
FREE=$(nvidia-smi --query-gpu=memory.free --format=csv,noheader,nounits | head -1)

# run <name> <model dir> <llama-server args...>
run() {
  local name="$1" dir="$2"; shift 2
  [ -f "$OUT/$name.json" ] && return
  step "$name"
  local log="$OUT/$name.log"
  "$SERVER" -m "$(gguf "$dir")" --port 8090 -c 8192 -t 4 -np 1 -lv 4 "$@" > "$log" 2>&1 & local pid=$!
  for i in $(seq 1 900); do curl -s -m 2 http://127.0.0.1:8090/health | grep -q ok && break; kill -0 $pid 2>/dev/null || break; sleep 1; done
  python3 - "$name" "$OUT/$name.json" "$*" <<'EOF'
import json, random, subprocess, sys, urllib.request
name, path, args = sys.argv[1:4]
rows = []
for run in range(4):
    body = {"prompt": f"Note {random.randint(0, 10**9)}. Write a long story about a lighthouse keeper who finds a ship with no crew.",
            "n_predict": 512, "temperature": 0.8, "cache_prompt": False, "ignore_eos": True}
    req = urllib.request.Request("http://127.0.0.1:8090/completion", json.dumps(body).encode(), {"Content-Type": "application/json"})
    try:
        t = json.loads(urllib.request.urlopen(req, timeout=3600).read())["timings"]
        rows.append({"run": run, "gen_n": t["predicted_n"], "gen_tps": round(t["predicted_per_second"], 2)})
    except Exception as e:
        rows.append({"run": run, "error": repr(e)[:200]})
gpu = subprocess.run(["nvidia-smi", "--query-gpu=memory.used", "--format=csv,noheader,nounits"], capture_output=True, text=True).stdout.strip()
warm = [r["gen_tps"] for r in rows[1:] if "gen_tps" in r]
json.dump({"name": name, "args": args, "runs": rows, "gpu_mib": gpu,
           "gen_tps": round(sum(warm) / len(warm), 2) if warm else None,
           "gen_min": min(warm) if warm else None, "gen_max": max(warm) if warm else None}, open(path, "w"), indent=1)
print(name, warm, gpu)
EOF
  kill $pid; wait $pid 2>/dev/null
}

# case <model> <dir> <experts MiB> <budget MiB> <non-expert MiB with -cmoe>
case_() {
  local model="$1" dir="$2" experts="$3" budget="$4" base="$5"
  local target=$(( FREE - budget ))
  local small=$(( experts / 10 / 64 * 64 ))
  local room=$(( (budget - base - 512) / 64 * 64 ))
  for lm in mmap mlock; do
    local p=0; [ "$lm" = mlock ] && p=1
    local name="${model}-${budget}-${lm}"
    run "$name-fit"         "$dir" -lm "$lm" -fitt "$target"
    run "$name-fit+cache"   "$dir" -lm "$lm" -fitt "$target" --moe-cache-mib "$small" --moe-cache-cpu-share "$(share "$small" "$experts" $p)"
    [ "$room" -ge 512 ] && run "$name-cmoe+cache" "$dir" -lm "$lm" -fitt "$target" -ngl 99 -cmoe --moe-cache-mib "$room" --moe-cache-cpu-share "$(share "$room" "$experts" $p)"
  done
}

# share <cache MiB> <experts MiB> <pinned 0|1>: the rule from run 13b
share() {
  python3 -c "
r = $1 / $2; steps = [0.95, 0.9, 0.8, 0.5, 0.0]
i = 0 if r <= 0.12 else 1 if r <= 0.25 else 2 if r <= 0.45 else 4
print(steps[min(i + $3, 4)])"
}

for budget in 8192 4096; do
  case_ qwen30b qwen3-30b-a3b-instruct-2507-gguf-Q4_K_M 16740 "$budget" 2052
  case_ gptoss20 gpt-oss-20b-gguf-FP4 9682 "$budget" 1926
done

python3 - "$OUT" <<'EOF' | tee "$OUT/results.md"
import glob, json, re, sys
print("| Setting | Gen t/s (avg, min-max) | GPU MiB | fit / cache in the log |\n|---|---|---|---|")
for p in sorted(glob.glob(sys.argv[1] + "/*.json")):
    r = json.load(open(p))
    log = open(p[:-5] + ".log", errors="replace").read()
    notes = re.findall(r"(?:fit[^\n]{0,90}n_cpu_moe[^\n]{0,40}|MoE cache size =[^\n]{0,70}|hit rate = [0-9.]+%)", log)
    hits = [n for n in notes if n.startswith("hit")]
    keep = [n for n in notes if not n.startswith("hit")][:2] + hits[-1:]
    print(f"| {r['name']} | {r['gen_tps']} ({r['gen_min']}-{r['gen_max']}) | {r['gpu_mib']} | {'; '.join(keep)} |")
EOF
step "all done"
