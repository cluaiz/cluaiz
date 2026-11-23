"""Run 50: image requests on a fresh isolated Engine server, with GPU memory after each request.

    python vision_vram.py --exe <cluaiz.exe> --home <CLUAIZ_HOME> --port 8014 --label before --out raw/

Each scenario starts its own server (so "fresh server" is real), writes the server log to
raw/<label>_<scenario>.log and one line per request to raw/<label>_<scenario>.txt, then stops it.
Scenarios:
  five_images   5 image requests in a row: text-first, image-first, text-first, image-first, text-first
  image_first   image-first, then text-first twice
  text_then     a text chat, then 3 image requests (text-first, image-first, text-first)
--extra adds fields to each image request (run 50 used {"tool_choice": "none"} to leave out the tool hint).
Never point it at the owner's home or port 8000.
"""
import argparse
import base64
import http.client
import json
import os
import struct
import subprocess
import sys
import time
import zlib

CHAT = "qwen3.5-0.8b-gguf-Q4_K_M"


def png_square(size=448, rgb=(255, 0, 0)):
    row = b"\x00" + bytes(rgb) * size
    raw = row * size

    def chunk(kind, data):
        return struct.pack(">I", len(data)) + kind + data + struct.pack(">I", zlib.crc32(kind + data) & 0xFFFFFFFF)

    return b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack(">IIBBBBB", size, size, 8, 2, 0, 0, 0)) + chunk(b"IDAT", zlib.compress(raw)) + chunk(b"IEND", b"")


IMAGE = {"type": "image_url", "image_url": {"url": "data:image/png;base64," + base64.b64encode(png_square()).decode()}}
TEXT = {"type": "text", "text": "What colour is this image? One word."}


def gpu():
    out = subprocess.run(["nvidia-smi", "--query-gpu=memory.used,memory.free", "--format=csv,noheader,nounits"],
                         capture_output=True, text=True).stdout.strip()
    used, free = (int(x) for x in out.split(","))
    return f"GPU used/free {used} MiB, {free} MiB"


def post(port, body, timeout=600):
    conn = http.client.HTTPConnection("127.0.0.1", port, timeout=timeout)
    conn.request("POST", "/v1/chat/completions", json.dumps(dict({"model": CHAT, "temperature": 0}, **body)),
                 {"Content-Type": "application/json"})
    response = conn.getresponse()
    data = response.read().decode("utf-8", "replace")
    conn.close()
    try:
        reply = json.loads(data)
    except ValueError:
        return response.status, data, {}
    if response.status != 200:
        return response.status, (reply.get("error") or {}).get("message", data), {}
    return response.status, reply["choices"][0]["message"].get("content") or "", reply.get("usage", {})


def wait_health(port, proc, seconds=120):
    end = time.time() + seconds
    while time.time() < end:
        if proc.poll() is not None:
            raise SystemExit(f"server exited with {proc.returncode}")
        try:
            conn = http.client.HTTPConnection("127.0.0.1", port, timeout=2)
            conn.request("GET", "/health")
            if conn.getresponse().status == 200:
                return
        except OSError:
            pass
        time.sleep(1)
    raise SystemExit("server did not answer /health")


def run(args, scenario, steps):
    log_path = os.path.join(args.out, f"{args.label}_{scenario}.log")
    txt_path = os.path.join(args.out, f"{args.label}_{scenario}.txt")
    env = dict(os.environ, CLUAIZ_HOME=args.home, CLUAIZ_PORT=str(args.port), CLUAIZ_NONINTERACTIVE="1", NO_COLOR="1")
    with open(log_path, "w", encoding="utf-8") as log, open(txt_path, "w", encoding="utf-8") as txt:
        def note(line):
            print(line)
            txt.write(line + "\n")
            txt.flush()

        proc = subprocess.Popen([args.exe, "serve"], env=env, stdout=log, stderr=subprocess.STDOUT)
        try:
            wait_health(args.port, proc)
            time.sleep(2)
            note(f"server started, nothing loaded: {gpu()}")
            for i, step in enumerate(steps, 1):
                if step == "text":
                    status, content, usage = post(args.port, {"messages": [{"role": "user", "content": "Say ok."}], "max_tokens": 16, "think_mode": "off"})
                else:
                    parts = [TEXT, IMAGE] if step == "text_first" else [IMAGE, TEXT]
                    body = {"messages": [{"role": "user", "content": parts}], "max_tokens": 64, "think_mode": "off"}
                    status, content, usage = post(args.port, dict(body, **json.loads(args.extra)))
                note(f"{i} {step} {status} {content[:120]!r} prompt_tokens={usage.get('prompt_tokens')}")
                note(f"  after {i}: {gpu()}")
        finally:
            proc.terminate()
            try:
                proc.wait(30)
            except subprocess.TimeoutExpired:
                proc.kill()
            time.sleep(3)


def main():
    p = argparse.ArgumentParser()
    p.add_argument("--exe", required=True)
    p.add_argument("--home", required=True)
    p.add_argument("--port", type=int, default=8014)
    p.add_argument("--label", required=True)
    p.add_argument("--out", required=True)
    p.add_argument("--only", default="")
    p.add_argument("--extra", default="{}", help='JSON merged into each image request, e.g. {"tool_choice": "none"}')
    args = p.parse_args()
    if args.port in (8000, 8011):
        raise SystemExit("never ports 8000 or 8011")
    os.makedirs(args.out, exist_ok=True)
    scenarios = {
        "five_images": ["text_first", "image_first", "text_first", "image_first", "text_first"],
        "image_first": ["image_first", "text_first", "text_first"],
        "text_then": ["text", "text_first", "image_first", "text_first"],
    }
    for name, steps in scenarios.items():
        if args.only and name not in args.only.split(","):
            continue
        print(f"== {name}")
        run(args, name, steps)


if __name__ == "__main__":
    sys.exit(main())
