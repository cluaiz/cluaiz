"""Run 64 tool control: the same get_weather question with and without the image, so a failed tool check in
layout 3 can be told apart from a model that never calls the tool.

    python3 tool_control.py --exe <cluaiz> --home <CLUAIZ_HOME> --model <id> --rounds 5 --out <dir>

Each round: fresh `cluaiz serve` on port 8011, then (order rotated) T0 text only + tools, T3 [image, text] + tools.
Lines go to <out>/<model>.tools.txt / .tools.jsonl; the engine's timings.prompt_n (tokens llama.cpp really
processed, template tools included) is printed beside usage.prompt_tokens.
"""
import argparse
import json
import os
import subprocess
import sys
import time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from vision_hint import IMAGE, TOOL_TEXT, WEATHER, gpu, post, wait_health, strip_image  # noqa: E402

CASES = {"T0_text_only_tools": [TOOL_TEXT], "T3_image_first_tools": [IMAGE, TOOL_TEXT]}


def main():
    p = argparse.ArgumentParser()
    p.add_argument("--exe", required=True)
    p.add_argument("--home", required=True)
    p.add_argument("--model", required=True)
    p.add_argument("--port", type=int, default=8011)
    p.add_argument("--rounds", type=int, default=5)
    p.add_argument("--out", required=True)
    a = p.parse_args()
    if a.port == 8000:
        raise SystemExit("never port 8000")
    jl = open(os.path.join(a.out, f"{a.model}.tools.jsonl"), "a", encoding="utf-8")
    txt = open(os.path.join(a.out, f"{a.model}.tools.txt"), "a", encoding="utf-8")
    env = dict(os.environ, CLUAIZ_HOME=a.home, CLUAIZ_PORT=str(a.port), CLUAIZ_HOST="127.0.0.1", CLUAIZ_NONINTERACTIVE="1", NO_COLOR="1")
    names = list(CASES)
    for r in range(1, a.rounds + 1):
        log = open(os.path.join(a.out, f"{a.model}_tools_r{r}.log"), "w", encoding="utf-8")
        proc = subprocess.Popen([a.exe, "-vv", "serve"], env=env, stdout=log, stderr=subprocess.STDOUT)
        try:
            wait_health(a.port, proc)
            time.sleep(2)
            order = names[(r - 1) % 2:] + names[:(r - 1) % 2]
            for name in order:
                body = {"model": a.model, "messages": [{"role": "user", "content": CASES[name]}], "tools": [WEATHER],
                        "temperature": 0, "max_tokens": 128, "think_mode": "off"}
                status, reply = post(a.port, body)
                msg = ((reply.get("choices") or [{}])[0]).get("message") or {}
                calls = [(c.get("function", {}).get("name"), c.get("function", {}).get("arguments")) for c in (msg.get("tool_calls") or [])]
                pn = (((reply.get("engine") or {}).get("timings")) or {}).get("prompt_n")
                jl.write(json.dumps({"round": r, "case": name, "status": status, "request": strip_image(body), "reply": reply}) + "\n")
                jl.flush()
                line = (f"{r} {name} {status} tool_calls={calls} content={(msg.get('content') or '')[:90]!r} "
                        f"reasoning={(msg.get('reasoning_content') or '')[:60]!r} prompt_tokens={(reply.get('usage') or {}).get('prompt_tokens')} "
                        f"prompt_n={pn} gpu={gpu()}")
                print(line, flush=True)
                txt.write(line + "\n")
                txt.flush()
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
