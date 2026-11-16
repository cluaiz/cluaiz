#!/usr/bin/env bash
# Run 16: the same 1 vCPU VPS as run 15. Two parts:
#  1. Cluaiz from the new v0.1.0 release (pull fix): re-pull Ternary-Bonsai-2-4B :Q2_0 and run the run-15 pairs
#     (cluaiz bench vs official llama-bench b12428, 1 thread, prompt 512, generation 128, 3 runs) for it.
#  2. Ollama (official ollama/ollama release, standalone, own port and model folder) on the same GGUF files,
#     num_thread 1, num_gpu 0, num_ctx 4096, flash attention on, KV f16 (cpu) or q8_0 (auto, as Cluaiz's plan).
#     Ollama has no bench tool: /api/generate with a raw prompt of ~512 tokens and num_predict 128, one warm-up
#     and 3 measured runs; prompt and generation t/s come from Ollama's own prompt_eval / eval timings.
# Usage: vps_run16_ollama.sh   (results in ~/results-run16)
set -uo pipefail
OUT="$HOME/results-run16"; mkdir -p "$OUT"
step() { echo "== $(date +%H:%M:%S) $*" | tee -a "$OUT/steps.log"; }
export CLUAIZ_NONINTERACTIVE=1
CLUAIZ="$HOME/.cluaiz/bin/cluaiz"
LDIR="$HOME/llama-official"; BENCH="$(find "$LDIR" -name llama-bench | head -1)"
bench() { LD_LIBRARY_PATH="$(dirname "$BENCH")" "$BENCH" "$@"; }
COMMON=(-t 1 -p 512 -n 128 -r 3)

step "Cluaiz: new release"
curl -fsSL https://cluaiz.com/install.sh -o /tmp/cluaiz-install.sh && bash /tmp/cluaiz-install.sh > "$OUT/install.log" 2>&1 < /dev/null
"$CLUAIZ" --version 2>&1 | head -1 | tee -a "$OUT/steps.log"
sha256sum "$CLUAIZ" | tee -a "$OUT/steps.log"

step "re-pull Ternary-Bonsai-2-4B :Q2_0"
"$CLUAIZ" rm ternary-bonsai-2-4b-gguf > "$OUT/rm_tbonsai2.log" 2>&1 < /dev/null
"$CLUAIZ" pull prism-ml/Ternary-Bonsai-2-4B-gguf:Q2_0 > "$OUT/pull_tbonsai2.log" 2>&1 < /dev/null
ls -la "$HOME/.cluaiz/models/chat/"*/ | tee -a "$OUT/steps.log"

