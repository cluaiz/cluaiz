"""Edge check for commits 790819e2 (image / audio requests get tool schemas) and 5cec4478 (response_format,
grammar and tool_choice with tools): the paths run 68 did not test. Streaming, thinking on, json_schema with
thinking and with streaming, and a text-only regression.

python -B edgecheck.py --exe <cluaiz.exe> --home <isolated-home> --port 8013 --out <dir> [--cases a,b] [--n 10]

Reuses run 68's fixcheck.py (request loop ideas, `constrained_in`, the vision body and judges), which reuses run 64's
image and get_weather (vision_hint.py) and run 65's cases and judges (probe.py). A streamed reply is read chunk by
chunk (SSE) and folded into the same shape as a non-streamed reply, so one set of judges serves both; the stream
checks also require a `finish_reason` and `data: [DONE]`, and no call markup (`<tool_call>`) in the streamed text.
Every request's engine log lines ([NativeStream] / [NativeTools] / tool_choice) are kept beside its result.
Results: <out>/results.jsonl, <out>/summary.jsonl (appended), <out>/engine.log.
"""
import argparse
import json
import os
import sys
import time
from pathlib import Path

RUNS = Path(os.environ.get("FIXCHECK_RUNS") or Path(__file__).resolve().parent.parent)
os.environ.setdefault("FIXCHECK_RUNS", str(RUNS))
sys.path.insert(0, str(RUNS / "2026-11-24-run68-tools-schema-fixes"))
import fixcheck  # noqa: E402  (run 68)
from fixcheck import IMAGE, TEXT, TOOL_TEXT, VISION, WEATHER, probe, strip_image  # noqa: E402

LOG_MARKERS = ("[NativeStream]", "[NativeTools]", "tool_choice", "left out", "[ToolChoice]")
SCHEMA_RF = probe.cases()["c_json_schema"][0]["response_format"]


def constrained_in(lines):
    """Did this request's own generation run under a strict grammar? Run 68's rule, except that only
    `grammar on (lazy false` counts, not `grammar none (lazy false` (a plain answer with no grammar, which
    run 68's `"lazy false" in line` also matched). Lines after the last `Parsed N call(s)` are not counted."""
    parsed = [i for i, l in enumerate(lines) if "Parsed" in l and "call(s)" in l]
    own = lines[: parsed[-1] + 1] if parsed else lines
    return any("Answer constrained" in l or "grammar on (lazy false" in l for l in own)


CALL_MARKUP = ("<tool_call>", "</tool_call>", "<function=", "<|tool_call")


def strip_media(req):
    """The request as logged: base64 image (run 64's strip_image) and audio replaced by their length."""
    copy = strip_image(req)
    for m in copy.get("messages", []):
        for part in m.get("content") if isinstance(m.get("content"), list) else []:
            if part.get("type") == "input_audio":
                part["input_audio"]["data"] = f"<wav base64, {len(part['input_audio']['data'])} chars>"
    return copy


def parse_sse(raw):
    """Fold an SSE body into a non-streamed reply shape, plus stream facts for the judges."""
    content, reasoning, calls, finish, done, chunks, errors, usage = [], [], {}, None, False, 0, [], None
    for line in raw.splitlines():
        if not line.startswith("data:"):
            continue
        data = line[5:].strip()
        if data == "[DONE]":
            done = True
            continue
        try:
            chunk = json.loads(data)
        except ValueError:
            errors.append(data[:200])
            continue
        if not isinstance(chunk, dict):
            continue
        if chunk.get("error"):
            errors.append(chunk["error"])
        if chunk.get("usage"):
            usage = chunk["usage"]
        for choice in chunk.get("choices") or []:
            chunks += 1
            delta = choice.get("delta") or {}
            if delta.get("content"):
                content.append(delta["content"])
            if delta.get("reasoning_content"):
                reasoning.append(delta["reasoning_content"])
            for call in delta.get("tool_calls") or []:
                slot = calls.setdefault(call.get("index", 0), {"id": None, "type": "function", "function": {"name": "", "arguments": ""}})
                slot["id"] = call.get("id") or slot["id"]
                fn = call.get("function") or {}
                slot["function"]["name"] += fn.get("name") or ""
                args = fn.get("arguments")
                slot["function"]["arguments"] += args if isinstance(args, str) else json.dumps(args) if args is not None else ""
            if choice.get("finish_reason"):
                finish = choice["finish_reason"]
    message = {"role": "assistant", "content": "".join(content) or None, "reasoning_content": "".join(reasoning) or None,
               "tool_calls": [calls[k] for k in sorted(calls)] or None}
    reply = {"choices": [{"index": 0, "message": message, "finish_reason": finish}], "usage": usage}
    facts = {"stream_done": done, "stream_chunks": chunks, "stream_errors": errors, "finish_reason": finish}
    return reply, facts


