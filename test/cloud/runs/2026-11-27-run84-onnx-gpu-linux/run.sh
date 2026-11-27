#!/usr/bin/env bash
# Run 84: ONNX on GPU (CUDA EP) on Linux + run 4b repro. Isolated home ~/run84/home, port 8015.
set -uo pipefail
cd ~/run84; mkdir -p raw; EXE=~/cluaiz/target/debug/cluaiz
export CLUAIZ_HOME=~/run84/home CLUAIZ_PORT=8015 CLUAIZ_HOST=127.0.0.1 CLUAIZ_NONINTERACTIVE=1 NO_COLOR=1
mkdir -p $CLUAIZ_HOME
ls ~/cluaiz/target/debug/ | grep -i "onnx\|cud" > raw/exe_dir_before.txt
{ time timeout 1200 $EXE calibrate; } > raw/calibrate.txt 2>&1; echo "exit=$?" >> raw/calibrate.txt
find $CLUAIZ_HOME -iname "*cud*" -o -iname "*onnxruntime*" | sed "s#$HOME#~#" > raw/cuda_files.txt
ls ~/cluaiz/target/debug/ | grep -i "onnx\|cud" > raw/exe_dir_after.txt
$EXE pull onnx-community/Kokoro-82M-v1.0-ONNX:uint8 2>&1 | tr '\r' '\n' | tail -6 > raw/pull_kokoro.txt
$EXE pull onnx-community/moonshine-tiny-ONNX:int8 2>&1 | tr '\r' '\n' | tail -6 > raw/pull_moonshine.txt
$EXE list > raw/list.txt 2>&1
KID=$(grep -ao "Registered ID: *[^ ]*" raw/pull_kokoro.txt | awk '{print $NF}')
MID=$(grep -ao "Registered ID: *[^ ]*" raw/pull_moonshine.txt | awk '{print $NF}')
echo "kokoro=$KID moonshine=$MID" > raw/ids.txt
$EXE -vv serve > raw/serve.log 2>&1 & S=$!
for i in $(seq 1 120); do curl -s -m 2 http://127.0.0.1:8015/health >/dev/null && break; sleep 1; done
T=$(cat $CLUAIZ_HOME/session.token 2>/dev/null)
nvidia-smi --query-gpu=memory.used --format=csv > raw/gpu_before.txt
for n in 1 2 3; do
  curl -s -o raw/tts$n.wav -w "tts$n http=%{http_code} bytes=%{size_download} s=%{time_total}\n" -m 300 \
    -H "Authorization: Bearer $T" -H "Content-Type: application/json" \
    -d "{\"model\":\"$KID\",\"input\":\"Hello, this is a short speech test from the Cluaiz engine on a Linux GPU.\",\"voice\":\"default\"}" \
    http://127.0.0.1:8015/v1/audio/speech >> raw/requests.txt
  kill -0 $S 2>/dev/null && echo "server alive after tts$n" >> raw/requests.txt || echo "SERVER DEAD after tts$n" >> raw/requests.txt
done
nvidia-smi --query-compute-apps=pid,used_memory --format=csv > raw/gpu_apps_after_tts.txt
for n in 1 2; do
  curl -s -o raw/stt$n.json -w "stt$n http=%{http_code} s=%{time_total}\n" -m 300 -H "Authorization: Bearer $T" \
    -F "model=$MID" -F "file=@raw/tts1.wav" http://127.0.0.1:8015/v1/audio/transcriptions >> raw/requests.txt
  kill -0 $S 2>/dev/null && echo "server alive after stt$n" >> raw/requests.txt || echo "SERVER DEAD after stt$n" >> raw/requests.txt
done
curl -s -m 5 http://127.0.0.1:8015/health >> raw/requests.txt; echo >> raw/requests.txt
kill $S; wait $S 2>/dev/null
grep -aiE "cuda|cudnn|execution provider|ExecutionProvider|CPU fallback|onnx|cublas|panic|abort" raw/serve.log | grep -v "Authorization" > raw/serve_onnx_lines.txt
cat $CLUAIZ_HOME/last_crash.txt > raw/last_crash.txt 2>/dev/null
echo done > raw/steps.txt
