"""J-suite: the context runtime's journeys (plan §10.2) against an isolated Engine.

    python journeys.py --exe target/release/cluaiz.exe --home <test home> --workspace <fixture> \
        [--model qwen3-4b-gguf-Q4_K_M] [--only J1,J4]

The fixture folder holds notes.txt ("The secret word is MANGO-42.") and calc.py ("print(6 * 7)");
the home has the `filesystem`, `pynotes` and `everything` MCP servers and the `code-interpreter`
skill. Journeys that restart the Engine (J10, J11, J15) need --exe and --home. Model-dependent journeys
can fail on a small model even when the Engine is right; the detail line says what happened.
"""
import argparse
import sys
import threading
import time
import uuid

from harness import Engine, add_engine_args, engine_from

MODEL = "qwen3-4b-gguf-Q4_K_M"
THINK = None


def session(name):
    return f"j-{name}-{uuid.uuid4().hex[:8]}"


def body(text, chat=None, **extra):
    out = {"model": MODEL, "messages": [{"role": "user", "content": text}], "max_tokens": 1200}
    if THINK:
        out["think_mode"] = THINK
    if chat:
        out["session_id"] = chat
    out.update(extra)
    return out


def tools_of(result):
    return (result.get("context") or {}).get("tools") or {}


def names(result):
    return [f"{c['tool']} {c['name']}".lower() for c in result["calls"]]


def no_tool_talk(engine, name, text):
    # thinking off: these check what reaches the model, not the answer
    r = engine.stream(body(text, session(name), think_mode="off"))
    t = tools_of(r)
    ok = not r["error"] and not r["calls"] and not t.get("suggested") and t.get("hint_tokens", 0) == 0 \
        and t.get("system_tools_tokens", 0) == 0
    return ok, f"suggested {[s.get('id') for s in t.get('suggested') or []]}, hint {t.get('hint_tokens')} tokens, calls {names(r)}"


def j1(engine):
    """"Hi" gets 0 tool tokens"""
    return no_tool_talk(engine, "j1", "Hi")


def j2(engine):
    """"I trust you" gets no suggestion"""
    return no_tool_talk(engine, "j2", "I trust you")


def j3(engine):
    """"protocol kya hota hai" gets no tool-creator"""
    r = engine.stream(body("protocol kya hota hai", session("j3"), think_mode="off"))
    suggested = [s.get("id") for s in tools_of(r).get("suggested") or []]
    return "tool-creator" not in suggested and not r["error"], f"suggested {suggested}, calls {names(r)}"


def j4(engine):
    """"notes.txt padho": filesystem suggested, loaded, read with approval, MANGO-42"""
    r = engine.stream(body("notes.txt padho aur secret word batao", session("j4")))
    read = any("filesystem" in n and "read" in n for n in names(r))
    return read and "MANGO-42" in r["text"] and not r["error"], \
        f"calls {names(r)}, asked {len(r['asked'])}, answer {r['text'][-120:]!r}"


def j5(engine):
    """"run this python: print(2+2)": code-interpreter runs it, 4"""
    r = engine.stream(body("run this python and tell me the output: print(2+2)", session("j5")))
    ran = any("code-interpreter" in n or "execute" in n for n in names(r))
    return ran and "4" in r["text"] and not r["error"], f"calls {names(r)}, answer {r['text'][-120:]!r}"


def j7(engine):
    """Three MCP servers in one answer"""
    r = engine.stream(body("Do three things: read notes.txt with the filesystem tool, read the pynotes note named "
                           "secret, and use the everything server's echo tool to echo hello. Then report all three.",
                           session("j7"), tools=["filesystem", "pynotes", "everything"], tool_search=False))
    got = names(r)
    servers = {s for s in ("filesystem", "pynotes", "everything") if any(s in n for n in got)}
    return len(servers) == 3 and not r["error"], f"servers {sorted(servers)}, calls {got}"


def _engine_context(result):
    return ((result.get("engine") or {}).get("engine") or {}).get("context") or {}


