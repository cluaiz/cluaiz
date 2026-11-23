"""Run 50: the same image prompts on official llama-server (b12428), so the Engine's answers have a baseline.

    python official_server.py --server <llama-server.exe> --model <gguf> --mmproj <mmproj> --port 8013 --out raw/official_server.txt

Sends the Engine's own system prompt and tool hint (raw/engine_system_prompt.txt, raw/engine_tools_hint.txt,
taken from a return_prompt reply of the Engine) as OpenAI content parts, text-first and image-first, plus the
plain question without them. llama-server joins text parts with a newline and none next to an image.
"""
import argparse
import base64
import http.client
import json
import os
import subprocess
import time

from vision_vram import png_square


def post(port, body):
    conn = http.client.HTTPConnection("127.0.0.1", port, timeout=600)
    conn.request("POST", "/v1/chat/completions", json.dumps(body), {"Content-Type": "application/json"})
    r = conn.getresponse()
    data = json.loads(r.read())
    conn.close()
    if r.status != 200:
        return r.status, json.dumps(data)[:200], {}
    return r.status, data["choices"][0]["message"].get("content") or "", data.get("usage", {})


def main():
    p = argparse.ArgumentParser()
    p.add_argument("--server", required=True)
    p.add_argument("--model", required=True)
    p.add_argument("--mmproj", required=True)
    p.add_argument("--port", type=int, default=8013)
    p.add_argument("--out", required=True)
    p.add_argument("--extra-args", default="")
    args = p.parse_args()
    if args.port in (8000, 8011):
        raise SystemExit("never ports 8000 or 8011")
    here = os.path.dirname(os.path.abspath(__file__))
    system = open(os.path.join(here, "raw", "engine_system_prompt.txt"), encoding="utf-8").read()
    hint = open(os.path.join(here, "raw", "engine_tools_hint.txt"), encoding="utf-8").read()
    image = {"type": "image_url", "image_url": {"url": "data:image/png;base64," + base64.b64encode(png_square()).decode()}}
    question = {"type": "text", "text": "What colour is this image? One word."}
    hint_part = {"type": "text", "text": hint}
    cases = [
        ("engine_prompt text_first", [{"role": "system", "content": system}, {"role": "user", "content": [hint_part, question, image]}]),
        ("engine_prompt image_first", [{"role": "system", "content": system}, {"role": "user", "content": [hint_part, image, question]}]),
        ("plain text_first", [{"role": "user", "content": [question, image]}]),
        ("plain image_first", [{"role": "user", "content": [image, question]}]),
    ]
    log = open(args.out + ".server.log", "w", encoding="utf-8")
    cmd = [args.server, "-m", args.model, "--mmproj", args.mmproj, "--port", str(args.port), "--host", "127.0.0.1",
           "-c", "16384", "--jinja"] + args.extra_args.split()
    proc = subprocess.Popen(cmd, stdout=log, stderr=subprocess.STDOUT)
    try:
        for _ in range(180):
            try:
                c = http.client.HTTPConnection("127.0.0.1", args.port, timeout=2)
                c.request("GET", "/health")
                if c.getresponse().status == 200:
                    break
            except OSError:
                pass
            time.sleep(1)
        with open(args.out, "w", encoding="utf-8") as out:
            out.write("command: " + " ".join(os.path.basename(x) if os.path.isabs(x) else x for x in cmd) + "\n")
            for name, messages in cases:
                for i in range(2):
                    status, content, usage = post(args.port, {"messages": messages, "temperature": 0, "max_tokens": 64,
                                                              "chat_template_kwargs": {"enable_thinking": False}})
                    line = f"{name} {i} {status} {content[:120]!r} prompt_tokens={usage.get('prompt_tokens')}"
                    print(line)
                    out.write(line + "\n")
    finally:
        proc.terminate()
        proc.wait(30)


if __name__ == "__main__":
    main()
