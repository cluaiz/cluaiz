#!/usr/bin/env bash
# Run 23 case G again, with the MoE expert prefetch: Qwen3.6-35B-A3B UD-IQ4_XS (18.2 GB) on the L4 VM
# (g2-standard-8), 8 GB of VRAM left by vram_hog.py, RAM capped at 12 GB by a cgroup (the experts are
# about 14 GB), page cache dropped before every run, MTP off everywhere. Adapted from run_prefetch.sh
# (gpt-oss-120b) and run 23's run23.sh (case G).
#
# Configurations (one run each per round, the order rotates every round):
#   L-bench       official llama-bench, case G's placement (-ngl 99 -ncmoe 41, plain mmap)
#   C-pin         cluaiz bench, case G's placement pinned (ngl -1, n_cpu_moe 41, ctx 16384): no MoE cache
#   C-plan        cluaiz bench, the planner's default plan, prefetch off
#   C-cache       cluaiz bench, CLUAIZ_MOE_CACHE=on, prefetch off
#   C-pf          cluaiz bench, CLUAIZ_MOE_CACHE=on, CLUAIZ_EXPERT_PREFETCH=on
#   *-text        the same five on case G's four prompts (code, chat, list, long; greedy, thinking off):
#                 llama-server /v1/chat/completions for L, `cluaiz run` (one load per prompt) for C
# Run 48 (fair baselines) adds:
#   L-bench-mc    llama-bench with upstream llama.cpp's MoE cache (--moe-cache-mib MC_MIB --moe-cache-cpu-share MC_SHARE)
#   C-off         cluaiz bench, CLUAIZ_MOE_CACHE=off (no MoE cache), prefetch off
#   C-b2048       cluaiz bench as C-plan with llama-bench's batch sizes (-b 2048, -ub 512)
#   L-serve, L-serve-mc, C-serve, C-pf-serve, B-serve: one server load each (llama-server, llama-server with
#                 the MoE cache, `cluaiz serve`, the same with the prefetch, bmoe-server --auto), case G's four
#                 prompts then three long prompts (fair_client.py)
#   B-cli         bmoe-cli --auto -n 128 on a short prompt (BigMoeOnEdge's own generation figure)
# C-pin is skipped when the default plan is already case G's placement without a cache.
# bench runs: -t 4 -p 512 -n 128,400 -r 1. Per run: /usr/bin/time -v (peak RSS, major faults), a 1 s
# sampler (free RAM, page cache, cgroup memory, VRAM, disk sectors), pgmajfault and disk MiB deltas.
#
# Usage on the VM: CLUAIZ=<binary> PROMPTS=<bench_mtp_prompts.json> bash run_case_g.sh [out_dir]
# Reads the model from ~/ch-run23 (not written: every Cluaiz run gets its own home copy under
# ~/ch-run42 with models/ linked to ~/ch-run23/models). A finished run (<tag>.done) is skipped.
set -uo pipefail
OUT="${1:-$HOME/results-run42}"; mkdir -p "$OUT"
HERE="$(cd "$(dirname "$0")" && pwd)"
SRC="${LLAMA_SRC:-$HOME/llama.cpp-official}"
BENCH="$SRC/build/bin/llama-bench"
SERVER="$SRC/build/bin/llama-server"
CLUAIZ="${CLUAIZ:-$HOME/cluaiz-pf/target/release/cluaiz}"
PROMPTS="${PROMPTS:-$HOME/cluaiz-pf/cmd/src/cli/data/bench_mtp_prompts.json}"
HOG="${HOG:-$HERE/vram_hog.py}"
CLIENT="$HERE/fair_client.py"
BMOE="${BMOE:-$HOME/BigMoeOnEdge/build/bin}"
MC=(--moe-cache-mib "${MC_MIB:-3600}" --moe-cache-cpu-share "${MC_SHARE:-0.9}")
BASE_HOME="$HOME/ch-run23"
HOMES="${HOMES:-$HOME/ch-run42}"   # run 48: HOMES=~/ch-run48 (new homes, no speed records of earlier runs)
ID=qwen3.6-35b-a3b-mtp-gguf-UD-IQ4_XS
ROUNDS="${ROUNDS:-3}"
THREADS=4
CAP_MB=12288
step() { echo "== $(date -u +%FT%TZ) $*" | tee -a "$OUT/steps.log"; }
drop() { sync; echo 3 | sudo tee /proc/sys/vm/drop_caches > /dev/null; }

