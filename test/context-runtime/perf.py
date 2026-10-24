"""PERF suite (plan §10.2): where the time and memory of a turn go, against an isolated Engine.

    python perf.py --exe target/release/cluaiz.exe --home <test home> --workspace <fixture> \
        [--model qwen3-4b-gguf-Q4_K_M] [--only PERF1,PERF6] [--cards 20000]

PERF1 preflight at about 10 and 100 installed tools (it adds 90 copies of a skill to the home for
the second half and removes them after), PERF2 the plan for a chat of --cards messages, PERF3 the
Engine's memory before and after that chat is indexed, PERF4 the tokens each turn processes and
why, PERF6 cold start. PERF5 (how fast a request stops an idle job) is in the Engine log only.
Times come from the context report's `timing` (the Engine's own clock), not from the network.
"""
import argparse
import json
import shutil
import statistics
import subprocess
import sys
import time
import uuid
from pathlib import Path

from harness import add_engine_args, engine_from
from memory_suite import build_chat

MODEL = "qwen3-4b-gguf-Q4_K_M"


def quick(engine, text, chat=None, **extra):
    body = {"model": MODEL, "messages": [{"role": "user", "content": text}], "max_tokens": 8, "think_mode": "off"}
    if chat:
        body["session_id"] = chat
    body.update(extra)
    return engine.chat(body)


def timing(response):
    return (response.get("context_report") or {}).get("timing") or {}


def median_of(engine, key, text, runs=5, **extra):
    values = [timing(quick(engine, text, **extra)).get(key, 0) for _ in range(runs)]
    return statistics.median(values), values


def working_set_mb(pid):
    out = subprocess.run(["powershell", "-NoProfile", "-Command", f"(Get-Process -Id {pid}).WorkingSet64"],
                         capture_output=True, text=True).stdout.strip()
    return round(int(out) / 1024 / 1024, 1) if out.isdigit() else None


def perf1(engine, args):
    """Preflight at about 10 and 100 tools"""
    text = "notes.txt padho aur calc.py chalao"
    small, _ = median_of(engine, "tools_ms", text)
    if not (engine.exe and engine.home):
        return f"tools {small} ms (installed tools as they are; 100 needs --exe and --home)"
    skills = Path(engine.home, "tools", "skills")
    source = skills / "code-reviewer"
    made = []
    try:
        for i in range(90):
            target = skills / f"perf-skill-{i:02d}"
            shutil.copytree(source, target)
            manifest = target / "package.json"
            data = json.loads(manifest.read_text(encoding="utf-8"))
            data["id"], data["name"] = target.name, f"Perf skill {i}"
            manifest.write_text(json.dumps(data), encoding="utf-8")
            made.append(target)
        engine.restart()
        quick(engine, "Hi")  # loads the model and the catalog
        large, _ = median_of(engine, "tools_ms", text)
        total = sum(1 for kind in ("skills", "mcp", "plugins") for d in Path(engine.home, "tools", kind).glob("*") if d.is_dir())
    finally:
        for target in made:
            shutil.rmtree(target, ignore_errors=True)
        if made:
            engine.restart()
    return f"tools part of preflight (median of 5): {small} ms installed as is, {large} ms with 90 more skills ({total} installed)"


def perf2_3(engine, args):
    """Plan time and memory for a chat of --cards messages"""
    history = build_chat(args.cards, seed=args.cards)
    chat = f"perf-{uuid.uuid4().hex[:8]}"
    # The model is loaded first, so the memory difference is the chat's alone
    quick(engine, "hi", tool_search=False)
    before = working_set_mb(engine.pid()) if engine.pid() else None
    started = time.time()
    first = quick(engine, "Mere deploy server ka port kya tha?", chat, history_mode="adaptive", tool_search=False,
                  messages=history + [{"role": "user", "content": "Mere deploy server ka port kya tha?"}])
    ingest_s = round(time.time() - started, 1)
    after = working_set_mb(engine.pid()) if engine.pid() else None
    plans = []
    for i in range(3):
        question = f"Project ka codename kya hai? ({i})"
        r = quick(engine, question, chat, history_mode="adaptive", tool_search=False,
                  messages=history + [{"role": "user", "content": question}])
        plans.append(timing(r).get("plan_ms", 0))
    cards = len(engine.get(f"/v1/chat/{chat}/index").get("cards", []))
    engine.delete(f"/v1/chat/{chat}/history")
    memory = f"; Engine memory {before} MB -> {after} MB with the chat indexed" if before else ""
    return (f"{cards} cards; first request (logs and indexes the chat) {ingest_s} s, "
            f"plan + recall after it {plans} ms{memory}") , first


def perf4(engine, args):
    """Tokens processed per turn and why"""
    chat = f"perf4-{uuid.uuid4().hex[:8]}"
    history, rows = [], []
    for text in ["Hi, my name is Asha.", "What is my name?", "Tell me a one-line joke.", "And one more."]:
        history.append({"role": "user", "content": text})
        r = engine.chat({"model": MODEL, "session_id": chat, "messages": history, "max_tokens": 40, "think_mode": "off",
                         "tool_search": False})
        context = r["engine"]["context"]
        rows.append(f"{context.get('processed_tokens')} of {context.get('prompt_tokens')} ({context.get('cause')})")
        history.append({"role": "assistant", "content": r["choices"][0]["message"]["content"] or ""})
    engine.delete(f"/v1/chat/{chat}/history")
    return "processed per turn: " + "; ".join(rows)


def perf6(engine, args):
    """Cold start"""
    if not engine.exe:
        return "needs --exe and --home"
    ready = engine.restart()
    started = time.time()
    r = quick(engine, "Hi")
    return f"Engine answers /health {ready:.1f} s after start; first chat (loads the model) {time.time() - started:.1f} s, " \
           f"model load {timing(r).get('model_load_ms')} ms"


def main():
    global MODEL
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    add_engine_args(parser)
    parser.add_argument("--model", default=MODEL)
    parser.add_argument("--cards", type=int, default=20000)
    parser.add_argument("--only", help="comma-separated: PERF1,PERF2,PERF4,PERF6 (PERF2 also gives PERF3)")
    args = parser.parse_args()
    MODEL = args.model
    engine = engine_from(args)
    if engine.exe:
        print(f"Engine ready in {engine.start():.1f} s")
    quick(engine, "Hi")  # the model is loaded before anything is timed
    picked = [p.strip().upper() for p in args.only.split(",")] if args.only else ["PERF1", "PERF2", "PERF4", "PERF6"]
    try:
        for pid in picked:
            started = time.time()
            if pid == "PERF1":
                line = perf1(engine, args)
            elif pid in ("PERF2", "PERF3"):
                line = perf2_3(engine, args)[0]
            elif pid == "PERF4":
                line = perf4(engine, args)
            elif pid == "PERF6":
                line = perf6(engine, args)
            else:
                line = "unknown"
            print(f"{pid:<6} {time.time() - started:7.1f}s  {line}", flush=True)
    finally:
        engine.stop()


if __name__ == "__main__":
    sys.exit(main())
