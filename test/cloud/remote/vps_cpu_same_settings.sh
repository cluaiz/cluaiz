#!/usr/bin/env bash
# Run 15: a 1 vCPU / 4 GB VPS without a GPU (AMD EPYC 9354P, AVX-512). Cluaiz release v0.1.0 against official
# llama.cpp (ggml-org prebuilt b12428, ubuntu-x64 CPU) with the same settings and the same method.
# Per model: `cluaiz bench` and `llama-bench`, CPU, 1 thread, prompt 512, generation 128, 3 runs.
#   cpu   both forced: --n-gpu-layers 0 / -ngl 0, F16 KV, flash attention auto
#   auto  Cluaiz's planner picks; llama-bench gets the KV type the plan printed
# Usage: vps_cpu_same_settings.sh   (results in ~/results-run15)
set -uo pipefail
OUT="$HOME/results-run15"; mkdir -p "$OUT"
step() { echo "== $(date +%H:%M:%S) $*" | tee -a "$OUT/steps.log"; }
export CLUAIZ_NONINTERACTIVE=1

step "install Cluaiz (fresh)"
if [ ! -x "$HOME/.cluaiz/bin/cluaiz" ]; then
  curl -fsSL https://cluaiz.com/install.sh -o /tmp/cluaiz-install.sh && bash /tmp/cluaiz-install.sh > "$OUT/install.log" 2>&1 < /dev/null
fi
CLUAIZ="$HOME/.cluaiz/bin/cluaiz"
"$CLUAIZ" --version 2>&1 | head -1 | tee -a "$OUT/steps.log"

step "official llama.cpp b12428"
LDIR="$HOME/llama-official"
if [ ! -d "$LDIR" ]; then
  mkdir -p "$LDIR" && curl -fsSL https://github.com/ggml-org/llama.cpp/releases/download/b12428/llama-b12428-bin-ubuntu-x64.tar.gz | tar xz -C "$LDIR"
fi
BENCH="$(find "$LDIR" -name llama-bench | head -1)"
dpkg -s libgomp1 > /dev/null 2>&1 || DEBIAN_FRONTEND=noninteractive apt-get install -y libgomp1 > "$OUT/apt.log" 2>&1
[ -x "$BENCH" ] || { step "llama-bench missing"; exit 1; }
# llama-bench gets its own libraries; Cluaiz must keep loading the ones beside its own binary
bench() { LD_LIBRARY_PATH="$(dirname "$BENCH")" "$BENCH" "$@"; }
bench --version 2>&1 | head -2 | tee -a "$OUT/steps.log"

step "models"
"$CLUAIZ" pull prism-ml/Bonsai-4B-gguf:Q1_0 > "$OUT/pull_bonsai4b.log" 2>&1 < /dev/null
"$CLUAIZ" pull prism-ml/Ternary-Bonsai-2-4B-gguf:Q2_0 > "$OUT/pull_tbonsai2.log" 2>&1 < /dev/null
"$CLUAIZ" pull bartowski/Llama-3.2-3B-Instruct-GGUF:Q4_K_M > "$OUT/pull_llama3b.log" 2>&1 < /dev/null
"$CLUAIZ" list > "$OUT/models.txt" 2>&1
cat "$OUT/models.txt"

COMMON=(-t 1 -p 512 -n 128 -r 3)
for id in $(ls "$HOME/.cluaiz/models/chat"); do
  gguf="$(ls "$HOME/.cluaiz/models/chat/$id"/*.gguf | grep -viE "mmproj|mtp|dflash" | head -1)"
  [ -n "$gguf" ] || continue
  step "$id cpu: llama.cpp"
  bench -m "$gguf" -ngl 0 -ctk f16 -ctv f16 -fa auto "${COMMON[@]}" -o md > "$OUT/${id}_cpu_llama.md" 2> "$OUT/${id}_cpu_llama.log"
  step "$id cpu: Cluaiz"
  "$CLUAIZ" bench "$id" --n-gpu-layers 0 --cache-type-k f16 --cache-type-v f16 --flash-attn auto -c 4096 "${COMMON[@]}" -v > "$OUT/${id}_cpu_cluaiz.md" 2> "$OUT/${id}_cpu_cluaiz.log"
  step "$id auto: Cluaiz"
  "$CLUAIZ" bench "$id" "${COMMON[@]}" -v > "$OUT/${id}_auto_cluaiz.md" 2> "$OUT/${id}_auto_cluaiz.log"
  kv=$(grep -oE "KV Kv(16|8|4)" "$OUT/${id}_auto_cluaiz.log" | head -1 | sed 's/KV Kv16/f16/; s/KV Kv8/q8_0/; s/KV Kv4/q4_0/')
  step "$id auto: llama.cpp with KV ${kv:-f16}"
  bench -m "$gguf" -ngl 0 -ctk "${kv:-f16}" -ctv "${kv:-f16}" -fa auto "${COMMON[@]}" -o md > "$OUT/${id}_auto_llama.md" 2> "$OUT/${id}_auto_llama.log"
done

for f in "$OUT"/*_cpu_*.md "$OUT"/*_auto_*.md; do echo "### $(basename "$f" .md)"; grep -E "^\|" "$f"; done > "$OUT/results.md"
step "all done"
