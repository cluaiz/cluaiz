#!/usr/bin/env bash
# Engine lifecycle helpers for vps26 (Native via systemd-run --scope, Docker via --memory/--cpus).
# All memory limits, threads, context, KV cache type, and parallel slots are parameterized.

PORT_CLUAIZ=8011
PORT_OLLAMA=11435
PORT_LLAMA_CHAT=8012
PORT_LLAMA_EMBED=8013

CLUAIZ_BIN="/root/.cluaiz/bin/cluaiz"
CLUAIZ_HOME_NATIVE="/root/vps26-cluaiz-home"
CLUAIZ_HOME_DOCKER="/root/vps26-cluaiz-docker-home"
OLLAMA_BIN="/root/ollama-official/bin/ollama"
OLLAMA_MODELS_DIR="/root/ollama-models"
LLAMA_DIR="/root/llama-official/llama-b12428"
LLAMA_BIN="$LLAMA_DIR/llama-server"

model_gguf_host() {
  case "$1" in
    qwen3-0.6b) echo "/root/.cluaiz/models/chat/qwen3-0.6b-gguf-Q4_K_M/Qwen3-0.6B-Q4_K_M.gguf" ;;
    qwen3-1.7b) echo "/root/.cluaiz/models/chat/qwen3-1.7b-gguf-Q4_K_M/Qwen3-1.7B-Q4_K_M.gguf" ;;
    llama3.2-3b) echo "/root/.cluaiz/models/chat/llama-3.2-3b-instruct-gguf-Q4_K_M/Llama-3.2-3B-Instruct-Q4_K_M.gguf" ;;
    embed-gemma) echo "/root/.cluaiz/models/embedding/embeddinggemma-300m-gguf-Q8_0/embeddinggemma-300M-Q8_0.gguf" ;;
    *) echo "Unknown model key: $1" >&2; return 1 ;;
  esac
}

model_gguf_docker() {
  case "$1" in
    qwen3-0.6b) echo "/models/chat/qwen3-0.6b-gguf-Q4_K_M/Qwen3-0.6B-Q4_K_M.gguf" ;;
    qwen3-1.7b) echo "/models/chat/qwen3-1.7b-gguf-Q4_K_M/Qwen3-1.7B-Q4_K_M.gguf" ;;
    llama3.2-3b) echo "/models/chat/llama-3.2-3b-instruct-gguf-Q4_K_M/Llama-3.2-3B-Instruct-Q4_K_M.gguf" ;;
    embed-gemma) echo "/models/embedding/embeddinggemma-300m-gguf-Q8_0/embeddinggemma-300M-Q8_0.gguf" ;;
    *) echo "Unknown model key: $1" >&2; return 1 ;;
  esac
}

model_cluaiz_id() {
  case "$1" in
    qwen3-0.6b) echo "qwen3-0.6b-gguf-Q4_K_M" ;;
    qwen3-1.7b) echo "qwen3-1.7b-gguf-Q4_K_M" ;;
    llama3.2-3b) echo "llama-3.2-3b-instruct-gguf-Q4_K_M" ;;
    embed-gemma) echo "embeddinggemma-300m-gguf-Q8_0" ;;
    kokoro) echo "kokoro-82m-v1.0-onnx-UINT8" ;;
    moonshine-tiny) echo "moonshine-tiny-onnx-INT8" ;;
    moonshine-base) echo "moonshine-base-onnx-INT8" ;;
    *) echo "Unknown cluaiz model key: $1" >&2; return 1 ;;
  esac
}

model_ollama_id() {
  case "$1" in
    qwen3-0.6b) echo "vps26-qwen3-0.6b" ;;
    qwen3-1.7b) echo "vps26-qwen3-1.7b" ;;
    llama3.2-3b) echo "vps26-llama3.2-3b" ;;
    embed-gemma) echo "vps26-embed-gemma" ;;
    *) echo "Unknown ollama model key: $1" >&2; return 1 ;;
  esac
}