def body(model, prompt_parts, temperature, max_tokens, think, stream, **extra):
    content = [part() if callable(part) else part for part in prompt_parts] if isinstance(prompt_parts, list) else prompt_parts
    out = {"model": model, "messages": [{"role": "user", "content": content}], "temperature": temperature,
           "max_tokens": max_tokens, "think_mode": think, "stream": stream}
    out.update(extra)
    return out


def message_of(reply):
    return reply["choices"][0]["message"]


def reasoning_of(reply):
    return message_of(reply).get("reasoning_content") or ""


def no_markup(reply):
    text = (message_of(reply).get("content") or "") + reasoning_of(reply)
    return not any(m in (message_of(reply).get("content") or "") for m in CALL_MARKUP), text


def judge_weather_call(reply):
    """A get_weather call with JSON arguments holding a non-empty city, no call markup left in the answer."""
    ok, detail = probe.judge_tool(reply)
    clean, _ = no_markup(reply)
    detail["reasoning_len"] = len(reasoning_of(reply))
    return ok and clean, detail


def judge_red(reply):
    content = message_of(reply).get("content") or ""
    return "red" in content.lower() and not message_of(reply).get("tool_calls"), {
        "content": content, "reasoning_len": len(reasoning_of(reply)), "finish_reason": reply["choices"][0].get("finish_reason")}


def judge_schema(reply):
    ok, detail = probe.judge_json(reply, True)
    detail.update(reasoning_len=len(reasoning_of(reply)), tool_calls=message_of(reply).get("tool_calls"),
                  finish_reason=reply["choices"][0].get("finish_reason"))
    return ok and not message_of(reply).get("tool_calls"), detail


def judge_schema_or_weather(reply):
    weather_ok, detail = judge_weather_call(reply)
    json_ok, json_detail = probe.judge_json(reply, True)
    return weather_ok or json_ok, {**detail, **json_detail, "reasoning_len": len(reasoning_of(reply))}


def judge_plain(reply):
    content = (message_of(reply).get("content") or "").strip()
    return bool(content) and not message_of(reply).get("tool_calls") and "<think>" not in content, {
        "content": content, "finish_reason": reply["choices"][0].get("finish_reason")}


def judge_json_object(reply):
    ok, detail = probe.judge_json(reply, False)
    return ok, detail


def judge_yes_no(reply):
    return probe.judge_yes_no(reply)


def with_reasoning(judge):
    """Thinking on: the judge's check and a non-empty reasoning_content (the thinking text still appears)."""
    def judged(reply):
        ok, detail = judge(reply)
        return ok and bool(reasoning_of(reply).strip()), detail
    return judged


AUDIO_MODEL = "qwen3-asr-0.6b-gguf-Q8_0"
AUDIO_WAV = Path(__file__).resolve().parent / "weather_question.wav"


def audio_part():
    """The spoken question "What is the weather in Paris right now?" (Windows SAPI voice, 16 kHz mono WAV)."""
    import base64
    return {"type": "input_audio", "input_audio": {"data": base64.b64encode(AUDIO_WAV.read_bytes()).decode(), "format": "wav"}}


def judge_any_answer(reply):
    """Audio: the request is served (HTTP 200, a finish reason); the call or the text is recorded, not required."""
    message = message_of(reply)
    return reply["choices"][0].get("finish_reason") is not None, {
        "content": message.get("content"), "tool_calls": message.get("tool_calls"),
        "finish_reason": reply["choices"][0].get("finish_reason")}


TEXT_MODEL = probe.MODEL
PERSON = probe.PROMPT
WEATHER_Q = "What is the weather in Paris right now?"


