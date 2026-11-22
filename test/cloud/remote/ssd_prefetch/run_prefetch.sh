#!/usr/bin/env bash
# SSD expert prefetch speed proof (plan: .agent/plans/engine/ssd-streaming/01-research-and-architecture.md,
# section 5) on the L4 VM of runs 11, 13 and 14: gpt-oss-120b MXFP4 (59 GiB, larger than the 31 GB RAM),
# official ggml-org llama.cpp against Cluaiz, the same GGUF file, page cache dropped before every run.
#
# Configurations (one run each per round, the order rotates every round):
#   L-mmap    llama-bench, plain mmap, the placement of the Cluaiz default plan (-ngl, -ncmoe)
#   L-dio     llama-bench, the same placement with -lm dio (load-time direct I/O; skipped if not supported)
#   L-cache   the placement of the Cluaiz forced-cache plan with --moe-cache-mib / --moe-cache-cpu-share
#             of that plan: llama-bench if it has the flags, else llama-server (llama.cpp's own timings)
#   C-plan    cluaiz bench, default plan (no MoE cache for a model read from disk, so no prefetch)
#   C-cache   cluaiz bench, CLUAIZ_MOE_CACHE=on, CLUAIZ_EXPERT_PREFETCH=off
#   C-pf      cluaiz bench, CLUAIZ_MOE_CACHE=on, CLUAIZ_EXPERT_PREFETCH=on
# Each run: -t 4 -p 512 -n 128,400 -r $REPS. A sampler writes free RAM, page cache, VRAM and disk reads
# every second; /usr/bin/time -v gives the peak RSS; iostat -x (if installed) the disk queue.
#
# Usage: CLUAIZ=<cluaiz binary> bash run_prefetch.sh [out_dir]    (default ~/results-ssd-prefetch)
# A run already finished (its .done file) is skipped, so a stopped Spot VM continues where it stopped.
set -uo pipefail
OUT="${1:-$HOME/results-ssd-prefetch}"; mkdir -p "$OUT"
SRC="${LLAMA_SRC:-$HOME/llama.cpp-official}"
BENCH="$SRC/build/bin/llama-bench"
SERVER="$SRC/build/bin/llama-server"
CLUAIZ="${CLUAIZ:-$HOME/cluaiz/target/release/cluaiz}"
export CLUAIZ_HOME="${CLUAIZ_HOME:-$HOME/ch-reg}"
ID=gpt-oss-120b-gguf-FP4
ROUNDS="${ROUNDS:-3}"
REPS="${REPS:-1}"
THREADS="${THREADS:-4}"
unset CLUAIZ_MOE_CACHE CLUAIZ_EXPERT_PREFETCH
step() { echo "== $(date -u +%FT%TZ) $*" | tee -a "$OUT/steps.log"; }
drop() { sync; echo 3 | sudo tee /proc/sys/vm/drop_caches > /dev/null; }

step "machine"
{ uname -a; nproc; free -m; nvidia-smi --query-gpu=name,memory.total,memory.used,driver_version --format=csv; df -h "$CLUAIZ_HOME"; } > "$OUT/machine.txt" 2>&1
sudo DEBIAN_FRONTEND=noninteractive apt-get install -y -qq time sysstat > /dev/null 2>&1 || step "time/sysstat not installed"

step "official llama.cpp"
if [ ! -x "$BENCH" ] || [ ! -x "$SERVER" ]; then
  [ -d "$SRC" ] || git clone --depth 1 https://github.com/ggml-org/llama.cpp "$SRC" > "$OUT/clone.log" 2>&1
  (
    cd "$SRC" && export PATH="/usr/local/cuda/bin:$PATH" &&
    cmake -B build -DCMAKE_BUILD_TYPE=Release -DGGML_CUDA=ON -DCMAKE_CUDA_ARCHITECTURES=89 \
      -DLLAMA_BUILD_TESTS=OFF -DLLAMA_OPENSSL=OFF &&
    cmake --build build -j "$(nproc)" --target llama-bench llama-server
  ) > "$OUT/build_llama.log" 2>&1