cluaiz_kv_enum() {
  case "$1" in
    f16) echo "Kv16" ;;
    q8_0) echo "Kv8" ;;
    q4_0) echo "Kv4" ;;
    *) echo "Auto" ;;
  esac
}

kill_port_pid() {
  local port="$1"
  local pids
  pids=$(ss -ltnpH "sport = :$port" 2>/dev/null | grep -oE 'pid=[0-9]+' | cut -d= -f2 | sort -u || true)
  for p in $pids; do
    [ -n "$p" ] || continue
    kill "$p" 2>/dev/null || true
    for _ in $(seq 1 20); do
      kill -0 "$p" 2>/dev/null || break
      sleep 0.5
    done
    kill -9 "$p" 2>/dev/null || true
  done
}

stop_all_engines() {
  local out_dir="${1:-}"
  for cname in vps26-cluaiz vps26-ollama vps26-llama; do
    docker rm -f "$cname" >/dev/null 2>&1 || true
  done
  for unit in vps26-cluaiz.scope vps26-ollama.scope vps26-llama.scope; do
    systemctl stop "$unit" >/dev/null 2>&1 || true
    systemctl reset-failed "$unit" >/dev/null 2>&1 || true
  done
  for port in "$PORT_CLUAIZ" "$PORT_OLLAMA" "$PORT_LLAMA_CHAT" "$PORT_LLAMA_EMBED"; do
    kill_port_pid "$port"
  done
  if [ -n "$out_dir" ]; then
    echo "none" > "$out_dir/target"
    echo "" > "$out_dir/cgroup_path"
  fi
}

configure_cluaiz_home() {
  local threads="$1" ctx="$2" kv="$3"
  local kv_enum
  kv_enum=$(cluaiz_kv_enum "$kv")
  mkdir -p "$CLUAIZ_HOME_NATIVE" "$CLUAIZ_HOME_DOCKER/config" "$CLUAIZ_HOME_DOCKER/bin" "$CLUAIZ_HOME_DOCKER/models"
  [ -L "$CLUAIZ_HOME_NATIVE/bin" ] || ln -sfn /root/.cluaiz/bin "$CLUAIZ_HOME_NATIVE/bin"
  [ -L "$CLUAIZ_HOME_NATIVE/models" ] || ln -sfn /root/.cluaiz/models "$CLUAIZ_HOME_NATIVE/models"
  CLUAIZ_HOME="$CLUAIZ_HOME_NATIVE" CLUAIZ_NONINTERACTIVE=1 "$CLUAIZ_BIN" config set optimization.custom_context_window "$ctx" >/dev/null
  CLUAIZ_HOME="$CLUAIZ_HOME_NATIVE" CLUAIZ_NONINTERACTIVE=1 "$CLUAIZ_BIN" config set optimization.n_gpu_layers 0 >/dev/null
  CLUAIZ_HOME="$CLUAIZ_HOME_NATIVE" CLUAIZ_NONINTERACTIVE=1 "$CLUAIZ_BIN" config set optimization.n_threads "$threads" >/dev/null
  CLUAIZ_HOME="$CLUAIZ_HOME_NATIVE" CLUAIZ_NONINTERACTIVE=1 "$CLUAIZ_BIN" config set optimization.n_threads_batch "$threads" >/dev/null
  CLUAIZ_HOME="$CLUAIZ_HOME_NATIVE" CLUAIZ_NONINTERACTIVE=1 "$CLUAIZ_BIN" config set optimization.kv_cache_quantization "$kv_enum" >/dev/null
  CLUAIZ_HOME="$CLUAIZ_HOME_NATIVE" CLUAIZ_NONINTERACTIVE=1 "$CLUAIZ_BIN" config set optimization.flash_attention Auto >/dev/null
  CLUAIZ_HOME="$CLUAIZ_HOME_NATIVE" CLUAIZ_NONINTERACTIVE=1 "$CLUAIZ_BIN" config set optimization.think_mode off >/dev/null
  CLUAIZ_HOME="$CLUAIZ_HOME_NATIVE" CLUAIZ_NONINTERACTIVE=1 "$CLUAIZ_BIN" config set chat_memory.enabled false >/dev/null
  cp "$CLUAIZ_HOME_NATIVE/config/settings.json" "$CLUAIZ_HOME_DOCKER/config/settings.json"
  chmod -R a+rX "$CLUAIZ_HOME_NATIVE" "$CLUAIZ_HOME_DOCKER"
}

