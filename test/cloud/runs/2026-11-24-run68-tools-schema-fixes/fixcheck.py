"""Fix check for two bugs: tools on image requests (run 64) and response_format / grammar / tool_choice with
tools (run 65). Same requests before and after each fix.

python -B fixcheck.py --exe <cluaiz.exe> --home <isolated-home> --port 8013 --out <dir> [--cases v_img_tool,...] [--n 10]

Reuses run 64's image and get_weather helpers (vision_hint.py) and run 65's cases and judges (probe.py).
Every request's engine log lines are kept beside its result, so "constrained" / "grammar on" lines are counted
per request, not guessed. Results: <out>/results.jsonl, <out>/summary.jsonl (appended), <out>/engine.log.
"""
import argparse
import json
import os
import sys
import time
from pathlib import Path

# The runs folder (test/cloud/runs); FIXCHECK_RUNS when the script runs from outside it
RUNS = Path(os.environ.get("FIXCHECK_RUNS") or Path(__file__).resolve().parent.parent)
for folder in (RUNS / "2026-11-24-run64-vision-hint-layouts", RUNS / "2026-11-24-run65-json-schema-check"):
    sys.path.insert(0, str(folder))
import probe  # noqa: E402  (run 65: cases, judges, Engine/Journal from test/release_soak)
from vision_hint import IMAGE, TEXT, TOOL_TEXT, WEATHER, strip_image  # noqa: E402  (run 64)

VISION = "qwen3.5-2b-mtp-gguf-Q4_K_M"
TIME = {"type": "function", "function": {"name": "get_time", "description": "Current time in a city.",
                                         "parameters": {"type": "object", "properties": {"city": {"type": "string"}}, "required": ["city"]}}}
LOG_MARKERS = ("Answer constrained", "grammar on", "grammar none", "[NativeTools]", "tool_choice", "left out", "[ToolChoice]")


def constrained_in(lines):
    """Did this request's own generation run under a strict grammar? Lines after the request's last
    `Parsed N call(s)` belong to the chat's background card job (it sets its own grammar), so they
    are not counted."""
    parsed = [i for i, l in enumerate(lines) if "Parsed" in l and "call(s)" in l]
    own = lines[: parsed[-1] + 1] if parsed else lines
    return any("Answer constrained" in l or "lazy false" in l for l in own)


def vision_body(parts, tools=True):
    body = {"model": VISION, "messages": [{"role": "user", "content": parts}], "temperature": 0, "max_tokens": 128,
            "think_mode": "off", "stream": False}
    if tools:
        body["tools"] = [WEATHER]
    return body


def judge_call(name):
    def judge(reply):
        message = reply["choices"][0]["message"]
        calls = message.get("tool_calls") or []
        names = [c.get("function", {}).get("name") for c in calls]
        return bool(calls) and names[0] == name, {"tool_calls": calls, "content": message.get("content"),
                                                  "reasoning": (message.get("reasoning_content") or "")[:80]}
    return judge


def judge_red(reply):
    content = reply["choices"][0]["message"].get("content") or ""
    return "red" in content.lower(), {"content": content}


def judge_status(expected):
    def judge(status, reply):
        message = reply.get("error", {}).get("message") if isinstance(reply, dict) and isinstance(reply.get("error"), dict) else reply
        return status == expected, {"status": status, "error": message}
    return judge


def judge_schema_or_weather(reply):
    weather_ok, detail = probe.judge_tool(reply)
    json_ok, json_detail = probe.judge_json(reply, True)
    return weather_ok or json_ok, {**detail, **json_detail}


