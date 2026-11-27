# Run 86: ONNX on CUDA after the calibrate fix. Runs on cluaiz-l4b after `cluaiz calibrate`
# filled ~/run86/home/bin. Isolated CLUAIZ_HOME, port 8016.
set -uo pipefail
cd ~/run86; mkdir -p raw; EXE=~/cluaiz/target/debug/cluaiz
export CLUAIZ_HOME=~/run86/home CLUAIZ_PORT=8016 CLUAIZ_HOST=127.0.0.1 CLUAIZ_NONINTERACTIVE=1 NO_COLOR=1
$EXE pull onnx-community/Kokoro-82M-v1.0-ONNX:uint8 2>&1 | tr '\r' '\n' | tail -4 > raw/pull_kokoro.txt
$EXE pull onnx-community/moonshine-tiny-ONNX:int8 2>&1 | tr '\r' '\n' | tail -4 > raw/pull_moonshine.txt
KID=kokoro-82m-v1.0-onnx-UINT8; MID=moonshine-tiny-onnx-INT8
$EXE -vv serve > raw/serve.log 2>&1 & S=$!
for i in $(seq 1 120); do curl -s -m 2 http://127.0.0.1:8016/health >/dev/null && break; sleep 1; done
T=$(cat $CLUAIZ_HOME/session.token)
for n in 1 2 3; do
  curl -s -o raw/tts$n.wav -w "tts$n http=%{http_code} bytes=%{size_download} s=%{time_total}\n" -m 300 \
    -H "Authorization: Bearer $T" -H "Content-Type: application/json" \
    -d "{\"model\":\"$KID\",\"input\":\"Hello, this is a short speech test from the Cluaiz engine on a Linux GPU.\",\"voice\":\"default\"}" \
    http://127.0.0.1:8016/v1/audio/speech >> raw/requests.txt
done
nvidia-smi --query-compute-apps=pid,used_memory --format=csv >> raw/requests.txt
for n in 1 2; do
  curl -s -o raw/stt$n.json -w "stt$n http=%{http_code} s=%{time_total}\n" -m 300 -H "Authorization: Bearer $T" \
    -H "Content-Type: application/json" -d "{\"model\":\"$MID\",\"file\":\"$HOME/run86/raw/tts1.wav\"}" \
    http://127.0.0.1:8016/v1/audio/transcriptions >> raw/requests.txt
done
kill -0 $S && echo "server alive at end" >> raw/requests.txt
kill $S; wait $S 2>/dev/null
grep -aiE "ExecutionProvider|cuda|cudnn|panic" raw/serve.log | grep -v Authorization | cut -c1-300 > raw/serve_onnx_lines.txt
echo done > raw/steps.txt
