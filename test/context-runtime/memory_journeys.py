"""Memory journeys (plan §10.2 MEM3, MEM5, MEM6, P4.2 ENR1, PERF5, the C2 J-variant) against an
isolated Engine.

    python memory_journeys.py --exe target/release/cluaiz.exe --home <test home> --workspace <fixture> \
        [--model gemma-4-e4b-it-gguf-Q4_K_M] [--only MEM3,MEM5]

MEM3 and MEM5 import a chat with times and tool steps of their own (`POST /v1/chat/{id}/import`),
so their checks do not depend on the model. MEM6 changes `cross_chat` with
`POST /v1/context/settings` and sets it back after. The fixture folder holds notes.txt
("The secret word is MANGO-42.").
"""
import argparse
import json
import sys
import time
import urllib.request
import uuid
from datetime import datetime, timedelta, timezone

from harness import add_engine_args, engine_from

MODEL = "gemma-4-e4b-it-gguf-Q4_K_M"


def session(name):
    return f"m-{name}-{uuid.uuid4().hex[:8]}"


def body(text, chat=None, **extra):
    out = {"model": MODEL, "messages": [{"role": "user", "content": text}], "max_tokens": 300, "think_mode": "off"}
    if chat:
        out["session_id"] = chat
    out.update(extra)
    return out


def engine_context(response):
    """The engine report's `context` of a non-streaming response"""
    return (response.get("engine") or {}).get("context") or {}


def answer_of(response):
    return response["choices"][0]["message"]["content"] or ""


def iso(at):
    return at.astimezone(timezone.utc).strftime("%Y-%m-%dT%H:%M:%S.000Z")


def import_chat(engine, chat, items):
    """`items`: (time, kind, fields) in order; a message's `pos` is counted here"""
    events, pos = [], 0
    for seq, (at, kind, fields) in enumerate(items, 1):
        event = {"v": 1, "id": f"ev_{chat}_{seq}", "chat": chat, "seq": seq, "ts": iso(at), "kind": kind}
        if kind == "message":
            fields = dict(fields, pos=pos, source="client" if fields["role"] == "user" else "engine")
            pos += 1
        events.append(dict(event, **fields))
    return engine.post(f"/v1/chat/{chat}/import", {"events": events})


def user(at, text):
    return (at, "message", {"role": "user", "text": text})


def assistant(at, text):
    return (at, "message", {"role": "assistant", "text": text})


def filler(start, turns):
    return [item for i in range(turns) for item in (user(start + timedelta(minutes=2 * i), f"Cricket score {i} aur mausam ki baat"),
                                                     assistant(start + timedelta(minutes=2 * i, seconds=5), f"Haan, score {i} accha tha."))]


def mem3(engine):
    """Time questions: kal, parso and a weekday keep to that day, in the user's own clock"""
    chat = session("mem3")
    today = datetime.now().astimezone().replace(hour=11, minute=0, second=0, microsecond=0)
    if datetime.now().astimezone().hour < 11:
        today -= timedelta(hours=2)
    days = {0: "9090", 1: "7070", 2: "5050"}
    items = []
    for back in (2, 1, 0):
        at = today - timedelta(days=back)
        items += [user(at, f"Deploy server ka port {days[back]} rakhte hain"), assistant(at + timedelta(seconds=5), f"Theek hai, port {days[back]}.")]
    import_chat(engine, chat, items + filler(today + timedelta(minutes=5), 10))
    rows = []
    weekday = (today - timedelta(days=2)).strftime("%A")
    for query, want, other in [("kal deploy port kya tha?", "7070", ("9090", "5050")),
                               ("parso deploy port kya rakha?", "5050", ("9090", "7070")),
                               (f"{weekday} ko deploy port kya tha?", "5050", ("9090", "7070")),
                               ("aaj deploy port kya hai?", "9090", ("7070", "5050"))]:
        results = engine.post("/v1/recall/search", {"chat_id": chat, "query": query, "limit": 5})["results"]
        texts = [engine.get(f"/v1/recall/item/{r['ref']}?chat_id={chat}").get("text", "") for r in results]
        found = any(want in t for t in texts) and not any(o in t for t in texts for o in other)
        rows.append((query, found))
    engine.delete(f"/v1/chat/{chat}/history")
    return all(ok for _, ok in rows), "; ".join(f"{q!r} {'right day' if ok else 'WRONG'}" for q, ok in rows)