def j6(engine):
    """Wrong load then unload (code-reviewer, then code-interpreter): the wrong load becomes a short note"""
    chat = session("j6")
    text = ("First call load_tool for code-reviewer. It cannot run code, so then call unload_tool for code-reviewer, "
            "call load_tool for code-interpreter and run this python: print(3*3)")
    r = engine.stream(body(text, chat, think_mode="off"))
    loads = [c["arguments"] for c in r["calls"] if c["name"] == "load_tool"]
    unloaded = any(c["name"] == "unload_tool" and "code-reviewer" in c["arguments"] for c in r["calls"])
    # The next turn's prompt: the ledger keeps the steps, the unloaded tool's instructions are a note
    history = [{"role": "user", "content": text}, {"role": "assistant", "content": r["text"]},
               {"role": "user", "content": "Thanks. What was the result?"}]
    r2 = engine.stream(dict(body("", chat, think_mode="off", max_tokens=40, return_prompt=True), messages=history))
    context = _engine_context(r2)
    prompt = context.get("prompt") or ""
    note = "[Tool 'code-reviewer' was unloaded.]" in prompt
    instructions_gone = "Code Reviewer & Security Audit Protocol" not in prompt
    ok = any("code-reviewer" in a for a in loads) and unloaded and "9" in r["text"] and note and instructions_gone \
        and not r["error"] and not r2["error"]
    return ok, (f"calls {names(r)}, note {note}, instructions gone {instructions_gone}; next turn processed "
                f"{context.get('processed_tokens')} of {context.get('prompt_tokens')} ({context.get('cause')})")


def j11(engine):
    """Skill too big for the room left: load_tool refuses it, nothing of it is cut into the prompt"""
    if not (engine.exe and engine.home):
        return None, "needs --exe and --home (it installs a large skill for the run)"
    import json as _json
    import shutil
    from pathlib import Path
    skills = Path(engine.home, "tools", "skills")
    target = skills / "j11-huge-manual"
    shutil.copytree(skills / "code-reviewer", target)
    try:
        manifest = target / "package.json"
        data = _json.loads(manifest.read_text(encoding="utf-8"))
        data["id"], data["name"] = target.name, "J11 huge manual"
        manifest.write_text(_json.dumps(data), encoding="utf-8")
        skill = target / "SKILL.md"
        filler = "".join(f"J11-FILLER line {i}: a long manual that no context has room for.\n" for i in range(6000))
        skill.write_text(skill.read_text(encoding="utf-8") + "\n" + filler, encoding="utf-8")
        engine.restart()
        r = engine.stream(body("Call load_tool for j11-huge-manual, then tell me in one line whether it loaded.",
                               session("j11"), think_mode="off", max_tokens=200, return_prompt=True))
        prompt = _engine_context(r).get("prompt") or ""
        refused = "j11-huge-manual' needs about" in prompt
        cut = "J11-FILLER" in prompt
        ok = any(c["name"] == "load_tool" for c in r["calls"]) and refused and not cut and not r["error"]
        return ok, f"calls {names(r)}, refused {refused}, any of it in the prompt {cut}, answer {r['text'][-100:]!r}"
    finally:
        shutil.rmtree(target, ignore_errors=True)
        engine.restart()


def _non_streaming(engine, decision):
    """A non-streaming call of a bound tool; a thread answers its approval from the pending list"""
    chat = session("ns")
    done = threading.Event()

    def answer():
        while not done.is_set():
            pending = engine.get("/v1/system/permission/pending").get("pending", [])
            for item in pending:
                if item.get("status") == "pending":
                    engine.post("/v1/system/permission/" + decision, {"request_id": item["id"]}, timeout=60)
                    return
            time.sleep(0.5)

    worker = threading.Thread(target=answer, daemon=True)
    if decision:
        worker.start()
    try:
        response = engine.chat(body("Read notes.txt and tell me the secret word.", chat, tools=["filesystem"], tool_search=False))
    finally:
        done.set()
    return response["choices"][0]["message"]["content"] or ""


