#!/usr/bin/env bash
# Test A (run 13): does official llama.cpp's MoE GPU cache (--moe-cache-mib, #29887 / #30966) make the
# generation of MoE models with experts in RAM (or on the SSD) faster? Official ggml-org llama.cpp,
# llama-server, the same GGUF files the earlier runs used. Per model and setting: one warm-up request,
# then 3 requests (random number + ~500 tokens of prompt, 128 new tokens); llama.cpp's own timings.
#
# Settings, all on top of the expert placement of the case:
#   base       experts in RAM as placed (today's Cluaiz: -ngl 99 + --n-cpu-moe)
#   minbatch1  base + GGML_OP_OFFLOAD_MIN_BATCH=1 (the old Cluaiz way: every token uploads whole layers)
#   cacheS     base + --moe-cache-mib S
#   cacheS-c50 base + --moe-cache-mib S --moe-cache-cpu-share 0.5
# Cases:
#   qwen30b-cmoe   Qwen3-30B-A3B, all experts in RAM (a 4 GB laptop GPU keeps only attention): cache 2048 / 8192
#   gptoss20-cmoe  gpt-oss-20b, all experts in RAM: cache 2048 / 8192
#   gptoss120-ssd  gpt-oss-120b (59 GiB > 31 GB RAM), experts of 25 layers in RAM as Cluaiz placed it in run 11,
#                  the rest read from the SSD: cache 1024 (what VRAM has left)
# Usage: moe_cache_test.sh   (results in ~/results-run13)
set -uo pipefail
OUT="$HOME/results-run13"; mkdir -p "$OUT"
SRC="$HOME/llama.cpp-official"
SERVER="$SRC/build/bin/llama-server"
MODELS="$HOME/ch-reg/models/chat"
step() { echo "== $(date +%H:%M:%S) $*" | tee -a "$OUT/steps.log"; }

step "build official llama.cpp"
if [ ! -x "$SERVER" ]; then
  [ -d "$SRC" ] || git clone --depth 1 https://github.com/ggml-org/llama.cpp "$SRC" > "$OUT/clone.log" 2>&1
  (
    cd "$SRC" && export PATH="/usr/local/cuda/bin:$PATH" &&
    cmake -B build -DCMAKE_BUILD_TYPE=Release -DGGML_CUDA=ON -DCMAKE_CUDA_ARCHITECTURES=89 \
      -DGGML_CUDA_CCCL_VERSION=v3.4.3 -DLLAMA_BUILD_TESTS=OFF -DLLAMA_OPENSSL=OFF &&
    cmake --build build -j "$(nproc)" --target llama-server
  ) > "$OUT/build.log" 2>&1
fi
git -C "$SRC" log -1 --format="official llama.cpp %h %ad %s" --date=short | tee -a "$OUT/steps.log"
[ -x "$SERVER" ] || { step "build failed"; exit 1; }

