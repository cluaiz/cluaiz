#!/usr/bin/env bash
# Run 84 phase 2: calibrate retries with backtrace; then serve-side provisioning, restart, TTS on GPU.
set -uo pipefail
cd ~/run84; EXE=~/cluaiz/target/debug/cluaiz
export CLUAIZ_HOME=~/run84/home CLUAIZ_PORT=8015 CLUAIZ_HOST=127.0.0.1 CLUAIZ_NONINTERACTIVE=1 NO_COLOR=1
for n in 2 3; do RUST_BACKTRACE=1 timeout 1500 $EXE calibrate > raw/calibrate_try$n.txt 2>&1; echo "exit=$?" >> raw/calibrate_try$n.txt; grep -q "exit=0" raw/calibrate_try$n.txt && break; done
ls -la $CLUAIZ_HOME/bin | sed "s#$HOME#~#" > raw/bin_after_calibrate.txt
KID=kokoro-82m-v1.0-onnx-UINT8; MID=moonshine-tiny-onnx-INT8
for phase in a b; do
  $EXE -vv serve > raw/serve2$phase.log 2>&1 & S=$!
  for i in $(seq 1 120); do curl -s -m 2 http://127.0.0.1:8015/health >/dev/null && break; sleep 1; done
  T=$(cat $CLUAIZ_HOME/session.token)
  for n in 1 2 3; do
    curl -s -o raw/p2${phase}_tts$n.wav -w "$phase tts$n http=%{http_code} bytes=%{size_download} s=%{time_total}\n" -m 300 \
      -H "Authorization: Bearer $T" -H "Content-Type: application/json" \
      -d "{\"model\":\"$KID\",\"input\":\"Hello, this is a short speech test from the Cluaiz engine on a Linux GPU.\",\"voice\":\"default\"}" \
      http://127.0.0.1:8015/v1/audio/speech >> raw/requests2.txt
    kill -0 $S 2>/dev/null && echo "server alive after $phase tts$n" >> raw/requests2.txt || echo "SERVER DEAD after $phase tts$n" >> raw/requests2.txt
  done
  nvidia-smi --query-compute-apps=pid,used_memory --format=csv >> raw/requests2.txt
  for n in 1 2; do
    curl -s -o raw/p2${phase}_stt$n.json -w "$phase stt$n http=%{http_code} s=%{time_total}\n" -m 300 -H "Authorization: Bearer $T" \
      -H "Content-Type: application/json" -d "{\"model\":\"$MID\",\"file\":\"$HOME/run84/raw/tts1.wav\"}" \
      http://127.0.0.1:8015/v1/audio/transcriptions >> raw/requests2.txt
    kill -0 $S 2>/dev/null && echo "server alive after $phase stt$n" >> raw/requests2.txt || echo "SERVER DEAD after $phase stt$n" >> raw/requests2.txt
  done
  if [ $phase = a ]; then
    for i in $(seq 1 120); do [ -f $CLUAIZ_HOME/bin/cuda_runtime.ready ] && break; sleep 10; done
    echo "ready file after wait: $(ls $CLUAIZ_HOME/bin/cuda_runtime.ready 2>&1)" >> raw/requests2.txt
  fi
  kill $S; wait $S 2>/dev/null
done
ls -la $CLUAIZ_HOME/bin | sed "s#$HOME#~#" > raw/bin_final.txt
cat $CLUAIZ_HOME/last_crash.txt > raw/last_crash.txt 2>/dev/null
echo done > raw/steps2.txt
