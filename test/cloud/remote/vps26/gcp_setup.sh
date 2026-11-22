#!/usr/bin/env bash
# Prepare a fresh Ubuntu 24.04 VM for run40.sh, laid out like the run 26 VPS (paths in engines.sh).
# Needs /root/bundle40.tar: the Cluaiz build (/root/.cluaiz/bin), official llama.cpp (/root/llama-official)
# and the model folders without weights. Weights come from the Hugging Face URLs in each model's manifest,
# Ollama from its GitHub release. Usage (as root): bash gcp_setup.sh
set -euo pipefail
export DEBIAN_FRONTEND=noninteractive
apt-get update -q
# libgomp1: official llama-server links OpenMP; without it llama.cpp did not start in run 40
apt-get install -yq postgresql python3-psycopg2 nodejs node-pg unzip curl zstd libgomp1 >/dev/null
sudo -u postgres psql -qc "CREATE ROLE root SUPERUSER LOGIN" 2>/dev/null || true
sudo -u postgres createdb -O root neighbour_app 2>/dev/null || true

tar -xf /root/bundle40.tar -C /
M=/root/.cluaiz/models
HF=https://huggingface.co
dl() { [ -s "$2" ] || curl -fsSL --retry 3 -o "$2" "$1"; }
dl "$HF/unsloth/Qwen3-0.6B-GGUF/resolve/main/Qwen3-0.6B-Q4_K_M.gguf" "$M/chat/qwen3-0.6b-gguf-Q4_K_M/Qwen3-0.6B-Q4_K_M.gguf" &
dl "$HF/unsloth/Qwen3-1.7B-GGUF/resolve/main/Qwen3-1.7B-Q4_K_M.gguf" "$M/chat/qwen3-1.7b-gguf-Q4_K_M/Qwen3-1.7B-Q4_K_M.gguf" &
dl "$HF/ggml-org/embeddinggemma-300M-GGUF/resolve/main/embeddinggemma-300M-Q8_0.gguf" "$M/embedding/embeddinggemma-300m-gguf-Q8_0/embeddinggemma-300M-Q8_0.gguf" &
dl "$HF/onnx-community/Kokoro-82M-v1.0-ONNX/resolve/main/onnx/model_uint8.onnx" "$M/tts/kokoro-82m-v1.0-onnx-UINT8/model_uint8.onnx" &
for size in tiny base; do
  for f in encoder_model_int8 decoder_model_int8 decoder_with_past_model_int8; do
    dl "$HF/onnx-community/moonshine-$size-ONNX/resolve/main/onnx/$f.onnx" "$M/stt/moonshine-$size-onnx-INT8/$f.onnx" &
  done
done

# Ollama 0.40.16, the version in run 26
url=$(curl -fsSL https://api.github.com/repos/ollama/ollama/releases/tags/v0.40.16 \
  | grep -oE '"browser_download_url": *"[^"]*ollama-linux-amd64\.t[^"]*"' | head -1 | sed -E 's/.*"(https[^"]+)"/\1/')
mkdir -p /root/ollama-official /root/ollama-models
case "$url" in
  *.zst) curl -fsSL --retry 3 "$url" | tar --zstd -x -C /root/ollama-official ;;
  *) curl -fsSL --retry 3 "$url" | tar -xz -C /root/ollama-official ;;
esac
wait
/root/ollama-official/bin/ollama --version 2>&1 | tail -1
/root/.cluaiz/bin/cluaiz --version 2>&1 | tail -1
find "$M" -name "*.gguf" -o -name "*.onnx" | xargs ls -la | awk '{print $5, $9}'
echo SETUP_DONE