def mem5(engine):
    """Recalling a tool output (tool_only): the run's result comes back, not the answer around it"""
    chat = session("mem5")
    start = datetime.now().astimezone() - timedelta(days=1)
    call = '<tool_call>\n{"name": "execute_code", "arguments": {"code": "exec(open(\'calc_report.py\').read())"}}\n</tool_call>'
    items = [user(start, "calc_report.py chalao"),
             (start + timedelta(seconds=2), "tool_call", {"step": 0, "tool": "code-interpreter", "call": call, "files": ["calc_report.py"]}),
             (start + timedelta(seconds=4), "tool_result", {"step": 0, "tool": "code-interpreter", "result": "stdout: checksum ZEBRA-7781 written to out.txt"}),
             assistant(start + timedelta(seconds=6), "Script chal gayi, sab theek hai.")]
    items += filler(start + timedelta(minutes=5), 10)
    imported = import_chat(engine, chat, items)
    # The App sends the whole chat; history_mode none shows none of it, so recall brings back what fits
    history = [{"role": f["role"], "content": f["text"]} for _, kind, f in items if kind == "message"]
    question = "calc_report.py ke pichhle run ka output kya aaya tha? Checksum batao."
    r = engine.chat(dict(body(question, chat, storage="external", history_mode="none", tool_search=False, return_prompt=True, max_tokens=80),
                         messages=history + [{"role": "user", "content": question}]))
    prompt = engine_context(r).get("prompt") or ""
    recalled = ((r.get("context_report") or {}).get("plan") or {}).get("recall") or {}
    levels = [x.get("level") for x in recalled.get("recalled", [])]
    in_prompt = "returned: stdout: checksum ZEBRA-7781" in prompt
    answer = answer_of(r)
    engine.delete(f"/v1/chat/{chat}/history")
    ok = in_prompt and "tool_only" in levels and "ZEBRA-7781" in answer
    return ok, f"{imported.get('cards')} cards; levels {levels}; result in prompt {in_prompt}; answer {answer[-100:]!r}"


def _set_cross_chat(engine, value):
    engine.post("/v1/context/settings", {"cross_chat": value})


def mem6(engine):
    """Cross-chat recall with ask, allow and deny: deny never returns another chat's text"""
    before = engine.get("/v1/context/settings")["settings"]["cross_chat"]
    code = f"HERON-{uuid.uuid4().hex[:4].upper()}"
    first = session("mem6-a")
    engine.chat(body(f"Yaad rakhna: hamare naye project ka codename {code} hai. Bas 'ok' bolo.", first, max_tokens=20))
    question = "Dusri chat me maine naye project ka codename kya bataya tha? Recall se dhundo."
    rows = {}
    try:
        _set_cross_chat(engine, "allow")
        r = engine.stream(dict(body(question, session("mem6-allow")), stream=True))
        rows["allow"] = (code in r["text"], [c["name"] for c in r["calls"]])
        _set_cross_chat(engine, "deny")
        r = engine.stream(dict(body(question, session("mem6-deny")), stream=True))
        search_status = None
        try:
            engine.post("/v1/recall/search", {"chat_id": session("x"), "query": "codename", "scope": "all"})
        except urllib.error.HTTPError as e:
            search_status = e.code
        rows["deny"] = (code not in r["text"] and search_status == 403, [c["name"] for c in r["calls"]], search_status)
        _set_cross_chat(engine, "ask")
        asked = []

        def reject(request):
            asked.append(request.get("capability") or request.get("tool"))
            return "reject"
        r = engine.stream(dict(body(question, session("mem6-ask")), stream=True), decide=reject)
        rows["ask+reject"] = (code not in r["text"] and bool(asked), asked)
    finally:
        _set_cross_chat(engine, before)
        engine.delete(f"/v1/chat/{first}/history")
    ok = rows["allow"][0] and rows["deny"][0] and rows["ask+reject"][0]
    return ok, f"code {code}; " + "; ".join(f"{k}: {v}" for k, v in rows.items())