def j8(engine):
    """Non-streaming approve"""
    text = _non_streaming(engine, "approve")
    return "MANGO-42" in text, f"answer {text[-120:]!r}"


def j9(engine):
    """Non-streaming reject: the tool does not run and the model is told"""
    text = _non_streaming(engine, "reject")
    return "MANGO-42" not in text, f"answer {text[-120:]!r}"


def j9t(engine):
    """Non-streaming timeout: nobody answers the approval, the tool does not run"""
    started = time.time()
    text = _non_streaming(engine, None)
    waited = time.time() - started
    return "MANGO-42" not in text and waited >= 115, f"waited {waited:.0f} s, answer {text[-120:]!r}"


def j10(engine):
    """Restart in the middle of a chat: loaded tools, ledger and KV come back"""
    if not engine.exe:
        return None, "needs --exe and --home"
    chat = session("j10")
    first = engine.stream(body("notes.txt padho aur secret word batao", chat))
    history = [{"role": "user", "content": "notes.txt padho aur secret word batao"},
               {"role": "assistant", "content": first["text"]}]
    engine.restart()
    second = engine.stream(dict(body("calc.py me kya likha hai?", chat), messages=history + [{"role": "user", "content": "calc.py me kya likha hai?"}]))
    loaded = tools_of(second).get("loaded_in_chat") or []
    cache = ((second.get("engine") or {}).get("engine") or {}).get("context", {}).get("cache_n", 0)
    reloaded = any("load_tool" in n for n in names(second))
    ok = "filesystem" in loaded and not reloaded and cache > 0 and "6 * 7" in second["text"]
    return ok, f"loaded after restart {loaded}, load_tool again {reloaded}, cache_n {cache}, answer {second['text'][-80:]!r}"


def j12(engine):
    """tool_choice none and tool_search false give zero tools"""
    a = engine.stream(body("notes.txt padho", session("j12a"), tool_choice="none", think_mode="off"))
    b = engine.stream(body("notes.txt padho", session("j12b"), tool_search=False, think_mode="off"))
    ta, tb = tools_of(a), tools_of(b)
    ok = not a["calls"] and not b["calls"] and not ta.get("meta_tools") and not ta.get("suggested") \
        and not tb.get("suggested") and ta.get("hint_tokens", 0) == 0 and tb.get("hint_tokens", 0) == 0
    return ok, f"none: suggested {ta.get('suggested')}, meta {ta.get('meta_tools')}; tool_search false: suggested {tb.get('suggested')}"


def j13(engine):
    """read_tool_file: a skill's reference file, read whole"""
    r = engine.stream(body("Use read_tool_file to read references/LANGUAGES.md of the code-interpreter tool, then tell me "
                           "the exact command it gives to run Go code.", session("j13"), tools=["code-interpreter"]))
    read = any("read_tool_file" in n for n in names(r))
    return read and "go run" in r["text"] and not r["error"], f"calls {names(r)}, answer {r['text'][-100:]!r}"


def j14(engine):
    """Idle auto-unload: a loaded tool goes after 3 answers without use; a pinned one stays"""
    chat = session("j14")
    engine.post(f"/v1/chat/{chat}/tools", {"tools": [{"id": "filesystem", "turns": -1, "inline": True},
                                                      {"id": "pynotes", "turns": -1, "inline": True, "pinned": True}]})
    history, seen = [], []
    for text in ["Hi!", "How are you?", "Tell me a one-line joke."]:
        history.append({"role": "user", "content": text})
        r = engine.stream(dict(body(text, chat, think_mode="off", max_tokens=60), messages=list(history)))
        history.append({"role": "assistant", "content": r["text"]})
        seen.append([t["id"] for t in engine.get(f"/v1/chat/{chat}/tools")["active_tools"]])
    ok = "filesystem" in seen[1] and "filesystem" not in seen[2] and all("pynotes" in s for s in seen)
    return ok, f"loaded after each answer {seen}"


