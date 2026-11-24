#!/usr/bin/env bash
# Run 66 VM setup (as root): PostgreSQL + psycopg2, the run 26 paths in engines.sh, Qwen3 1.7B Q4_K_M,
# official llama.cpp b12428 (CPU build) and Ollama 0.40.16.
set -euo pipefail
export DEBIAN_FRONTEND=noninteractive
apt-get update -q
apt-get install -yq postgresql python3-psycopg2 unzip curl zstd libgomp1 >/dev/null
systemctl start postgresql
sudo -u postgres psql -qc "CREATE ROLE root SUPERUSER LOGIN" 2>/dev/null || true
sudo -u postgres createdb -O root neighbour_app 2>/dev/null || true
# Same schema and 5,000 rows as webapp/server.js setup()
psql -q neighbour_app <<'SQL'
CREATE TABLE IF NOT EXISTS items (id serial PRIMARY KEY, category text NOT NULL, payload text NOT NULL, updated_at timestamptz NOT NULL DEFAULT now());
CREATE INDEX IF NOT EXISTS idx_items_category ON items (category);
CREATE TABLE IF NOT EXISTS pings (id serial PRIMARY KEY, ts timestamptz NOT NULL DEFAULT now(), note text NOT NULL);
INSERT INTO items (category, payload) SELECT (ARRAY['orders','users','billing','events','audit'])[1 + g % 5], repeat(md5(g::text), 4)
  FROM generate_series(1, 5000) g WHERE NOT EXISTS (SELECT 1 FROM items);
SQL
psql -qtc "SELECT count(*) FROM items" neighbour_app

M=/root/.cluaiz/models/chat/qwen3-1.7b-gguf-Q4_K_M
mkdir -p "$M" /root/.cluaiz/bin
[ -s "$M/Qwen3-1.7B-Q4_K_M.gguf" ] || curl -fsSL --retry 3 -o "$M/Qwen3-1.7B-Q4_K_M.gguf" \
  https://huggingface.co/unsloth/Qwen3-1.7B-GGUF/resolve/main/Qwen3-1.7B-Q4_K_M.gguf
cat > "$M/model_manifest.json" <<'JSON'
{
  "id": "qwen3-1.7b-gguf-Q4_K_M",
  "name": "Qwen3-1.7B-GGUF",
  "architecture": "qwen3",
  "architecture_type": "gguf",
  "parameters": "1.7B",
  "training_tokens": "Unknown",
  "bit_depth": 4.0,
  "huggingface_repo": "unsloth/Qwen3-1.7B-GGUF",
  "huggingface_filename": "Qwen3-1.7B-Q4_K_M.gguf",
  "download_url": "https://huggingface.co/unsloth/Qwen3-1.7B-GGUF/resolve/main/Qwen3-1.7B-Q4_K_M.gguf",
  "description": "Model Qwen3-1.7B-GGUF (Q4_K_M)",
  "is_cloud_api": false,
  "requires_gpu": false,
  "is_free_tier": true,
  "input_modality": "chat",
  "context_window": "32k",
  "family": "Qwen3-1.7B-GGUF",
  "category": "chat",
  "assets": [],
  "local_path": null,
  "dna_path": null,
  "has_vision": false,
  "has_audio": false,
  "expert_count": null,
  "experts_per_token": null
}
JSON
sha256sum "$M/Qwen3-1.7B-Q4_K_M.gguf"

# Official llama.cpp b12428, Ubuntu x64 CPU build
L=/root/llama-official
if [ ! -x "$L/llama-b12428/llama-server" ]; then
  mkdir -p "$L/dl"
  api=$(curl -fsSL https://api.github.com/repos/ggml-org/llama.cpp/releases/tags/b12428)
  url=$(echo "$api" | grep -oE '"browser_download_url": *"[^"]*bin-ubuntu-x64\.(zip|tar\.gz)"' | head -1 | sed -E 's/.*"(https[^"]+)"/\1/')
  echo "llama.cpp asset: $url"
  case "$url" in
    *.zip) curl -fsSL --retry 3 -o "$L/dl/l.zip" "$url"; unzip -q -o "$L/dl/l.zip" -d "$L/dl/x" ;;
    *) mkdir -p "$L/dl/x"; curl -fsSL --retry 3 "$url" | tar -xz -C "$L/dl/x" ;;
  esac
  srv=$(find "$L/dl/x" -name llama-server -type f | head -1)
  mkdir -p "$L/llama-b12428"
  cp -a "$(dirname "$srv")"/. "$L/llama-b12428/"
  rm -rf "$L/dl"
fi
LD_LIBRARY_PATH=$L/llama-b12428 $L/llama-b12428/llama-server --version 2>&1 | tail -2

# Ollama 0.40.16
if [ ! -x /root/ollama-official/bin/ollama ]; then
  url=$(curl -fsSL https://api.github.com/repos/ollama/ollama/releases/tags/v0.40.16 \
    | grep -oE '"browser_download_url": *"[^"]*ollama-linux-amd64\.t[^"]*"' | head -1 | sed -E 's/.*"(https[^"]+)"/\1/')
  echo "ollama asset: $url"
  mkdir -p /root/ollama-official /root/ollama-models
  case "$url" in
    *.zst) curl -fsSL --retry 3 "$url" | tar --zstd -x -C /root/ollama-official ;;
    *) curl -fsSL --retry 3 "$url" | tar -xz -C /root/ollama-official ;;
  esac
fi
/root/ollama-official/bin/ollama --version 2>&1 | tail -1
df -h / | tail -1
echo SETUP_DONE