fi
[ -x "$BENCH" ] || { step "llama-bench build failed: $OUT/build_llama.log"; exit 1; }
git -C "$SRC" log -1 --format="official llama.cpp %h %ad %s" --date=short | tee -a "$OUT/steps.log"
"$CLUAIZ" --version 2>&1 | head -1 | tee -a "$OUT/steps.log"
"$BENCH" --help > "$OUT/llama-bench-help.txt" 2>&1
HAS_DIO=0; grep -q -- "--load-mode" "$OUT/llama-bench-help.txt" && grep -q "dio" "$OUT/llama-bench-help.txt" && HAS_DIO=1
HAS_CACHE=0; grep -q -- "--moe-cache-mib" "$OUT/llama-bench-help.txt" && HAS_CACHE=1
echo "llama-bench: -lm dio $HAS_DIO, --moe-cache-mib $HAS_CACHE" | tee -a "$OUT/steps.log"

step "model"
MODELS="$CLUAIZ_HOME/models/chat"
if [ ! -d "$MODELS/$ID" ]; then
  "$CLUAIZ" pull ggml-org/gpt-oss-120b-GGUF:MXFP4 < /dev/null > "$OUT/pull.log" 2>&1
fi
GGUF=$(ls "$MODELS/$ID"/*.gguf 2>/dev/null | grep -viE "mmproj|mtp" | head -1)
[ -n "$GGUF" ] || { step "model missing: $MODELS/$ID"; exit 1; }
ls -la "$MODELS/$ID" > "$OUT/model_files.txt"
# the partition that holds the model, for the disk counters in /proc/diskstats
DEV=$(basename "$(df --output=source "$GGUF" | tail -1)")
echo "model $GGUF on $DEV" | tee -a "$OUT/steps.log"

# The plan Cluaiz makes (with the given env): ngl ncmoe n_ctx kv cache_mib cpu_share, from `cluaiz bench -vv`
plan_of() {
  env "$@" "$CLUAIZ" bench "$ID" -p 0 -n 8 -r 1 -vv 2>&1 | tee -a "$OUT/plans.log" | python3 -c '
import re, sys
t = sys.stdin.read()
m = re.search(r"n_gpu_layers = (-?\d+) \| n_cpu_moe = (\d+) \| n_ctx = (\d+) \| KV (Kv\w+)", t)
kv = {"Kv16": "f16", "Kv8": "q8_0", "Kv4": "q4_0"}
mib, share = 0, 0
c = re.search(r"MoE cache size = +([\d.]+) MiB for [\d.]+ MiB of host experts, (\d+)% of the misses on the CPU", t)
if c:
    mib, share = int(float(c.group(1))), int(c.group(2)) / 100
else:
    g = re.search(r"MoE cache +: ([\d.]+) GB in VRAM .*CPU share of the misses ([\d.]+)", t)
    if g:
        mib, share = int(float(g.group(1)) * 1024), float(g.group(2))
print(m.group(1), m.group(2), m.group(3), kv.get(m.group(4), "f16"), mib, share) if m else print("")'
}

step "plans"
drop
read -r NGL NCMOE NCTX KV _ _ <<< "$(plan_of CLUAIZ_MOE_CACHE=)"
drop
read -r NGL_C NCMOE_C NCTX_C KV_C CACHE_MIB SHARE <<< "$(plan_of CLUAIZ_MOE_CACHE=on)"
[ -n "${NCMOE:-}" ] && [ -n "${NCMOE_C:-}" ] || { step "no plan read: $OUT/plans.log"; exit 1; }
[ "${CACHE_MIB:-0}" -gt 0 ] || { step "the forced plan has no MoE cache (VRAM left < 256 MiB?): $OUT/plans.log"; exit 1; }
LB_NGL=$NGL; [ "$NGL" -lt 0 ] && LB_NGL=99
LB_NGL_C=$NGL_C; [ "$NGL_C" -lt 0 ] && LB_NGL_C=99
{
  echo "default plan: ngl $NGL ncmoe $NCMOE ctx $NCTX kv $KV"
  echo "forced-cache plan: ngl $NGL_C ncmoe $NCMOE_C ctx $NCTX_C kv $KV_C cache $CACHE_MIB MiB cpu share $SHARE"
} | tee -a "$OUT/steps.log"

# sampler <csv>: one row per second until killed
sampler() {
  echo "t,mem_avail_mib,page_cache_mib,vram_used_mib,sectors_read" > "$1"
  while :; do
    m=$(awk '/^MemAvailable/{a=$2} /^Cached/{c=$2} END{printf "%d,%d", a/1024, c/1024}' /proc/meminfo)
    v=$(nvidia-smi --query-gpu=memory.used --format=csv,noheader,nounits | head -1)
    s=$(awk -v d="$DEV" '$3==d{print $6}' /proc/diskstats)
    echo "$(date +%s.%N),$m,$v,$s" >> "$1"
    sleep 1
  done
}

# run <tag> <command...>: page cache dropped, sampler + iostat around the command, raw output kept
run() {
  local tag="$1"; shift
  [ -f "$OUT/$tag.done" ] && return
  step "$tag: $*"
  drop
  grep -E "^(MemAvailable|Cached)" /proc/meminfo > "$OUT/$tag.meminfo_before"
  sampler "$OUT/$tag.samples.csv" & local sp=$!
  local ip=""
  if command -v iostat > /dev/null; then iostat -x -m -y 1 > "$OUT/$tag.iostat.log" 2>&1 & ip=$!; fi
  if declare -F "$1" > /dev/null; then
    "$@" > "$OUT/$tag.md" 2> "$OUT/$tag.log"   # a shell function (server_cache): no peak RSS
  else
    /usr/bin/time -v -o "$OUT/$tag.time" "$@" > "$OUT/$tag.md" 2> "$OUT/$tag.log"
  fi
  echo "exit $?" >> "$OUT/$tag.time"
  kill "$sp" ${ip:+"$ip"} 2> /dev/null; wait "$sp" ${ip:+"$ip"} 2> /dev/null
  touch "$OUT/$tag.done"
}

COMMON=(-t "$THREADS" -p 512 -n 128,400 -r "$REPS")

# L-cache through llama-server when llama-bench has no --moe-cache-mib: one /completion per test with a
# 512-token prompt; prompt and generation t/s from llama.cpp's own timings, written as a markdown table
server_cache() {
  "$SERVER" -m "$GGUF" --port 8090 -t "$THREADS" -np 1 -c 4096 -ngl "$LB_NGL_C" -ncmoe "$NCMOE_C" \
    -ctk "$KV_C" -ctv "$KV_C" --moe-cache-mib "$CACHE_MIB" --moe-cache-cpu-share "$SHARE" & local pid=$!
  for _ in $(seq 1 1800); do curl -s -m 2 http://127.0.0.1:8090/health | grep -q ok && break; sleep 1; done
  python3 - "$REPS" <<'EOF'
import json, sys, urllib.request
reps = int(sys.argv[1])
print("| tool | test | t/s |\n| --- | --- | --- |")
for n in (128, 400):
    for _ in range(reps):
        # about 512 prompt tokens; prompt_n in the raw line gives the exact count
        body = {"prompt": "The river carried small stones to the valley. " * 52, "n_predict": n,
                "temperature": 0, "cache_prompt": False, "ignore_eos": True}
        req = urllib.request.Request("http://127.0.0.1:8090/completion", json.dumps(body).encode(),
                                     {"Content-Type": "application/json"})
        t = json.loads(urllib.request.urlopen(req, timeout=7200).read())["timings"]
        print(json.dumps(t), file=sys.stderr)
        print(f"| llama-server | pp{t['prompt_n']} | {t['prompt_per_second']:.2f} ± 0.00 |")
        print(f"| llama-server | tg{t['predicted_n']} | {t['predicted_per_second']:.2f} ± 0.00 |")
EOF
  kill "$pid"; wait "$pid" 2> /dev/null
}

configs=(L-mmap L-dio L-cache C-plan C-cache C-pf)
for round in $(seq 1 "$ROUNDS"); do
  n=${#configs[@]}
  for i in $(seq 0 $((n - 1))); do
    cfg=${configs[$(( (i + round - 1) % n ))]}
    tag="r${round}_${cfg}"
    case $cfg in
      L-mmap) run "$tag" "$BENCH" -m "$GGUF" -ngl "$LB_NGL" -ncmoe "$NCMOE" -ctk "$KV" -ctv "$KV" -fa auto "${COMMON[@]}" -o md ;;
      L-dio) [ "$HAS_DIO" = 1 ] && run "$tag" "$BENCH" -m "$GGUF" -ngl "$LB_NGL" -ncmoe "$NCMOE" -ctk "$KV" -ctv "$KV" -fa auto -lm dio "${COMMON[@]}" -o md ;;
      L-cache)
        if [ "$HAS_CACHE" = 1 ]; then
          run "$tag" "$BENCH" -m "$GGUF" -ngl "$LB_NGL_C" -ncmoe "$NCMOE_C" -ctk "$KV_C" -ctv "$KV_C" -fa auto \
            --moe-cache-mib "$CACHE_MIB" --moe-cache-cpu-share "$SHARE" "${COMMON[@]}" -o md
        else
          run "$tag" server_cache
        fi ;;
      C-plan) run "$tag" env CLUAIZ_MOE_CACHE= CLUAIZ_EXPERT_PREFETCH=off "$CLUAIZ" bench "$ID" --flash-attn auto "${COMMON[@]}" -vv ;;
      C-cache) run "$tag" env CLUAIZ_MOE_CACHE=on CLUAIZ_EXPERT_PREFETCH=off "$CLUAIZ" bench "$ID" --flash-attn auto "${COMMON[@]}" -vv ;;
      C-pf) run "$tag" env CLUAIZ_MOE_CACHE=on CLUAIZ_EXPERT_PREFETCH=on "$CLUAIZ" bench "$ID" --flash-attn auto "${COMMON[@]}" -vv ;;
    esac
  done
done

step "summary"
python3 - "$OUT" <<'EOF' | tee "$OUT/results.md"
import csv, glob, os, re, statistics, sys
out = sys.argv[1]
rows = {}
for md in sorted(glob.glob(f"{out}/r*_*.md")):
    tag = os.path.basename(md)[:-3]
    rnd, cfg = tag.split("_", 1)
    speeds = {}
    for line in open(md, errors="replace"):
        cells = [c.strip() for c in line.strip().strip("|").split("|")]
        if len(cells) >= 2 and re.fullmatch(r"(pp|tg)\d+", cells[-2]) and "±" in cells[-1]:
            speeds[cells[-2]] = float(cells[-1].split("±")[0])
    peak_rss = ""
    tf = f"{out}/{tag}.time"
    if os.path.exists(tf):
        m = re.search(r"Maximum resident set size \(kbytes\): (\d+)", open(tf).read())
        peak_rss = int(m.group(1)) // 1024 if m else ""
    vram, avail, mbps = "", "", ""
    sf = f"{out}/{tag}.samples.csv"
    if os.path.exists(sf):
        s = [r for r in csv.DictReader(open(sf)) if r["sectors_read"] and r["vram_used_mib"]]
        if len(s) > 1:
            vram = max(int(r["vram_used_mib"]) for r in s)
            avail = min(int(r["mem_avail_mib"]) for r in s)
            dt = float(s[-1]["t"]) - float(s[0]["t"])
            mbps = round((int(s[-1]["sectors_read"]) - int(s[0]["sectors_read"])) * 512 / 1e6 / dt, 1) if dt > 0 else ""
    rows.setdefault(cfg, []).append((rnd, speeds, peak_rss, vram, avail, mbps))
tests = ["pp512", "tg128", "tg400"]
print("| Config | Round | " + " | ".join(tests) + " | peak RSS MiB | peak VRAM MiB | min free RAM MiB | disk MB/s (mean) |")
print("|---|---|" + "---|" * len(tests) + "---|---|---|---|")
for cfg, runs in rows.items():
    for rnd, sp, rss, vram, avail, mbps in runs:
        # llama-server prints pp<real prompt length>: take its pp value for the pp512 column
        pp = sp.get("pp512", next((v for k, v in sp.items() if k.startswith("pp")), ""))
        vals = [pp, sp.get("tg128", ""), sp.get("tg400", "")]
        print(f"| {cfg} | {rnd} | " + " | ".join(str(v) for v in vals) + f" | {rss} | {vram} | {avail} | {mbps} |")
print("\nMedian of the rounds:\n")
print("| Config | " + " | ".join(tests) + " |\n|---|" + "---|" * len(tests))
for cfg, runs in rows.items():
    med = []
    for i, t in enumerate(tests):
        vals = []
        for _, sp, *_ in runs:
            v = sp.get(t) if t != "pp512" else sp.get("pp512", next((x for k, x in sp.items() if k.startswith("pp")), None))
            if v is not None:
                vals.append(v)
        med.append(f"{statistics.median(vals):.2f} ({min(vals):.2f}-{max(vals):.2f})" if vals else "")
    print(f"| {cfg} | " + " | ".join(med) + " |")
EOF
grep -h "llama_cluaiz_moe_prefetch" "$OUT"/r*_C-pf.log > "$OUT/prefetch_lines.txt" 2> /dev/null
# the placement and cache size each run really used (Cluaiz plans again at every load; they must match)
grep -H -E "n_gpu_layers = -?[0-9]+ \| n_cpu_moe|MoE cache size" "$OUT"/r*_C-*.log "$OUT"/r*_L-cache.log > "$OUT/placements.txt" 2> /dev/null
step "all done"