gguf_of() { ls "$HOME/.cluaiz/models/chat/$1"/*.gguf | grep -viE "mmproj|mtp|dflash" | head -1; }
id=ternary-bonsai-2-4b-gguf; gguf="$(gguf_of $id)"
if [ -n "$gguf" ]; then
  step "$id cpu: llama.cpp"
  bench -m "$gguf" -ngl 0 -ctk f16 -ctv f16 -fa auto "${COMMON[@]}" -o md > "$OUT/${id}_cpu_llama.md" 2> "$OUT/${id}_cpu_llama.log"
  step "$id cpu: Cluaiz"
  "$CLUAIZ" bench "$id" --n-gpu-layers 0 --cache-type-k f16 --cache-type-v f16 --flash-attn auto -c 4096 "${COMMON[@]}" -v > "$OUT/${id}_cpu_cluaiz.md" 2> "$OUT/${id}_cpu_cluaiz.log"
  step "$id auto: Cluaiz"
  "$CLUAIZ" bench "$id" "${COMMON[@]}" -v > "$OUT/${id}_auto_cluaiz.md" 2> "$OUT/${id}_auto_cluaiz.log"
  kv=$(grep -oE "KV Kv(16|8|4)" "$OUT/${id}_auto_cluaiz.log" | head -1 | sed 's/KV Kv16/f16/; s/KV Kv8/q8_0/; s/KV Kv4/q4_0/')
  step "$id auto: llama.cpp with KV ${kv:-f16}"
  bench -m "$gguf" -ngl 0 -ctk "${kv:-f16}" -ctv "${kv:-f16}" -fa auto "${COMMON[@]}" -o md > "$OUT/${id}_auto_llama.md" 2> "$OUT/${id}_auto_llama.log"
fi

step "Ollama: official release, standalone"
ODIR="$HOME/ollama-official"
if [ ! -x "$ODIR/bin/ollama" ]; then
  command -v zstd > /dev/null || DEBIAN_FRONTEND=noninteractive apt-get install -y zstd > "$OUT/apt.log" 2>&1
  mkdir -p "$ODIR" && curl -fsSL https://github.com/ollama/ollama/releases/latest/download/ollama-linux-amd64.tar.zst | zstd -d | tar x -C "$ODIR"
fi
export OLLAMA_HOST=127.0.0.1:11435 OLLAMA_MODELS="$HOME/ollama-models" OLLAMA_FLASH_ATTENTION=1
"$ODIR/bin/ollama" --version 2>&1 | tail -1 | tee -a "$OUT/steps.log"
OPID=""
serve() { # KV type
  [ -n "$OPID" ] && kill "$OPID" 2> /dev/null && wait "$OPID" 2> /dev/null
  OLLAMA_KV_CACHE_TYPE="$1" "$ODIR/bin/ollama" serve >> "$OUT/ollama_serve_$1.log" 2>&1 &
  OPID=$!
  for _ in $(seq 60); do curl -sf "http://$OLLAMA_HOST/api/version" > /dev/null && return; sleep 1; done
}
PROMPT="$(python3 -c 'print(" ".join(["The quick brown fox jumps over the lazy dog near the quiet river bank."] * 34))')"
measure() { # ollama model name, out file
  python3 - "$1" "$PROMPT" > "$2" <<'EOF'
import json, statistics, sys, urllib.request, os
name, prompt = sys.argv[1], sys.argv[2]
def run(i):
    body = {"model": name, "prompt": f"Run {i}. {prompt}", "raw": True, "stream": False,
            "options": {"num_predict": 128, "temperature": 0, "num_thread": 1, "num_gpu": 0, "num_ctx": 4096}}
    req = urllib.request.Request(f"http://{os.environ['OLLAMA_HOST']}/api/generate", json.dumps(body).encode(),
                                 {"Content-Type": "application/json"})
    r = json.load(urllib.request.urlopen(req, timeout=1800))
    return r["prompt_eval_count"], r["prompt_eval_count"] / r["prompt_eval_duration"] * 1e9, \
           r["eval_count"], r["eval_count"] / r["eval_duration"] * 1e9
try:
    run(0)
    rows = [run(i) for i in (1, 2, 3)]
except Exception as e:
    print(f"error: {e}"); sys.exit(0)
pp = [r[1] for r in rows]; tg = [r[3] for r in rows]
print(f"prompt tokens {[r[0] for r in rows]} generated {[r[2] for r in rows]}")
print(f"pp {statistics.mean(pp):.2f} ± {statistics.stdev(pp):.2f} | tg {statistics.mean(tg):.2f} ± {statistics.stdev(tg):.2f}")
EOF
}
for kv in f16 q8_0; do
  serve "$kv"
  mode=cpu; [ "$kv" = q8_0 ] && mode=auto
  for id in $(ls "$HOME/.cluaiz/models/chat"); do
    gguf="$(gguf_of "$id")"; [ -n "$gguf" ] || continue
    name="r16-$id"
    printf 'FROM %s\nPARAMETER num_thread 1\nPARAMETER num_gpu 0\nPARAMETER num_ctx 4096\n' "$gguf" > "$OUT/$id.Modelfile"
    "$ODIR/bin/ollama" create "$name" -f "$OUT/$id.Modelfile" > "$OUT/${id}_ollama_create.log" 2>&1
    step "$id $mode: Ollama KV $kv"
    measure "$name" "$OUT/${id}_${mode}_ollama.txt"; cat "$OUT/${id}_${mode}_ollama.txt" | tee -a "$OUT/steps.log"
  done
done
[ -n "$OPID" ] && kill "$OPID" 2> /dev/null
step "all done"