step "machine"
{ uname -a; nproc; free -m; nvidia-smi --query-gpu=name,memory.total,memory.used,driver_version --format=csv; df -h "$HOME"; } > "$OUT/machine.txt" 2>&1
command -v /usr/bin/time > /dev/null || sudo DEBIAN_FRONTEND=noninteractive apt-get install -y -qq time > /dev/null 2>&1

step "official llama.cpp"
if [ ! -x "$BENCH" ]; then
  ( cd "$SRC" && export PATH="/usr/local/cuda/bin:$PATH" && cmake --build build -j "$(nproc)" --target llama-bench llama-server ) > "$OUT/build_llama.log" 2>&1
fi
[ -x "$BENCH" ] && [ -x "$SERVER" ] || { step "llama-bench/llama-server missing: $OUT/build_llama.log"; exit 1; }
git -C "$SRC" log -1 --format="official llama.cpp %h %ad %s" --date=short | tee -a "$OUT/steps.log"
"$BENCH" --version 2>&1 | grep -i version | head -1 | tee -a "$OUT/steps.log"
"$CLUAIZ" --version 2>&1 | head -1 | tee -a "$OUT/steps.log"

step "model and homes"
GGUF=$(ls "$BASE_HOME/models/chat/$ID"/*.gguf 2>/dev/null | grep -vi mmproj | head -1)
[ -n "$GGUF" ] || { step "model missing: $BASE_HOME/models/chat/$ID"; exit 1; }
ls -la "$BASE_HOME/models/chat/$ID" > "$OUT/model_files.txt"
DEV=$(basename "$(df --output=source "$GGUF" | tail -1)")
echo "model $GGUF on $DEV" | tee -a "$OUT/steps.log"
if [ ! -d "$HOMES/base" ]; then
  mkdir -p "$HOMES/base"
  (cd "$BASE_HOME" && tar --exclude=./models -cf - .) | tar -xf - -C "$HOMES/base"
  ln -s "$BASE_HOME/models" "$HOMES/base/models"
fi
du -sh "$HOMES/base" | tee -a "$OUT/steps.log"
# A home of its own for every Cluaiz run, so speed records and load history of one run cannot steer the
# planner of the next one
home_for() { [ -d "$HOMES/$1" ] || cp -a "$HOMES/base" "$HOMES/$1"; echo "$HOMES/$1"; }

step "VRAM hog"
nvidia-smi --query-gpu=memory.used --format=csv,noheader | tee -a "$OUT/steps.log"
python3 "$HOG" 0 8 30000 > "$OUT/hog.log" 2>&1 & HOGPID=$!
sleep 20
cat "$OUT/hog.log" | tee -a "$OUT/steps.log"
kill -0 "$HOGPID" 2> /dev/null || { step "vram_hog stopped: $OUT/hog.log"; exit 1; }

# capped <unit> <env assignments...> -- <command...>: run in a 12 GB cgroup scope as this user
capped() {
  local unit="$1"; shift
  local envs=()
  while [ "$1" != "--" ]; do envs+=("$1"); shift; done; shift
  sudo systemd-run --scope --quiet --unit="$unit" -p MemoryMax="${CAP_MB}M" -p MemorySwapMax=0 \
    sudo -u "$USER" env -i HOME="$HOME" PATH="$PATH" USER="$USER" CLUAIZ_PORT=8011 CLUAIZ_HOST=127.0.0.1 \
    CLUAIZ_NONINTERACTIVE=1 CLUAIZ_MTP=off "${envs[@]}" "$@"
}

# sampler <csv> <cgroup dir>: one row per second until killed
sampler() {
  echo "t,mem_avail_mib,page_cache_mib,cgroup_mib,vram_used_mib,sectors_read" > "$1"
  while :; do
    m=$(awk '/^MemAvailable/{a=$2} /^Cached/{c=$2} END{printf "%d,%d", a/1024, c/1024}' /proc/meminfo)
    g=$(( $(cat "$2/memory.current" 2> /dev/null || echo 0) / 1048576 ))
    v=$(nvidia-smi --query-gpu=memory.used --format=csv,noheader,nounits | head -1)
    s=$(awk -v d="$DEV" '$3==d{print $6}' /proc/diskstats)
    echo "$(date +%s.%N),$m,$g,$v,$s" >> "$1"
    sleep 1
  done
}
faults() { awk '/^pgmajfault /{print $2}' /proc/vmstat; }
sectors() { awk -v d="$DEV" '$3==d{print $6}' /proc/diskstats; }

# run <tag> <body function> <args...>: drop the page cache, sample, run the body, keep the deltas
run() {
  local tag="$1"; shift
  [ -f "$OUT/$tag.done" ] && return
  kill -0 "$HOGPID" 2> /dev/null || { step "vram_hog stopped before $tag"; exit 1; }
  step "$tag"
  drop
  local f0 s0 t0; f0=$(faults); s0=$(sectors); t0=$(date +%s.%N)
  sampler "$OUT/$tag.samples.csv" "/sys/fs/cgroup/system.slice/r42-$tag.scope" & local sp=$!
  "$@" "$tag"
  local rc=$?
  kill "$sp" 2> /dev/null; wait "$sp" 2> /dev/null
  echo "rc $rc, major faults $(( $(faults) - f0 )), read $(( ($(sectors) - s0) / 2048 )) MiB from disk, $(awk -v a="$t0" -v b="$(date +%s.%N)" 'BEGIN{printf "%.0f", b - a}') s" > "$OUT/$tag.delta"
  cat "$OUT/$tag.delta" | tee -a "$OUT/steps.log"
  touch "$OUT/$tag.done"
}

COMMON=(-t "$THREADS" -p "${PP:-512}" -n "${TG:-128,400}" -r "${REPS:-1}")   # PP=512,2048,4096 TG=0: prompt processing only
BARGS=("${COMMON[@]}")   # cluaiz bench arguments (the plan reads below use a short test)

# l_bench <extra args...> <tag>
l_bench() {
  local tag="${*: -1}"; local extra=("${@:1:$#-1}")
  capped "r42-$tag" -- /usr/bin/time -v -o "$OUT/$tag.time" "$BENCH" -m "$GGUF" -ngl 99 -ncmoe 41 -ctk "$KV" -ctv "$KV" \
    -fa auto "${COMMON[@]}" "${extra[@]}" -o md > "$OUT/$tag.md" 2> "$OUT/$tag.log"
}
# c_bench <env...> -- <extra args...> <tag>
c_bench() {
  local envs=(); while [ "$1" != "--" ]; do envs+=("$1"); shift; done; shift
  local tag="${*: -1}"; local extra=("${@:1:$#-1}")
  local home; home=$(home_for "$tag")
  capped "r42-$tag" CLUAIZ_HOME="$home" "${envs[@]}" -- /usr/bin/time -v -o "$OUT/$tag.time" \
    "$CLUAIZ" bench "$ID" --flash-attn auto "${BARGS[@]}" "${extra[@]}" -vv > "$OUT/$tag.md" 2> "$OUT/$tag.log"
  grep -rh "llama_cluaiz_moe_prefetch\|MoE cache size" "$home/logs" 2> /dev/null > "$OUT/$tag.homelog.txt"
  return 0
}
# c_text <env...> -- <extra args...> <tag>: case G's prompts, one `cluaiz run` (one load) each
c_text() {
  local envs=(); while [ "$1" != "--" ]; do envs+=("$1"); shift; done; shift
  local tag="${*: -1}"; local extra=("${@:1:$#-1}")
  local home; home=$(home_for "$tag")
  local script="$OUT/$tag.sh"
  {
    echo 'set -u'
    python3 - "$PROMPTS" <<'EOF'
import json, shlex, sys
for p in json.load(open(sys.argv[1])):
    print(f"echo '### prompt {p['name']}' >&2")
    print(f'printf %s {shlex.quote(p["text"])} > "$PDIR/{p["name"]}.txt"')
    print(f'"$CLUAIZ" -vv run "$ID" --local -f "$PDIR/{p["name"]}.txt" -n {p["max_tokens"]} --temp 0 --think off --mtp off "$@"')
    print("echo '### end' >&2")
EOF
  } > "$script"
  mkdir -p "$OUT/$tag.prompts"
  capped "r42-$tag" CLUAIZ_HOME="$home" CLUAIZ="$CLUAIZ" ID="$ID" PDIR="$OUT/$tag.prompts" "${envs[@]}" -- \
    /usr/bin/time -v -o "$OUT/$tag.time" bash "$script" -t "$THREADS" "${extra[@]}" > "$OUT/$tag.md" 2> "$OUT/$tag.log"
  grep -rh "llama_cluaiz_moe_prefetch\|MoE cache size" "$home/logs" 2> /dev/null > "$OUT/$tag.homelog.txt"
  return 0
}
l_text() {
  capped "r42-$1" -- /usr/bin/time -v -o "$OUT/$1.time" "$SERVER" -m "$GGUF" --host 127.0.0.1 --port 8091 -t "$THREADS" -np 1 \
    -c 16384 -ngl 99 -ncmoe 41 -ctk "$KV" -ctv "$KV" -fa auto > "$OUT/$1.server.log" 2>&1 &
  local spid=$! ok=0
  for _ in $(seq 1 1800); do curl -s -m 2 http://127.0.0.1:8091/health | grep -q ok && { ok=1; break; }; sleep 1; done
  [ "$ok" = 1 ] && python3 - "$PROMPTS" > "$OUT/$1.md" 2> "$OUT/$1.log" <<'EOF'
import json, sys, urllib.request
for p in json.load(open(sys.argv[1])):
    body = {"messages": [{"role": "user", "content": p["text"]}], "max_tokens": p["max_tokens"], "temperature": 0,
            "chat_template_kwargs": {"enable_thinking": False}, "cache_prompt": False}
    req = urllib.request.Request("http://127.0.0.1:8091/v1/chat/completions", json.dumps(body).encode(),
                                 {"Content-Type": "application/json"})
    r = json.loads(urllib.request.urlopen(req, timeout=7200).read())
    t = r["timings"]
    print(json.dumps({"prompt": p["name"], "timings": t, "text": r["choices"][0]["message"].get("content", "")}))
    print(f"| {p['name']} | prompt {t['prompt_n']} tok {t['prompt_per_second']:.2f} t/s | gen {t['predicted_n']} tok {t['predicted_per_second']:.2f} t/s |", file=sys.stderr)
EOF
  pkill -TERM -x llama-server; wait "$spid" 2> /dev/null
  return 0
}

# wait_up <port> <pid>: until the server answers /health (30 min at most, or the process ends)
wait_up() {
  for _ in $(seq 1 1800); do
    curl -sf -m 2 -o /dev/null "http://127.0.0.1:$1/health" && return 0   # HTTP 200 (llama-server: 503 while loading)
    kill -0 "$2" 2> /dev/null || return 1
    sleep 1
  done
  return 1
}
# l_serve <extra args...> <tag>: llama-server, one load, fair_client.py
l_serve() {
  local tag="${*: -1}"; local extra=("${@:1:$#-1}")
  capped "r42-$tag" -- /usr/bin/time -v -o "$OUT/$tag.time" "$SERVER" -m "$GGUF" --host 127.0.0.1 --port 8091 -t "$THREADS" -np 1 \
    -c 16384 -ngl 99 -ncmoe 41 -ctk "$KV" -ctv "$KV" -fa auto "${extra[@]}" > "$OUT/$tag.server.log" 2>&1 &
  local spid=$!
  wait_up 8091 "$spid" && python3 "$CLIENT" "$PROMPTS" 8091 llama "${TEXT_PP-512,2048,4096}" > "$OUT/$tag.md" 2> "$OUT/$tag.log"
  pkill -TERM -x llama-server; wait "$spid" 2> /dev/null
  return 0
}
# c_serve <env...> -- <tag>: one `cluaiz serve` process (port 8011) for every request
c_serve() {
  local envs=(); while [ "$1" != "--" ]; do envs+=("$1"); shift; done; shift
  local tag="$1"
  local home; home=$(home_for "$tag")
  capped "r42-$tag" CLUAIZ_HOME="$home" "${envs[@]}" -- /usr/bin/time -v -o "$OUT/$tag.time" \
    "$CLUAIZ" -vv serve > "$OUT/$tag.server.log" 2>&1 &
  local spid=$!
  wait_up 8011 "$spid" && python3 "$CLIENT" "$PROMPTS" 8011 cluaiz "${TEXT_PP-512,2048,4096}" > "$OUT/$tag.md" 2> "$OUT/$tag.log"
  pkill -TERM -f "cluaiz -vv serve"; wait "$spid" 2> /dev/null
  grep -rh "llama_cluaiz_moe_prefetch\|MoE cache size\|llama.cpp timings\|n_threads = \|n_gpu_layers = " "$home/logs" "$OUT/$tag.server.log" 2> /dev/null > "$OUT/$tag.homelog.txt"
  return 0
}
# b_serve <extra args...> <tag>: bmoe-server (BigMoeOnEdge), one load, fair_client.py
b_serve() {
  local tag="${*: -1}"; local extra=("${@:1:$#-1}")
  capped "r42-$tag" -- /usr/bin/time -v -o "$OUT/$tag.time" "$BMOE/bmoe-server" -m "$GGUF" --host 127.0.0.1 --port 8765 \
    --threads "$THREADS" -c 16384 "${extra[@]}" > "$OUT/$tag.server.log" 2>&1 &
  local spid=$!
  wait_up 8765 "$spid" && python3 "$CLIENT" "$PROMPTS" 8765 llama "${TEXT_PP-512,2048,4096}" > "$OUT/$tag.md" 2> "$OUT/$tag.log"
  pkill -TERM -x bmoe-server; wait "$spid" 2> /dev/null
  return 0
}
# b_cli <extra args...> <tag>: bmoe-cli, one load, 128 tokens after a short prompt
b_cli() {
  local tag="${*: -1}"; local extra=("${@:1:$#-1}")
  capped "r42-$tag" -- /usr/bin/time -v -o "$OUT/$tag.time" "$BMOE/bmoe-cli" -m "$GGUF" --threads "$THREADS" -c 16384 \
    -n 128 -p "Write a long detailed essay about the history of computing." "${extra[@]}" > "$OUT/$tag.md" 2> "$OUT/$tag.log"
}

step "plans"
# The plans Cluaiz makes in the capped cgroup with the hog running (bench -p 0 -n 8 -vv)
plan_line() {
  grep -E "n_gpu_layers = -?[0-9]+ \| n_cpu_moe|MoE cache( size| +:)|from disk|disk" "$OUT/$1.log" | head -8
}
BARGS=(-t "$THREADS" -p 0 -n 8 -r 1)
[ -f "$OUT/plan_default.done" ] || { drop; c_bench CLUAIZ_EXPERT_PREFETCH=off -- plan_default; touch "$OUT/plan_default.done"; }
[ -f "$OUT/plan_forced.done" ] || { drop; c_bench CLUAIZ_MOE_CACHE=on CLUAIZ_EXPERT_PREFETCH=off -- plan_forced; touch "$OUT/plan_forced.done"; }
BARGS=("${COMMON[@]}")
{ echo "default plan:"; plan_line plan_default; echo "forced-cache plan:"; plan_line plan_forced; } | tee "$OUT/plans.txt" | tee -a "$OUT/steps.log"
KV=$(grep -oE "n_ctx = [0-9]+ \| KV Kv[0-9]+" "$OUT/plan_default.log" | head -1 | grep -oE "Kv[0-9]+")
case "$KV" in Kv8) KV=q8_0 ;; Kv4) KV=q4_0 ;; *) KV=f16 ;; esac
NCTX=$(grep -oE "n_ctx = [0-9]+" "$OUT/plan_default.log" | head -1 | grep -oE "[0-9]+")
PLACE=$(grep -oE "n_gpu_layers = -?[0-9]+ \| n_cpu_moe = [0-9]+" "$OUT/plan_default.log" | head -1)
echo "KV $KV, n_ctx ${NCTX:-?}, default placement: ${PLACE:-not read}" | tee -a "$OUT/steps.log"
[ -n "$PLACE" ] || { step "no default plan read: $OUT/plan_default.log"; exit 1; }
PIN=(--n-gpu-layers all --n-cpu-moe 41 -c 16384 --cache-type-k "$KV" --cache-type-v "$KV")

configs=(L-bench C-pin C-plan C-cache C-pf L-text C-pin-text C-plan-text C-cache-text C-pf-text)
if [ "$PLACE" = "n_gpu_layers = -1 | n_cpu_moe = 41" ] && ! grep -qE "MoE cache size = +[1-9]|MoE cache +: [0-9.]*[1-9]" "$OUT/plan_default.log"; then
  configs=(L-bench C-plan C-cache C-pf L-text C-plan-text C-cache-text C-pf-text)
  step "default plan is case G's placement without a cache: C-pin skipped"
fi
# CONFIGS="L-text C-plan-text ..." runs only those configurations
[ -n "${CONFIGS:-}" ] && read -r -a configs <<< "$CONFIGS"
step "configs: ${configs[*]}"

for round in $(seq 1 "$ROUNDS"); do
  n=${#configs[@]}
  for i in $(seq 0 $((n - 1))); do
    cfg=${configs[$(( (i + round - 1) % n ))]}
    tag="r${round}_${cfg}"
    case $cfg in
      L-bench) run "$tag" l_bench ;;
      L-bench-mc) run "$tag" l_bench "${MC[@]}" ;;
      C-off) run "$tag" c_bench CLUAIZ_MOE_CACHE=off CLUAIZ_EXPERT_PREFETCH=off -- ;;
      C-b2048) run "$tag" c_bench CLUAIZ_EXPERT_PREFETCH=off -- --batch-size 2048 --ubatch-size 512 ;;
      L-serve) run "$tag" l_serve ;;
      L-serve-mc) run "$tag" l_serve "${MC[@]}" ;;
      C-serve) run "$tag" c_serve CLUAIZ_EXPERT_PREFETCH=off -- ;;
      C-pf-serve) run "$tag" c_serve CLUAIZ_EXPERT_PREFETCH=on -- ;;
      B-serve) run "$tag" b_serve ${BMOE_ARGS:---auto} ;;
      # BigMoeOnEdge on Cluaiz's placement: every layer on the GPU, experts streamed from the file,
      # a 3600 MiB device expert cache with 90% of the misses on the CPU, f16 KV, its planner off
      # Run 48: bmoe-server 0.48.0 refuses this one ("Device cache misses on the CPU ... needs Expert
      # streaming off") and never loads the model; B-serve-pin0 is the same with the misses on the GPU
      B-serve-pin) run "$tag" b_serve --no-auto --moe-stream --gpu-layers 99 --device-cache-mb 3600 \
                     --device-cache-cpu-share 0.9 --cache-type-k f16 --cache-type-v f16 ;;
      B-serve-pin0) run "$tag" b_serve --no-auto --moe-stream --gpu-layers 99 --device-cache-mb 3600 \
                     --cache-type-k f16 --cache-type-v f16 ;;
      # llama-bench built from the Cluaiz llama.cpp fork (same CMake flags as the official build), same command
      LF-bench) local_bench="$BENCH"; BENCH="${FORK_BENCH:-$HOME/fork-bench-build/bin/llama-bench}"; run "$tag" l_bench; BENCH="$local_bench" ;;
      # C-pin with a context just above pp4096 + tg128 (llama-bench sizes its context to each test)
      C-pin-c4608) run "$tag" c_bench CLUAIZ_EXPERT_PREFETCH=off -- --n-gpu-layers all --n-cpu-moe 41 -c 4608 \
                     --cache-type-k "$KV" --cache-type-v "$KV" ;;
      # Today's default: no CLUAIZ_EXPERT_PREFETCH, so the planner's Auto rule decides
      C-auto) run "$tag" c_bench -- ;;
      B-cli) run "$tag" b_cli ${BMOE_ARGS:---auto} ;;
      C-pin) run "$tag" c_bench CLUAIZ_EXPERT_PREFETCH=off -- "${PIN[@]}" ;;
      C-plan) run "$tag" c_bench CLUAIZ_EXPERT_PREFETCH=off -- ;;
      C-cache) run "$tag" c_bench CLUAIZ_MOE_CACHE=on CLUAIZ_EXPERT_PREFETCH=off -- ;;
      C-pf) run "$tag" c_bench CLUAIZ_MOE_CACHE=on CLUAIZ_EXPERT_PREFETCH=on -- ;;
      L-text) run "$tag" l_text ;;
      C-pin-text) run "$tag" c_text CLUAIZ_EXPERT_PREFETCH=off -- "${PIN[@]}" ;;
      C-plan-text) run "$tag" c_text CLUAIZ_EXPERT_PREFETCH=off -- ;;
      C-cache-text) run "$tag" c_text CLUAIZ_MOE_CACHE=on CLUAIZ_EXPERT_PREFETCH=off -- ;;
      C-pf-text) run "$tag" c_text CLUAIZ_MOE_CACHE=on CLUAIZ_EXPERT_PREFETCH=on -- ;;
    esac
  done
done
kill "$HOGPID" 2> /dev/null; wait "$HOGPID" 2> /dev/null
grep -H -E "n_gpu_layers = -?[0-9]+ \| n_cpu_moe|MoE cache size|MoE cache +:" "$OUT"/r*_C-*.log > "$OUT/placements.txt" 2> /dev/null
cat "$OUT"/r*_C-pf*.homelog.txt 2> /dev/null | grep llama_cluaiz_moe_prefetch > "$OUT/prefetch_lines.txt"
step "all done"