def _children(pid):
    """(pid, command line) of every process under `pid`"""
    import json as _json
    import subprocess
    out = subprocess.run(["powershell", "-NoProfile", "-Command",
                          "Get-CimInstance Win32_Process | Select-Object ProcessId,ParentProcessId,CommandLine | ConvertTo-Json"],
                         capture_output=True, text=True).stdout
    rows = _json.loads(out or "[]")
    found, frontier = [], {pid}
    while frontier:
        nxt = {r["ProcessId"] for r in rows if r["ParentProcessId"] in frontier}
        found += [(r["ProcessId"], r.get("CommandLine") or "") for r in rows if r["ProcessId"] in nxt]
        frontier = nxt
    return found


def j15(engine):
    """MCP crash recovery: a killed server starts again on the next call"""
    if not engine.exe:
        return None, "needs --exe (only servers under this Engine are killed)"
    import subprocess
    # A fresh Engine: the crash limit (3 in 10 minutes) counts earlier runs' kills too; greedy
    # sampling, so the answer does not depend on a draw
    engine.restart()
    chat = session("j15")
    extra = dict(tools=["filesystem"], tool_search=False, temperature=0)
    first = engine.stream(body("Read notes.txt and tell me the secret word.", chat, **extra))
    killed = [p for p, cmd in _children(engine.pid()) if "server-filesystem" in cmd]
    for p in killed:
        subprocess.run(["taskkill", "/PID", str(p), "/T", "/F"], capture_output=True)
    time.sleep(2)
    second = engine.stream(body("Read calc.py and show what it contains.", chat, return_prompt=True, **extra))
    # The Engine's part: the server started again and its result reached the model (the prompt
    # of the answer holds it). Whether the model then writes about it is the model's part: Gemma 4
    # at temperature 0 sometimes ends right after this result.
    prompt = _engine_context(second).get("prompt") or ""
    result_in_prompt = "print(6 * 7)" in prompt
    ok = "MANGO-42" in first["text"] and bool(killed) and result_in_prompt and not second["error"]
    return ok, (f"killed {len(killed)} process(es); after: calls {names(second)}, result reached the model "
                f"{result_in_prompt}, answer {second['text'][-80:]!r}")


JOURNEYS = {"J1": j1, "J2": j2, "J3": j3, "J4": j4, "J5": j5, "J6": j6, "J7": j7, "J8": j8, "J9": j9, "J9T": j9t, "J10": j10, "J11": j11, "J12": j12, "J13": j13, "J14": j14, "J15": j15}


def main():
    global MODEL
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    add_engine_args(parser)
    parser.add_argument("--model", default=MODEL)
    parser.add_argument("--only", help="comma-separated journey ids")
    parser.add_argument("--think", choices=["on", "off"], help="think_mode for every request (default: the model's)")
    args = parser.parse_args()
    global THINK
    MODEL, THINK = args.model, args.think
    engine = engine_from(args)
    if engine.exe:
        print(f"Engine ready in {engine.start():.1f} s")
    picked = [j.strip().upper() for j in args.only.split(",")] if args.only else list(JOURNEYS)
    failed = 0
    try:
        for jid in picked:
            started = time.time()
            try:
                ok, detail = JOURNEYS[jid](engine)
            except Exception as e:
                ok, detail = False, f"{type(e).__name__}: {e}"
            flag = "SKIP" if ok is None else "PASS" if ok else "FAIL"
            failed += ok is False
            print(f"{flag} {jid:<4} {JOURNEYS[jid].__doc__.splitlines()[0][:60]:<60} {time.time() - started:6.1f}s  {detail}", flush=True)
    finally:
        engine.stop()
    print(f"\n{len(picked) - failed} of {len(picked)} journeys did not fail ({MODEL})")
    sys.exit(1 if failed else 0)


if __name__ == "__main__":
    main()