def _drain(engine, timeout=120):
    """Empties the job queue of earlier tests: with the indexer off, queued jobs are skipped"""
    engine.post("/v1/context/settings", {"indexer": "off"})
    deadline = time.time() + timeout
    while time.time() < deadline:
        jobs = engine.get("/v1/context/jobs")
        if not jobs.get("queue") and not jobs.get("running"):
            return
        time.sleep(0.5)


def _wait_job(engine, chat, timeout=90):
    """The idle-time job of `chat` once it is over: (outcome, ms)"""
    deadline = time.time() + timeout
    while time.time() < deadline:
        for job in engine.get("/v1/context/jobs").get("recent", []):
            if job.get("chat") == chat:
                return job.get("outcome"), job.get("ms")
        time.sleep(0.5)
    return None, None


def enr1(engine):
    """Card enrichment on the hot KV: the next request reuses the cache as much as with no card job"""
    processed, details = {}, []
    before = engine.get("/v1/context/settings")["settings"]["indexer"]
    try:
        for indexer in ("off", "always"):
            _drain(engine)
            engine.post("/v1/context/settings", {"indexer": indexer})
            chat = session(f"enr1-{indexer}")
            history = [{"role": "user", "content": "Mera deploy server ka naam orion hai aur port 8011. Ek line me confirm karo."}]
            r = engine.chat(dict(body("", chat, max_tokens=40), messages=history))
            history.append({"role": "assistant", "content": answer_of(r)})
            outcome = _wait_job(engine, chat, 120)[0] if indexer == "always" else "off"
            history.append({"role": "user", "content": "Port kya tha?"})
            r = engine.chat(dict(body("", chat, max_tokens=20), messages=history))
            context = engine_context(r)
            processed[indexer] = context.get("processed_tokens", 0)
            card = [c for c in engine.get(f"/v1/chat/{chat}/index")["cards"] if c.get("enrichment")]
            details.append(f"{indexer}: job {outcome}, card {bool(card)}, processed {processed[indexer]} of {context.get('prompt_tokens')}")
            engine.delete(f"/v1/chat/{chat}/history")
    finally:
        engine.post("/v1/context/settings", {"indexer": before})
    # The new message only, as without the job: the job's own prompt left no trace in the KV
    ok = "card True" in details[1] and processed["always"] <= processed["off"] + 2
    return ok, "; ".join(details)


def _ttft(engine, payload):
    """Seconds from sending a streaming request to its first answer text"""
    req = urllib.request.Request(engine.url + "/v1/chat/completions", data=json.dumps(dict(payload, stream=True)).encode(), method="POST",
                                 headers={"Authorization": "Bearer " + engine.token, "Content-Type": "application/json"})
    started, first = time.perf_counter(), None
    with urllib.request.urlopen(req, timeout=300) as r:
        for raw in r:
            line = raw.decode("utf-8", "replace").strip()
            if first is None and line.startswith("data:") and '"content"' in line and '"content":""' not in line.replace(" ", ""):
                first = time.perf_counter() - started
    return first


