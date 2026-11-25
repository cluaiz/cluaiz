"""Run 76: image/text part order on image requests with installed Engine tools (Windows laptop).

    python vision_order.py --exe <cluaiz.exe> --home <CLUAIZ_HOME> --model <id> --rounds 10 --out <dir> --tag <before|after>

Starts one `cluaiz -vv serve` on port 8013 (never 8000), isolated CLUAIZ_HOME (installed tools: math plugin and
filesystem MCP, as in runs 70/72). Each round sends the three layouts in a rotated order, then one weather-tool
request in a rotated layout. Before the rounds, each layout is sent once with return_prompt: true so the rendered
prompt (part order, tool hint place) is kept. Every request and reply goes to <out>/<tag>.jsonl, a short line to
<out>/<tag>.txt, the server log to <out>/<tag>_serve.log. The bearer token is read from <home>/session.token and
never printed.

Layouts (the client's user message):
  L1_text_first  [text, image]
  L2_image_first [image, text]
  L3_text_image_text [text, image, text]
Colour pass: no tool call and the reply contains "red". Weather pass: the reply has a get_weather tool call.
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


def text(t):
    return {"type": "text", "text": t}


LAYOUTS = {
    "L1_text_first": [text("What colour is this image? One word."), IMAGE],
    "L2_image_first": [IMAGE, text("What colour is this image? One word.")],
    "L3_text_image_text": [text("Look at this picture."), IMAGE, text("What colour is it? One word.")],
}
WEATHER_TEXT = "What is the weather in Paris right now? Use the get_weather tool."
WEATHER_LAYOUTS = {
    "W1_text_first": [text(WEATHER_TEXT), IMAGE],
    "W2_image_first": [IMAGE, text(WEATHER_TEXT)],
    "W3_text_image_text": [text("Here is a photo from my trip."), IMAGE, text(WEATHER_TEXT)],
}
WEATHER = {"type": "function", "function": {"name": "get_weather", "description": "Current weather for a city.",
                                            "parameters": {"type": "object", "properties": {"city": {"type": "string"}}, "required": ["city"]}}}


def post(port, token, body, timeout=600):
    conn = http.client.HTTPConnection("127.0.0.1", port, timeout=timeout)
    headers = {"Content-Type": "application/json"}
    if token:
        headers["Authorization"] = "Bearer " + token
    conn.request("POST", "/v1/chat/completions", json.dumps(body), headers)
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
    p.add_argument("--port", type=int, default=8013)
    p.add_argument("--rounds", type=int, default=10)
    p.add_argument("--out", required=True)
    p.add_argument("--tag", required=True)
    args = p.parse_args()
    if args.port == 8000 or ".cluaiz" in os.path.abspath(args.home).replace("\\", "/").split("/"):
        raise SystemExit("never port 8000 or ~/.cluaiz")
    os.makedirs(args.out, exist_ok=True)
    token_path = os.path.join(args.home, "session.token")
    token = open(token_path).read().strip() if os.path.exists(token_path) else ""
    jl = open(os.path.join(args.out, f"{args.tag}.jsonl"), "w", encoding="utf-8")
    txt = open(os.path.join(args.out, f"{args.tag}.txt"), "w", encoding="utf-8")
    score = {}

    def note(line):
        print(line, flush=True)
        txt.write(line + "\n")
        txt.flush()

    env = dict(os.environ, CLUAIZ_HOME=args.home, CLUAIZ_PORT=str(args.port), CLUAIZ_HOST="127.0.0.1",
               CLUAIZ_NONINTERACTIVE="1", NO_COLOR="1")
    log = open(os.path.join(args.out, f"{args.tag}_serve.log"), "w", encoding="utf-8")
    proc = subprocess.Popen([args.exe, "-vv", "serve"], env=env, stdout=log, stderr=subprocess.STDOUT,
                            cwd=os.path.dirname(args.exe))
    names = list(LAYOUTS)
    wnames = list(WEATHER_LAYOUTS)
    try:
        wait_health(args.port, proc)
        steps = [(n, True) for n in names]
        for r in range(args.rounds):
            off = r % len(names)
            steps += [(n, False) for n in names[off:] + names[:off]]
            steps.append((wnames[r % len(wnames)], False))
        for i, (name, probe) in enumerate(steps):
            weather = name in WEATHER_LAYOUTS
            parts = WEATHER_LAYOUTS[name] if weather else LAYOUTS[name]
            body = {"model": args.model, "messages": [{"role": "user", "content": parts}],
                    "temperature": 0, "max_tokens": 64, "think_mode": "off"}
            if weather:
                body["tools"] = [WEATHER]
            if probe:
                body["return_prompt"] = True
            t0 = time.time()
            status, reply = post(args.port, token, body)
            secs = time.time() - t0
            msg = ((reply.get("choices") or [{}])[0]).get("message") or {}
            content = msg.get("content") or ""
            calls = [c.get("function", {}).get("name") for c in (msg.get("tool_calls") or [])]
            if weather:
                ok = "get_weather" in calls
            else:
                ok = not calls and "red" in content.lower()
            if not probe:
                s = score.setdefault(name, [0, 0])
                s[0] += int(ok)
                s[1] += 1
            jl.write(json.dumps({"step": i, "layout": name, "probe": probe, "status": status, "pass": ok,
                                 "seconds": round(secs, 2), "request": strip_image(body), "reply": reply}) + "\n")
            jl.flush()
            note(f"{i} {'probe' if probe else 'run'} {name} {status} pass={ok} content={content[:100]!r} "
                 f"tool_calls={calls} prompt_tokens={(reply.get('usage') or {}).get('prompt_tokens')} {secs:.1f}s")
    finally:
        proc.terminate()
        try:
            proc.wait(30)
        except subprocess.TimeoutExpired:
            proc.kill()
        log.close()
    for name, (good, total) in score.items():
        note(f"SCORE {args.tag} {name} {good}/{total}")


if __name__ == "__main__":
    sys.exit(main())
