"""Run 64: image + tool hint layouts (run 50 follow-up) on the L4 VM, isolated home, port 8011.

    python3 vision_hint.py --exe <cluaiz> --home <CLUAIZ_HOME> --model <id> --rounds 5 --out <dir>

Each round starts a fresh `cluaiz serve` (port 8011, its log in <out>/<model>_r<N>.log), sends the three
layouts in a rotated order, then one tool check, then stops the server. Every request and reply goes to
<out>/<model>.jsonl (one JSON line per request) and a short line to <out>/<model>.txt.

Layouts (the client's message; the Engine itself puts its tool hint, when it has one, in front as part 0):
  L1 text_first_hint   [text, image]                        (run 50's failing order)
  L2 text_first_nohint [text, image], tool_choice "none"    (no tool hint, no tools)
  L3 image_first_hint  [image, text]
Tool check (layout 3 order): [image, "What is the weather in Paris right now? Use the get_weather tool."] with a
client function get_weather(city); pass = the reply has a get_weather tool call.
Round 1 also asks each layout once with return_prompt: true, so the rendered prompt (part order, hint text) is kept.
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


def png_square(size=448, rgb=(255, 0, 0)):
    row = b"\x00" + bytes(rgb) * size
    raw = row * size

    def chunk(kind, data):
        return struct.pack(">I", len(data)) + kind + data + struct.pack(">I", zlib.crc32(kind + data) & 0xFFFFFFFF)

    return b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack(">IIBBBBB", size, size, 8, 2, 0, 0, 0)) + chunk(b"IDAT", zlib.compress(raw)) + chunk(b"IEND", b"")


IMAGE = {"type": "image_url", "image_url": {"url": "data:image/png;base64," + base64.b64encode(png_square()).decode()}}
TEXT = {"type": "text", "text": "What colour is this image? One word."}
TOOL_TEXT = {"type": "text", "text": "What is the weather in Paris right now? Use the get_weather tool."}
WEATHER = {"type": "function", "function": {"name": "get_weather", "description": "Current weather for a city.",
                                            "parameters": {"type": "object", "properties": {"city": {"type": "string"}}, "required": ["city"]}}}
LAYOUTS = {
    "L1_text_first_hint": ([TEXT, IMAGE], {}),
    "L2_text_first_nohint": ([TEXT, IMAGE], {"tool_choice": "none"}),
    "L3_image_first_hint": ([IMAGE, TEXT], {}),
}


def gpu():
    out = subprocess.run(["nvidia-smi", "--query-gpu=memory.used,memory.free", "--format=csv,noheader,nounits"],
                         capture_output=True, text=True).stdout.strip()
    return out


def post(port, body, timeout=600):
    conn = http.client.HTTPConnection("127.0.0.1", port, timeout=timeout)
    conn.request("POST", "/v1/chat/completions", json.dumps(body), {"Content-Type": "application/json"})
    response = conn.getresponse()
    data = response.read().decode("utf-8", "replace")
    conn.close()
    try:
        return response.status, json.loads(data)
    except ValueError:
        return response.status, {"raw": data}


def wait_health(port, proc, seconds=300):
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


def strip_image(body):
    """The request as logged: the base64 image replaced by its length."""
    copy = json.loads(json.dumps(body))
    for m in copy.get("messages", []):
        if isinstance(m.get("content"), list):
            for p in m["content"]:
                if p.get("type") == "image_url":
                    p["image_url"]["url"] = f"<png base64, {len(IMAGE['image_url']['url'])} chars>"
    return copy


def main():
    p = argparse.ArgumentParser()
    p.add_argument("--exe", required=True)
    p.add_argument("--home", required=True)
    p.add_argument("--model", required=True)
    p.add_argument("--port", type=int, default=8011)
    p.add_argument("--rounds", type=int, default=5)
    p.add_argument("--out", required=True)
    args = p.parse_args()
    if args.port == 8000:
        raise SystemExit("never port 8000")
    os.makedirs(args.out, exist_ok=True)
    tag = args.model
    names = list(LAYOUTS)
    jl = open(os.path.join(args.out, f"{tag}.jsonl"), "a", encoding="utf-8")
    txt = open(os.path.join(args.out, f"{tag}.txt"), "a", encoding="utf-8")

    def note(line):
        print(line, flush=True)
        txt.write(line + "\n")
        txt.flush()

    env = dict(os.environ, CLUAIZ_HOME=args.home, CLUAIZ_PORT=str(args.port), CLUAIZ_HOST="127.0.0.1",
               CLUAIZ_NONINTERACTIVE="1", NO_COLOR="1")
    for r in range(1, args.rounds + 1):
        log = open(os.path.join(args.out, f"{tag}_r{r}.log"), "w", encoding="utf-8")
        proc = subprocess.Popen([args.exe, "-vv", "serve"], env=env, stdout=log, stderr=subprocess.STDOUT)
        try:
            wait_health(args.port, proc)
            time.sleep(2)
            note(f"== round {r}: server up, GPU used,free MiB {gpu()}")
            off = (r - 1) % len(names)
            order = names[off:] + names[:off]
            steps = [(n, False) for n in order] + [("TOOL_image_first", False)]
            if r == 1:
                steps = [(n, True) for n in names] + steps
            for name, probe in steps:
                if name == "TOOL_image_first":
                    body = {"messages": [{"role": "user", "content": [IMAGE, TOOL_TEXT]}], "tools": [WEATHER]}
                else:
                    parts, extra = LAYOUTS[name]
                    body = dict({"messages": [{"role": "user", "content": parts}]}, **extra)
                body.update({"model": args.model, "temperature": 0, "max_tokens": 64, "think_mode": "off"})
                if probe:
                    body["return_prompt"] = True
                t0 = time.time()
                status, reply = post(args.port, body)
                secs = time.time() - t0
                msg = ((reply.get("choices") or [{}])[0]).get("message") or {}
                content = msg.get("content") or ""
                calls = [c.get("function", {}).get("name") for c in (msg.get("tool_calls") or [])]
                usage = reply.get("usage") or {}
                jl.write(json.dumps({"round": r, "layout": name, "probe": probe, "status": status, "seconds": round(secs, 2),
                                     "request": strip_image(body), "reply": reply}) + "\n")
                jl.flush()
                kind = "probe" if probe else "run"
                note(f"{r} {kind} {name} {status} content={content[:100]!r} tool_calls={calls} "
                     f"prompt_tokens={usage.get('prompt_tokens')} {secs:.1f}s gpu={gpu()}")
        finally:
            proc.terminate()
            try:
                proc.wait(30)
            except subprocess.TimeoutExpired:
                proc.kill()
            log.close()
            time.sleep(3)


if __name__ == "__main__":
    sys.exit(main())