sync_ollama_modelfiles() {
  local threads="$1" ctx="$2"
  local stamp="$OLLAMA_MODELS_DIR/.vps26_params"
  if [ -f "$stamp" ] && [ "$(cat "$stamp")" = "${threads}_${ctx}" ] && [ -d "$OLLAMA_MODELS_DIR/manifests" ]; then
    local count
    count=$(find "$OLLAMA_MODELS_DIR/manifests" -type d -name 'vps26-*' 2>/dev/null | wc -l)
    local want=0 m
    for m in qwen3-0.6b qwen3-1.7b llama3.2-3b embed-gemma; do [ -f "$(model_gguf_host "$m")" ] && want=$((want + 1)); done
    if [ "$count" -ge "$want" ]; then
      echo "Ollama models already synced for threads=$threads ctx=$ctx"
      return 0
    fi
  fi
  export OLLAMA_HOST="127.0.0.1:$PORT_OLLAMA" OLLAMA_MODELS="$OLLAMA_MODELS_DIR"
  kill_port_pid "$PORT_OLLAMA"
  "$OLLAMA_BIN" serve >/dev/null 2>&1 &
  local opid=$!
  for _ in $(seq 1 30); do
    curl -sf "http://$OLLAMA_HOST/api/version" >/dev/null && break
    sleep 1
  done
  for mkey in qwen3-0.6b qwen3-1.7b llama3.2-3b embed-gemma; do
    local gguf mname mf
    gguf=$(model_gguf_host "$mkey")
    # A machine set up for fewer models (gcp_setup.sh has no Llama 3.2 3B) skips the missing ones
    [ -f "$gguf" ] || { echo "Ollama sync: skip $mkey, no GGUF at $gguf"; continue; }
    mname=$(model_ollama_id "$mkey")
    mf="/tmp/${mname}.Modelfile"
    printf 'FROM %s\nPARAMETER num_ctx %s\nPARAMETER num_gpu 0\nPARAMETER num_thread %s\nPARAMETER temperature 0\n' "$gguf" "$ctx" "$threads" > "$mf"
    "$OLLAMA_BIN" create "$mname" -f "$mf" >/dev/null
    rm -f "$mf"
  done
  kill "$opid" 2>/dev/null || true
  wait "$opid" 2>/dev/null || true
  echo "${threads}_${ctx}" > "$stamp"
  chmod -R a+rX "$OLLAMA_MODELS_DIR"
}

wait_http() {
  local url="$1" secs="${2:-120}"
  for _ in $(seq 1 "$secs"); do
    curl -sf -m 2 "$url" >/dev/null 2>&1 && return 0
    sleep 0.5
  done
  return 1
}

set_cgroup_hint_native() {
  local unit="$1" out_dir="$2"
  local cg
  cg=$(systemctl show -p ControlGroup --value "$unit" 2>/dev/null || true)
  if [ -n "$cg" ] && [ -d "/sys/fs/cgroup$cg" ]; then
    echo "/sys/fs/cgroup$cg" > "$out_dir/cgroup_path"
  else
    echo "" > "$out_dir/cgroup_path"
  fi
}