def perf5(engine):
    """Preemption: a request that arrives during an idle-time job waits for one prompt chunk at most"""
    before = engine.get("/v1/context/settings")["settings"]["indexer"]
    # A new question each time: no probe finds its prompt in the cache
    probe = lambda: body(f"Say hi in one word. ({uuid.uuid4().hex[:6]})", max_tokens=5, storage="none", tool_search=False)
    try:
        _drain(engine)
        engine.post("/v1/context/settings", {"indexer": "always"})
        _ttft(engine, probe())  # warm
        normal = sorted(_ttft(engine, probe()) for _ in range(5))[2]
        chat = session("perf5")
        engine.chat(body("Mera deploy server orion hai, port 8011. Ek line me confirm karo.", chat, max_tokens=40))
        deadline = time.time() + 30
        while time.time() < deadline and not engine.get("/v1/context/jobs").get("running"):
            time.sleep(0.02)
        running = engine.get("/v1/context/jobs").get("running")
        during = _ttft(engine, probe())
        outcome = _wait_job(engine, chat, 30)[0]
        engine.delete(f"/v1/chat/{chat}/history")
    finally:
        engine.post("/v1/context/settings", {"indexer": before})
    extra_ms = round((during - normal) * 1000)
    # The job stops between two prompt chunks (or tokens): the wait is at most one decode, which
    # is about one short prompt's time to first text (plan: 50 ms; not reachable when one decode
    # alone takes longer, as with layers in RAM)
    ok = running and outcome == "stopped" and extra_ms <= max(50, normal * 1000)
    return ok, f"time to first text {round(normal * 1000)} ms idle, {round(during * 1000)} ms during a job ({extra_ms:+} ms); job was running {running}, outcome {outcome}"


def c2(engine):
    """Client mode: an earlier tool step goes back with its answer ("what did you read last time")"""
    chat = session("c2")
    history = [{"role": "user", "content": "Read notes.txt and tell me the secret word."}]
    r = engine.stream(dict(body("", chat, tools=["filesystem"], tool_search=False), messages=history))
    history.append({"role": "assistant", "content": r["text"]})
    for text in ["Thanks!", "Tell me a one-line joke."]:
        history.append({"role": "user", "content": text})
        r2 = engine.chat(dict(body("", chat, max_tokens=60, tool_search=False), messages=history))
        history.append({"role": "assistant", "content": answer_of(r2)})
    history.append({"role": "user", "content": "Pichhli baar notes.txt me tumne kya padha tha? Uski pehli line batao."})
    r3 = engine.chat(dict(body("", chat, max_tokens=80, tool_search=False, return_prompt=True), messages=history))
    prompt = engine_context(r3).get("prompt") or ""
    # No tool is bound or suggested now: the call can only be in the prompt as the earlier step
    reattached = "read_text_file" in prompt and "The secret word is MANGO-42" in prompt
    answer = answer_of(r3)
    engine.delete(f"/v1/chat/{chat}/history")
    ok = any("read_text_file" in c["name"] for c in r["calls"]) and reattached and "Project notes" in answer
    return ok, f"first calls {[c['name'] for c in r['calls']]}; step in the 4th prompt {reattached}; answer {answer[-100:]!r}"


JOURNEYS = {"MEM3": mem3, "MEM5": mem5, "MEM6": mem6, "ENR1": enr1, "PERF5": perf5, "C2": c2}


def main():
    global MODEL
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    add_engine_args(parser)
    parser.add_argument("--model", default=MODEL)
    parser.add_argument("--only", help="comma-separated ids")
    args = parser.parse_args()
    MODEL = args.model
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
            failed += not ok
            print(f"{'PASS' if ok else 'FAIL'} {jid:<5} {JOURNEYS[jid].__doc__.splitlines()[0][:60]:<60} {time.time() - started:6.1f}s  {detail}", flush=True)
    finally:
        engine.stop()
    print(f"\n{len(picked) - failed} of {len(picked)} did not fail ({MODEL})")
    sys.exit(1 if failed else 0)


if __name__ == "__main__":
    main()
