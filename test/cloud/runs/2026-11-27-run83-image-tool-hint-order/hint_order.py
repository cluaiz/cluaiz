"""Run 83: image + tool hint order test (pending plan item 2), adapted from run 76's vision_order.py.

    python3 hint_order.py --exe <cluaiz> --home <CLUAIZ_HOME> --model <id> --runs 5 --out <dir> --tag <name>

One `cluaiz -vv serve` on port 8014 (never 8000), isolated home with no tools and no project items, so the Engine
adds no hint of its own (checked with return_prompt on the first request of each layout). The hint text is the
exact <tools_hint> block the Engine rendered in run 76 (raw/prompts_after.txt), sent by the client so its place
is controlled:
  H1_text_hint_then_image  [text(hint + question), image]   (run 50's failing layout)
  H2_no_hint               [text(question), image]
  H3_image_then_hint       [image, text(hint + question)]
Pass: HTTP 200, no tool call, reply contains "red". Temperature 0, max_tokens 64, think off.
Layouts rotate each round. Token read from <home>/session.token, never printed.
"""
import argparse, json, os, subprocess, sys, time
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from vision_order import IMAGE, text, post, wait_health, strip_image

HINT = ("<tools_hint>\nBefore a task that fits a rule, workflow or skill of the project's index, call read_item "
        "with its name and follow it.\n</tools_hint>\n")
Q = "What colour is this image? One word."
LAYOUTS = {
    "H1_text_hint_then_image": [text(HINT + Q), IMAGE],
    "H2_no_hint": [text(Q), IMAGE],
    "H3_image_then_hint": [IMAGE, text(HINT + Q)],
}


def main():
    p = argparse.ArgumentParser()
    for a in ("exe", "home", "model", "out", "tag"):
        p.add_argument("--" + a, required=True)
    p.add_argument("--port", type=int, default=8014)
    p.add_argument("--runs", type=int, default=5)
    a = p.parse_args()
    if a.port == 8000 or ".cluaiz" in a.home.split("/"):
        raise SystemExit("never port 8000 or ~/.cluaiz")
    os.makedirs(a.out, exist_ok=True)
    env = dict(os.environ, CLUAIZ_HOME=a.home, CLUAIZ_PORT=str(a.port), CLUAIZ_HOST="127.0.0.1",
               CLUAIZ_NONINTERACTIVE="1", NO_COLOR="1")
    log = open(os.path.join(a.out, a.tag + "_serve.log"), "w")
    proc = subprocess.Popen([a.exe, "-vv", "serve"], env=env, stdout=log, stderr=subprocess.STDOUT)
    jl = open(os.path.join(a.out, a.tag + ".jsonl"), "w")
    txt = open(os.path.join(a.out, a.tag + ".txt"), "w")
    score = {n: [0, 0] for n in LAYOUTS}
    names = list(LAYOUTS)
    try:
        wait_health(a.port, proc)
        tp = os.path.join(a.home, "session.token")
        token = open(tp).read().strip() if os.path.exists(tp) else ""
        steps = [(n, True) for n in names]
        for r in range(a.runs):
            o = r % 3
            steps += [(n, False) for n in names[o:] + names[:o]]
        for i, (name, probe) in enumerate(steps):
            body = {"model": a.model, "messages": [{"role": "user", "content": LAYOUTS[name]}],
                    "temperature": 0, "max_tokens": 64, "think_mode": "off"}
            if probe:
                body["return_prompt"] = True
            t0 = time.time()
            status, reply = post(a.port, token, body)
            secs = time.time() - t0
            msg = ((reply.get("choices") or [{}])[0]).get("message") or {}
            content = msg.get("content") or ""
            calls = [c.get("function", {}).get("name") for c in (msg.get("tool_calls") or [])]
            ok = status == 200 and not calls and "red" in content.lower()
            if not probe:
                score[name][0] += ok; score[name][1] += 1
            jl.write(json.dumps({"step": i, "layout": name, "probe": probe, "status": status, "pass": ok,
                                 "seconds": round(secs, 2), "request": strip_image(body), "reply": reply}) + "\n"); jl.flush()
            line = (f"{i} {'probe' if probe else 'run'} {name} {status} pass={ok} content={content[:100]!r} "
                    f"tool_calls={calls} prompt_tokens={(reply.get('usage') or {}).get('prompt_tokens')} {secs:.1f}s")
            print(line, flush=True); txt.write(line + "\n"); txt.flush()
    finally:
        proc.terminate()
        try: proc.wait(30)
        except subprocess.TimeoutExpired: proc.kill()
    for n, (g, t) in score.items():
        line = f"SCORE {a.tag} {n} {g}/{t}"; print(line); txt.write(line + "\n")


if __name__ == "__main__":
    main()