set_cgroup_hint_docker() {
  local cname="$1" out_dir="$2"
  local cid cg
  cid=$(docker inspect -f '{{.Id}}' "$cname" 2>/dev/null || true)
  if [ -n "$cid" ]; then
    cg=$(find /sys/fs/cgroup -maxdepth 4 -type d -name "*${cid}*" 2>/dev/null | head -1 || true)
    echo "$cg" > "$out_dir/cgroup_path"
  else
    echo "" > "$out_dir/cgroup_path"
  fi
}

record_cold_start() {
  local out_dir="$1" mode="$2" engine="$3" model="$4" ready_s="$5" ok="$6"
  python3 - "$out_dir/cold_starts.jsonl" "$mode" "$engine" "$model" "$ready_s" "$ok" <<'PY'
import json, sys, time
row = {
    "ts": time.strftime("%Y-%m-%dT%H:%M:%S"),
    "mode": sys.argv[2],
    "engine": sys.argv[3],
    "model": sys.argv[4],
    "server_ready_s": round(float(sys.argv[5]), 3),
    "ok": sys.argv[6] == "1",
}
with open(sys.argv[1], "a") as f:
    f.write(json.dumps(row) + "\n")
PY
}

# start_engine <mode> <engine> <out_dir> <mem_max_mb> <threads> <ctx> <kv> <slots> <chat_model_key> <embed_model_key_or_none> <ollama_image>
start_engine() {
  local mode="$1" engine="$2" out_dir="$3" mem_max_mb="$4" threads="$5" ctx="$6" kv="$7" slots="$8" chat_key="$9" embed_key="${10:-none}" ollama_image="${11:-ollama/ollama:0.40.16}"
  stop_all_engines "$out_dir"
  echo "$engine" > "$out_dir/target"
  local t0 t1 ready_s logf
  logf="$out_dir/engine_${mode}_${engine}.log"
  t0=$(python3 -c 'import time; print(time.monotonic())')

  if [ "$engine" = "cluaiz" ]; then
    if [ "$mode" = "native" ]; then
      rm -f "$CLUAIZ_HOME_NATIVE/session.token"
      systemd-run --scope --quiet --unit=vps26-cluaiz -p "MemoryMax=${mem_max_mb}M" -p MemorySwapMax=0 \
        env CLUAIZ_HOME="$CLUAIZ_HOME_NATIVE" CLUAIZ_HOST=127.0.0.1 CLUAIZ_PORT="$PORT_CLUAIZ" CLUAIZ_NONINTERACTIVE=1 \
        "$CLUAIZ_BIN" serve >> "$logf" 2>&1 &
      if wait_http "http://127.0.0.1:$PORT_CLUAIZ/health" 120; then
        t1=$(python3 -c 'import time; print(time.monotonic())')
        ready_s=$(python3 -c "print($t1 - $t0)")
        set_cgroup_hint_native "vps26-cluaiz.scope" "$out_dir"
        cp "$CLUAIZ_HOME_NATIVE/session.token" /tmp/vps26_cluaiz.token
        record_cold_start "$out_dir" "$mode" "$engine" "$chat_key" "$ready_s" 1
        return 0
      fi
    else
      rm -f "$CLUAIZ_HOME_DOCKER/session.token"
      docker run -d --rm --name vps26-cluaiz --network host \
        --memory="${mem_max_mb}m" --memory-swap="${mem_max_mb}m" --cpus="$threads" \
        -e CLUAIZ_HOME=/cluaiz_home -e CLUAIZ_HOST=127.0.0.1 -e CLUAIZ_PORT="$PORT_CLUAIZ" \
        -e CLUAIZ_NONINTERACTIVE=1 -e LD_LIBRARY_PATH=/cluaiz_home/bin \
        -v "$CLUAIZ_HOME_DOCKER:/cluaiz_home" \
        -v "/root/.cluaiz/bin:/cluaiz_home/bin:ro" \
        -v "/root/.cluaiz/models:/cluaiz_home/models:ro" \
        vps26-cluaiz:0.1.0 serve >/dev/null
      docker logs -f vps26-cluaiz >> "$logf" 2>&1 &
      if wait_http "http://127.0.0.1:$PORT_CLUAIZ/health" 120; then
        t1=$(python3 -c 'import time; print(time.monotonic())')
        ready_s=$(python3 -c "print($t1 - $t0)")
        set_cgroup_hint_docker "vps26-cluaiz" "$out_dir"
        cp "$CLUAIZ_HOME_DOCKER/session.token" /tmp/vps26_cluaiz.token
        record_cold_start "$out_dir" "$mode" "$engine" "$chat_key" "$ready_s" 1
        return 0
      fi
    fi

  elif [ "$engine" = "ollama" ]; then
    local max_loaded="${OLLAMA_MAX_LOADED:-1}"
    if [ "$mode" = "native" ]; then
      systemd-run --scope --quiet --unit=vps26-ollama -p "MemoryMax=${mem_max_mb}M" -p MemorySwapMax=0 \
        env OLLAMA_HOST="127.0.0.1:$PORT_OLLAMA" OLLAMA_MODELS="$OLLAMA_MODELS_DIR" \
        OLLAMA_FLASH_ATTENTION=1 OLLAMA_KV_CACHE_TYPE="$kv" OLLAMA_NUM_PARALLEL="$slots" \
        OLLAMA_MAX_LOADED_MODELS="$max_loaded" OLLAMA_KEEP_ALIVE=60m \
        "$OLLAMA_BIN" serve >> "$logf" 2>&1 &
      if wait_http "http://127.0.0.1:$PORT_OLLAMA/api/version" 120; then
        t1=$(python3 -c 'import time; print(time.monotonic())')
        ready_s=$(python3 -c "print($t1 - $t0)")
        set_cgroup_hint_native "vps26-ollama.scope" "$out_dir"
        record_cold_start "$out_dir" "$mode" "$engine" "$chat_key" "$ready_s" 1
        return 0
      fi
    else
      docker run -d --rm --name vps26-ollama --network host \
        --memory="${mem_max_mb}m" --memory-swap="${mem_max_mb}m" --cpus="$threads" \
        -e OLLAMA_HOST="127.0.0.1:$PORT_OLLAMA" -e OLLAMA_MODELS=/root/.ollama/models \
        -e OLLAMA_FLASH_ATTENTION=1 -e OLLAMA_KV_CACHE_TYPE="$kv" -e OLLAMA_NUM_PARALLEL="$slots" \
        -e OLLAMA_MAX_LOADED_MODELS="$max_loaded" -e OLLAMA_KEEP_ALIVE=60m \
        -v "$OLLAMA_MODELS_DIR:/root/.ollama/models" \
        "$ollama_image" serve >/dev/null
      docker logs -f vps26-ollama >> "$logf" 2>&1 &
      if wait_http "http://127.0.0.1:$PORT_OLLAMA/api/version" 120; then
        t1=$(python3 -c 'import time; print(time.monotonic())')
        ready_s=$(python3 -c "print($t1 - $t0)")
        set_cgroup_hint_docker "vps26-ollama" "$out_dir"
        record_cold_start "$out_dir" "$mode" "$engine" "$chat_key" "$ready_s" 1
        return 0
      fi
    fi

  elif [ "$engine" = "llama" ]; then
    local total_ctx=$((ctx * slots))
    local chat_gguf embed_gguf
    if [ "$mode" = "native" ]; then
      if [ "$chat_key" != "none" ] && [ "$embed_key" != "none" ]; then
        chat_gguf=$(model_gguf_host "$chat_key")
        embed_gguf=$(model_gguf_host "$embed_key")
        systemd-run --scope --quiet --unit=vps26-llama -p "MemoryMax=${mem_max_mb}M" -p MemorySwapMax=0 \
          env LD_LIBRARY_PATH="$LLAMA_DIR" bash -c \
          "'$LLAMA_BIN' -m '$chat_gguf' --host 127.0.0.1 --port $PORT_LLAMA_CHAT -ngl 0 -t $threads -tb $threads -np $slots -c $total_ctx -ctk $kv -ctv $kv -fa auto --reasoning off --reasoning-budget 0 >> '$logf' 2>&1 & '$LLAMA_BIN' -m '$embed_gguf' --embedding --host 127.0.0.1 --port $PORT_LLAMA_EMBED -ngl 0 -t $threads -tb $threads -np $slots -c $total_ctx -b 2048 -ub 2048 >> '${logf}.embed' 2>&1 & wait" &
        if wait_http "http://127.0.0.1:$PORT_LLAMA_CHAT/health" 180 && wait_http "http://127.0.0.1:$PORT_LLAMA_EMBED/health" 180; then
          t1=$(python3 -c 'import time; print(time.monotonic())')
          ready_s=$(python3 -c "print($t1 - $t0)")
          set_cgroup_hint_native "vps26-llama.scope" "$out_dir"
          record_cold_start "$out_dir" "$mode" "$engine" "${chat_key}+${embed_key}" "$ready_s" 1
          return 0
        fi
      elif [ "$embed_key" != "none" ]; then
        embed_gguf=$(model_gguf_host "$embed_key")
        systemd-run --scope --quiet --unit=vps26-llama -p "MemoryMax=${mem_max_mb}M" -p MemorySwapMax=0 \
          env LD_LIBRARY_PATH="$LLAMA_DIR" \
          "$LLAMA_BIN" -m "$embed_gguf" --embedding --host 127.0.0.1 --port "$PORT_LLAMA_EMBED" \
          -ngl 0 -t "$threads" -tb "$threads" -np "$slots" -c "$total_ctx" -b 2048 -ub 2048 >> "${logf}.embed" 2>&1 &
        if wait_http "http://127.0.0.1:$PORT_LLAMA_EMBED/health" 180; then
          t1=$(python3 -c 'import time; print(time.monotonic())')
          ready_s=$(python3 -c "print($t1 - $t0)")
          set_cgroup_hint_native "vps26-llama.scope" "$out_dir"
          record_cold_start "$out_dir" "$mode" "$engine" "$embed_key" "$ready_s" 1
          return 0
        fi
      else
        chat_gguf=$(model_gguf_host "$chat_key")
        systemd-run --scope --quiet --unit=vps26-llama -p "MemoryMax=${mem_max_mb}M" -p MemorySwapMax=0 \
          env LD_LIBRARY_PATH="$LLAMA_DIR" \
          "$LLAMA_BIN" -m "$chat_gguf" --host 127.0.0.1 --port "$PORT_LLAMA_CHAT" \
          -ngl 0 -t "$threads" -tb "$threads" -np "$slots" -c "$total_ctx" -ctk "$kv" -ctv "$kv" -fa auto \
          --reasoning off --reasoning-budget 0 >> "$logf" 2>&1 &
        if wait_http "http://127.0.0.1:$PORT_LLAMA_CHAT/health" 180; then
          t1=$(python3 -c 'import time; print(time.monotonic())')
          ready_s=$(python3 -c "print($t1 - $t0)")
          set_cgroup_hint_native "vps26-llama.scope" "$out_dir"
          record_cold_start "$out_dir" "$mode" "$engine" "$chat_key" "$ready_s" 1
          return 0
        fi
      fi
    else
      if [ "$chat_key" != "none" ] && [ "$embed_key" != "none" ]; then
        chat_gguf=$(model_gguf_docker "$chat_key")
        embed_gguf=$(model_gguf_docker "$embed_key")
        docker run -d --rm --name vps26-llama --network host --entrypoint bash \
          --memory="${mem_max_mb}m" --memory-swap="${mem_max_mb}m" --cpus="$threads" \
          -v "$LLAMA_DIR:/opt/llama:ro" -v "/root/.cluaiz/models:/models:ro" \
          vps26-llama:b12428 -c \
          "/opt/llama/llama-server -m '$chat_gguf' --host 127.0.0.1 --port $PORT_LLAMA_CHAT -ngl 0 -t $threads -tb $threads -np $slots -c $total_ctx -ctk $kv -ctv $kv -fa auto --reasoning off --reasoning-budget 0 & /opt/llama/llama-server -m '$embed_gguf' --embedding --host 127.0.0.1 --port $PORT_LLAMA_EMBED -ngl 0 -t $threads -tb $threads -np $slots -c $total_ctx -b 2048 -ub 2048 & wait" >/dev/null
        docker logs -f vps26-llama >> "$logf" 2>&1 &
        if wait_http "http://127.0.0.1:$PORT_LLAMA_CHAT/health" 180 && wait_http "http://127.0.0.1:$PORT_LLAMA_EMBED/health" 180; then
          t1=$(python3 -c 'import time; print(time.monotonic())')
          ready_s=$(python3 -c "print($t1 - $t0)")
          set_cgroup_hint_docker "vps26-llama" "$out_dir"
          record_cold_start "$out_dir" "$mode" "$engine" "${chat_key}+${embed_key}" "$ready_s" 1
          return 0
        fi
      elif [ "$embed_key" != "none" ]; then
        embed_gguf=$(model_gguf_docker "$embed_key")
        docker run -d --rm --name vps26-llama --network host \
          --memory="${mem_max_mb}m" --memory-swap="${mem_max_mb}m" --cpus="$threads" \
          -v "$LLAMA_DIR:/opt/llama:ro" -v "/root/.cluaiz/models:/models:ro" \
          vps26-llama:b12428 -m "$embed_gguf" --embedding --host 127.0.0.1 --port "$PORT_LLAMA_EMBED" \
          -ngl 0 -t "$threads" -tb "$threads" -np "$slots" -c "$total_ctx" -b 2048 -ub 2048 >/dev/null
        docker logs -f vps26-llama >> "${logf}.embed" 2>&1 &
        if wait_http "http://127.0.0.1:$PORT_LLAMA_EMBED/health" 180; then
          t1=$(python3 -c 'import time; print(time.monotonic())')
          ready_s=$(python3 -c "print($t1 - $t0)")
          set_cgroup_hint_docker "vps26-llama" "$out_dir"
          record_cold_start "$out_dir" "$mode" "$engine" "$embed_key" "$ready_s" 1
          return 0
        fi
      else
        chat_gguf=$(model_gguf_docker "$chat_key")
        docker run -d --rm --name vps26-llama --network host \
          --memory="${mem_max_mb}m" --memory-swap="${mem_max_mb}m" --cpus="$threads" \
          -v "$LLAMA_DIR:/opt/llama:ro" -v "/root/.cluaiz/models:/models:ro" \
          vps26-llama:b12428 -m "$chat_gguf" --host 127.0.0.1 --port "$PORT_LLAMA_CHAT" \
          -ngl 0 -t "$threads" -tb "$threads" -np "$slots" -c "$total_ctx" -ctk "$kv" -ctv "$kv" -fa auto \
          --reasoning off --reasoning-budget 0 >/dev/null
        docker logs -f vps26-llama >> "$logf" 2>&1 &
        if wait_http "http://127.0.0.1:$PORT_LLAMA_CHAT/health" 180; then
          t1=$(python3 -c 'import time; print(time.monotonic())')
          ready_s=$(python3 -c "print($t1 - $t0)")
          set_cgroup_hint_docker "vps26-llama" "$out_dir"
          record_cold_start "$out_dir" "$mode" "$engine" "$chat_key" "$ready_s" 1
          return 0
        fi
      fi
    fi
  fi

  t1=$(python3 -c 'import time; print(time.monotonic())')
  ready_s=$(python3 -c "print($t1 - $t0)")
  record_cold_start "$out_dir" "$mode" "$engine" "$chat_key" "$ready_s" 0
  return 1
}