def cases():
    vimg_tool = [IMAGE, TOOL_TEXT]
    vimg_red = [IMAGE, TEXT]
    # Vision model: think off cases at temperature 0 (run 68); think on at 0.6 (Qwen's thinking setting), with room to think
    v_off = dict(model=VISION, temperature=0, max_tokens=128, think="off")
    v_on = dict(model=VISION, temperature=0.6, max_tokens=2048, think="on")
    # Text model: run 65's settings; think on with room to think
    t_off = dict(model=TEXT_MODEL, temperature=0.8, max_tokens=200, think="off")
    t_on = dict(model=TEXT_MODEL, temperature=0.8, max_tokens=2048, think="on")
    weather = [WEATHER]
    out = {
        # 1. image + client tool, streamed
        "s1_img_tool_stream": (body(prompt_parts=vimg_tool, stream=True, tools=weather, **v_off), judge_weather_call),
        "s1_txt_tool_stream": (body(prompt_parts=[TOOL_TEXT], stream=True, tools=weather, **v_off), judge_weather_call),
        # 2. image + client tool, thinking on (Qwen3.5-2B)
        "t2_img_tool_think": (body(prompt_parts=vimg_tool, stream=False, tools=weather, **v_on), with_reasoning(judge_weather_call)),
        "t2_img_tool_think_stream": (body(prompt_parts=vimg_tool, stream=True, tools=weather, **v_on), with_reasoning(judge_weather_call)),
        "t2_img_tool_required_think": (body(prompt_parts=vimg_tool, stream=False, tools=weather, tool_choice="required", **v_on), with_reasoning(judge_weather_call)),
        "t2_img_red_think": (body(prompt_parts=vimg_red, stream=False, **v_on), with_reasoning(judge_red)),
        "t2_img_red_think_tools": (body(prompt_parts=vimg_red, stream=False, tools=weather, **v_on), with_reasoning(judge_red)),
        # 3. json_schema with thinking on, without and with a client tool (both models)
        "k3_schema_think": (body(prompt_parts=PERSON, stream=False, response_format=SCHEMA_RF, **t_on), with_reasoning(judge_schema)),
        "k3_schema_tool_think": (body(prompt_parts=PERSON, stream=False, response_format=SCHEMA_RF, tools=probe.WEATHER, **t_on), with_reasoning(judge_schema)),
        "k3_schema_tool_think_weatherq": (body(prompt_parts=WEATHER_Q, stream=False, response_format=SCHEMA_RF, tools=probe.WEATHER, **t_on), with_reasoning(judge_schema_or_weather)),
        "k3v_schema_think": (body(prompt_parts=PERSON, stream=False, response_format=SCHEMA_RF, **v_on), with_reasoning(judge_schema)),
        "k3v_schema_tool_think": (body(prompt_parts=PERSON, stream=False, response_format=SCHEMA_RF, tools=weather, **v_on), with_reasoning(judge_schema)),
        # json_object and GBNF with thinking on (text model): the same wake-after-thinking path
        "k3_json_object_think": (body(prompt_parts=PERSON, stream=False, response_format={"type": "json_object"}, **t_on), with_reasoning(judge_json_object)),
        "k3_gbnf_think": (body(prompt_parts="Is the sky blue on a clear day? Answer yes or no.", stream=False, grammar=probe.GBNF, **t_on), with_reasoning(judge_yes_no)),
        # 4. json_schema streamed
        "c4_schema_stream": (body(prompt_parts=PERSON, stream=True, response_format=SCHEMA_RF, **t_off), judge_schema),
        "c4_schema_tool_stream": (body(prompt_parts=PERSON, stream=True, response_format=SCHEMA_RF, tools=probe.WEATHER, **t_off), judge_schema),
        "c4_schema_think_stream": (body(prompt_parts=PERSON, stream=True, response_format=SCHEMA_RF, **t_on), with_reasoning(judge_schema)),
        "c4_schema_tool_think_stream": (body(prompt_parts=PERSON, stream=True, response_format=SCHEMA_RF, tools=probe.WEATHER, **t_on), with_reasoning(judge_schema)),
        # 5. audio + client tool (only an ASR model is on disk): do the schemas reach the prompt (prompt_n with and
        # without tools) and is the request served; a call is recorded, not required
        "a5_audio_tool": (body(model=AUDIO_MODEL, prompt_parts=[audio_part, {"type": "text", "text": "Answer the spoken question. Use the get_weather tool."}],
                               temperature=0, max_tokens=128, think="off", stream=False, tools=weather), judge_any_answer),
        "a5_audio_notool": (body(model=AUDIO_MODEL, prompt_parts=[audio_part, {"type": "text", "text": "Answer the spoken question. Use the get_weather tool."}],
                                 temperature=0, max_tokens=128, think="off", stream=False), judge_any_answer),
        # 6. text-only regression (run 65 settings)
        "r6_plain": (body(prompt_parts="Say hello to me in one short sentence.", stream=False, **t_off), judge_plain),
        "r6_tool_auto": (body(prompt_parts=WEATHER_Q, stream=False, tools=probe.WEATHER, **t_off), judge_weather_call),
        "r6_json_object": (body(prompt_parts=PERSON, stream=False, response_format={"type": "json_object"}, **t_off), judge_json_object),
        "r6_gbnf_yes_no": (body(prompt_parts="Is the sky blue on a clear day? Answer yes or no.", stream=False, grammar=probe.GBNF, **t_off), judge_yes_no),
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
    if args.port in (8000, 8011, 8014) or args.home == Path.home() / ".cluaiz":
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
            req, judge = all_cases[name]
            stream = bool(req.get("stream"))
            # Warm-up per model (load), not counted
            engine.request("POST", "/v1/chat/completions", {"model": req["model"], "messages": [{"role": "user", "content": "hi"}],
                                                            "max_tokens": 4, "think_mode": "off"}, timeout=900, record=False)
            valid, times, constrained, reasoned, finishes = 0, [], 0, 0, {}
            for i in range(args.n):
                time.sleep(0.3)
                start = log_path.stat().st_size if log_path.exists() else 0
                started = time.perf_counter()
                status, reply = engine.request("POST", "/v1/chat/completions", req, timeout=900, record=False, stream=stream)
                elapsed = time.perf_counter() - started
                time.sleep(0.5)
                with log_path.open("r", encoding="utf-8", errors="replace") as f:
                    f.seek(start)
                    lines = [l.rstrip() for l in f if any(m in l for m in LOG_MARKERS)]
                facts = {}
                if stream and status == 200 and isinstance(reply, str):
                    reply, facts = parse_sse(reply)
                if status == 200 and isinstance(reply, dict) and reply.get("choices"):
                    ok, detail = judge(reply)
                    if stream:
                        ok = ok and facts["stream_done"] and bool(facts["finish_reason"]) and not facts["stream_errors"]
                    reasoning = reasoning_of(reply)
                else:
                    ok, detail, reasoning = False, {"status": status, "reply": reply if not isinstance(reply, str) else reply[:2000]}, ""
                is_constrained = constrained_in(lines)
                constrained += is_constrained
                timings = ((reply.get("engine") or {}).get("timings") or {}) if isinstance(reply, dict) else {}
                prompt_tokens = [l.split("prompt ")[1].split(" tokens")[0] for l in lines if "llama.cpp timings: prompt " in l]
                reasoned += bool(reasoning.strip())
                fr = reply["choices"][0].get("finish_reason") if isinstance(reply, dict) and reply.get("choices") else None
                finishes[str(fr)] = finishes.get(str(fr), 0) + 1
                valid += ok
                times.append(round(elapsed, 3))
                journal.write("results.jsonl", {"case": name, "i": i, "status": status, "valid": ok, "elapsed_s": round(elapsed, 3),
                                                "constrained_log": is_constrained, "reasoning_len": len(reasoning),
                                                "prompt_n": timings.get("prompt_n"), "prompt_tokens_log": prompt_tokens, "finish_reason": fr,
                                                "reasoning_head": reasoning[:160], "log": lines, **facts,
                                                "request": strip_media(req) if i == 0 else None, **detail})
                print(name, i, status, "VALID" if ok else "INVALID", f"constrained={is_constrained}", f"reasoning={len(reasoning)}",
                      f"finish={fr}", json.dumps(detail, ensure_ascii=False)[:220], flush=True)
            summary[name] = {"valid": valid, "n": args.n, "constrained_log": constrained, "reasoning_nonempty": reasoned,
                             "finish_reasons": finishes, "median_s": sorted(times)[len(times) // 2]}
            journal.write("summary.jsonl", {"case": name, "exe": str(args.exe.parent.name), **summary[name]})
    finally:
        engine.stop()
    print(json.dumps(summary, indent=1))


if __name__ == "__main__":
    main()
