#!/bin/sh
# max_steps 2, client tools and streaming checks, before and after binaries
W='C:/Users/<user>/AppData/Local/Temp/claude/6c30a0ef6fee852f/8b8b6806-071f-4fe0-86dc-c1c89edfc47b/scratchpad/optb'
M=qwen3.5-0.8b-gguf-Q4_K_M
cd "$W" || exit 1
for v in before after; do
  python -B loop_probe.py --exe "$W/$v/cluaiz.exe" --home "$W/home" --port 8014 --out "$W/raw/${v}_maxsteps2" --model $M --case marker --n 5 --max-steps 2 --label ${v}_maxsteps2
done > raw_maxsteps2.out 2>&1
for v in before after; do
  python -B loop_probe.py --exe "$W/$v/cluaiz.exe" --home "$W/home" --port 8014 --out "$W/raw/${v}_client_tools" --model $M --case marker --n 3 --client-tools --label ${v}_client_tools
done > raw_client_tools.out 2>&1
for v in before after; do
  python -B loop_probe.py --exe "$W/$v/cluaiz.exe" --home "$W/home" --port 8014 --out "$W/raw/${v}_stream" --model $M --case marker --n 3 --stream --label ${v}_stream
done > raw_stream.out 2>&1
echo done