gguf() { ls "$MODELS/$1"/*.gguf | grep -vi mmproj | head -1; }

# run <case> <setting> <model dir> <llama-server args...>
run() {
  local case="$1" setting="$2" dir="$3"; shift 3
  local log="$OUT/server_${case}_${setting}.log"
  [ -f "$OUT/${case}_${setting}.json" ] && return
  local envs=()
  [ "$setting" = minbatch1 ] && envs=(GGML_OP_OFFLOAD_MIN_BATCH=1)
  env "${envs[@]}" "$SERVER" -m "$(gguf "$dir")" --port 8090 -c 8192 -t 4 -np 1 "$@" > "$log" 2>&1 & local pid=$!
  for i in $(seq 1 900); do curl -s -m 2 http://127.0.0.1:8090/health | grep -q ok && break; sleep 1; done
  python3 - "$case" "$setting" "$OUT/${case}_${setting}.json" "$*" <<'EOF'
import json, random, subprocess, sys, urllib.request
case, setting, path, args = sys.argv[1:5]
filler = ("The river carried small stones from the mountains to the valley, and the farmers along its "
          "banks used the water for their fields in every season. ") * 17
rows = []
for run in range(4):
    body = {"prompt": f"Note {random.randint(0, 10**9)}. {filler}\nWrite a long story about this river.",
            "n_predict": 128, "temperature": 0.8, "cache_prompt": False, "ignore_eos": True}
    req = urllib.request.Request("http://127.0.0.1:8090/completion", json.dumps(body).encode(), {"Content-Type": "application/json"})
    try:
        t = json.loads(urllib.request.urlopen(req, timeout=3600).read())["timings"]
        rows.append({"run": run, "prompt_n": t["prompt_n"], "prompt_tps": round(t["prompt_per_second"], 1),
                     "gen_n": t["predicted_n"], "gen_tps": round(t["predicted_per_second"], 2)})
    except Exception as e:
        rows.append({"run": run, "error": repr(e)[:200]})
    print(json.dumps({"case": case, "setting": setting, **rows[-1]}), flush=True)
gpu = subprocess.run(["nvidia-smi", "--query-gpu=memory.used", "--format=csv,noheader,nounits"], capture_output=True, text=True).stdout.strip()
warm = [r for r in rows[1:] if "gen_tps" in r]
avg = lambda k: round(sum(r[k] for r in warm) / len(warm), 2) if warm else None
json.dump({"case": case, "setting": setting, "args": args, "runs": rows, "gen_tps": avg("gen_tps"),
           "prompt_tps": avg("prompt_tps"), "gpu_mib": gpu}, open(path, "w"), indent=1)
EOF
  kill $pid; wait $pid 2>/dev/null
  grep -iE "moe.cache|error|failed|out of memory" "$log" | head -5 >> "$OUT/notes_${case}_${setting}.txt"
}

for case in qwen30b-cmoe gptoss20-cmoe; do
  case $case in
    qwen30b-cmoe) dir=qwen3-30b-a3b-instruct-2507-gguf-Q4_K_M ;;
    gptoss20-cmoe) dir=gpt-oss-20b-gguf-FP4 ;;
  esac
  step "$case"
  run "$case" base "$dir" -ngl 99 -cmoe
  run "$case" minbatch1 "$dir" -ngl 99 -cmoe
  run "$case" cache2048 "$dir" -ngl 99 -cmoe --moe-cache-mib 2048
  run "$case" cache2048-c50 "$dir" -ngl 99 -cmoe --moe-cache-mib 2048 --moe-cache-cpu-share 0.5
  run "$case" cache8192 "$dir" -ngl 99 -cmoe --moe-cache-mib 8192
done

step "gptoss120-ssd"
dir=gpt-oss-120b-gguf-FP4
if [ ! -d "$MODELS/$dir" ]; then
  ~/.cluaiz/bin/cluaiz pull ggml-org/gpt-oss-120b-GGUF:MXFP4 </dev/null > "$OUT/pull_120b.log" 2>&1
fi
if [ -d "$MODELS/$dir" ]; then
  sync; echo 3 | sudo tee /proc/sys/vm/drop_caches >/dev/null
  run gptoss120-ssd base "$dir" -ngl 99 -ncmoe 25
  run gptoss120-ssd minbatch1 "$dir" -ngl 99 -ncmoe 25
  run gptoss120-ssd cache1024 "$dir" -ngl 99 -ncmoe 25 --moe-cache-mib 1024
  run gptoss120-ssd cache1024-c50 "$dir" -ngl 99 -ncmoe 25 --moe-cache-mib 1024 --moe-cache-cpu-share 0.5
fi

python3 - "$OUT" <<'EOF' | tee "$OUT/results.md"
import glob, json, sys
rows = [json.load(open(p)) for p in sorted(glob.glob(sys.argv[1] + "/*.json"))]
print("| Case | Setting | Prompt t/s | Generation t/s | GPU MiB |\n|---|---|---|---|---|")
for r in rows:
    print(f"| {r['case']} | {r['setting']} | {r['prompt_tps']} | {r['gen_tps']} | {r['gpu_mib']} |")
EOF
step "all done"