def cases():
    run65 = probe.cases()
    yes_no = probe.base("Is the sky blue on a clear day? Answer yes or no.")
    schema = run65["c_json_schema"][0]["response_format"]
    session = "fixcheck-engine-functions"
    out = {
        # Bug 1 (vision model, temperature 0)
        "v_img_tool": (vision_body([IMAGE, TOOL_TEXT]), judge_call("get_weather")),
        "v_txt_tool": (vision_body([TOOL_TEXT]), judge_call("get_weather")),
        "v_img_red": (vision_body([IMAGE, TEXT], tools=False), judge_red),
        # Bug 2 (qwen3-0.6b, temperature 0.8, run 65's settings)
        **{k: run65[k] for k in ("b_json_object", "c_json_schema", "d_grammar", "e_tool_required",
                                 "f_json_schema_with_tool", "h_required_unrelated")},
        "g_grammar_with_tool": (run65["g_grammar_with_tool"][0], judge_status(400)),
        # grammar with only the Engine's own functions (remember / forget come with a session)
        "g2_grammar_engine_functions": (dict(yes_no, grammar=probe.GBNF, session_id=session), probe.judge_yes_no),
        # schema with only the Engine's own functions
        "j_schema_engine_functions": (dict(run65["c_json_schema"][0], session_id=session + "-j"), lambda r: probe.judge_json(r, True)),
        # schema + tool on a question that needs the tool: a call or schema JSON, nothing else
        "f2_schema_tool_weather_question": (dict(probe.base("What is the weather in Paris right now?"), tools=probe.WEATHER,
                                                 response_format=schema), judge_schema_or_weather),
        # a named function: get_time forced although the question is about the weather
        "i_named_get_time": (dict(probe.base("What is the weather in Paris right now?"), tools=probe.WEATHER + [TIME],
                                  tool_choice={"type": "function", "function": {"name": "get_time"}}), judge_call("get_time")),
        "i2_named_unknown": (dict(probe.base("Hi"), tools=probe.WEATHER,
                                  tool_choice={"type": "function", "function": {"name": "nope"}}), judge_status(400)),
    }
    return out


def main():
    p = argparse.ArgumentParser()
    p.add_argument("--exe", type=Path, required=True)
    p.add_argument("--home", type=Path, required=True)
    p.add_argument("--port", type=int, default=8013)
    p.add_argument("--out", type=Path, required=True)
    p.add_argument("--n", type=int, default=10)
    p.add_argument("--cases", default="")
    args = p.parse_args()
    args.exe, args.home = args.exe.resolve(), args.home.resolve()
    if args.port in (8000, 8011) or args.home == Path.home() / ".cluaiz":
        p.error("use an isolated home and port")
    journal = probe.Journal(args.out, args.home)
    engine = probe.Engine(args, journal)
    all_cases = cases()
    wanted = [c for c in args.cases.split(",") if c] or list(all_cases)
    log_path = args.out / "engine.log"
    summary = {}
    try:
        engine.start()
        for name in wanted:
            body, judge = all_cases[name]
            # Warm-up per model (load), not counted
            engine.request("POST", "/v1/chat/completions", {"model": body["model"], "messages": [{"role": "user", "content": "hi"}],
                                                            "max_tokens": 4, "think_mode": "off"}, timeout=900, record=False)
            valid, times, prompt_n, constrained = 0, [], [], 0
            for i in range(args.n):
                time.sleep(0.3)
                start = log_path.stat().st_size if log_path.exists() else 0
                started = time.perf_counter()
                status, reply = engine.request("POST", "/v1/chat/completions", body, timeout=900, record=False)
                elapsed = time.perf_counter() - started
                time.sleep(0.5)
                with log_path.open("r", encoding="utf-8", errors="replace") as f:
                    f.seek(start)
                    lines = [l.rstrip() for l in f if any(m in l for m in LOG_MARKERS)]
                if judge.__code__.co_argcount == 2:
                    ok, detail = judge(status, reply)
                elif status == 200 and isinstance(reply, dict) and reply.get("choices"):
                    ok, detail = judge(reply)
                else:
                    ok, detail = False, {"status": status, "reply": reply}
                pn = (((reply.get("engine") or {}).get("timings")) or {}).get("prompt_n") if isinstance(reply, dict) else None
                is_constrained = constrained_in(lines)
                constrained += is_constrained
                valid += ok
                times.append(round(elapsed, 3))
                prompt_n.append(pn)
                journal.write("results.jsonl", {"case": name, "i": i, "status": status, "valid": ok, "elapsed_s": round(elapsed, 3),
                                                "prompt_n": pn, "constrained_log": is_constrained, "log": lines,
                                                "request": strip_image(body) if i == 0 else None, **detail})
                print(name, i, status, "VALID" if ok else "INVALID", f"prompt_n={pn}", f"constrained={is_constrained}",
                      json.dumps(detail, ensure_ascii=False)[:220], flush=True)
            summary[name] = {"valid": valid, "n": args.n, "constrained_log": constrained,
                             "median_s": sorted(times)[len(times) // 2], "prompt_n": sorted(set(x for x in prompt_n if x is not None))}
            journal.write("summary.jsonl", {"case": name, "exe": str(args.exe.parent.name), **summary[name]})
    finally:
        engine.stop()
    print(json.dumps(summary, indent=1))


if __name__ == "__main__":
    main()
