"""Run 65 probe: do `response_format`, `grammar` and function tools constrain a small local GGUF model?

python -B probe.py --exe <cluaiz.exe> --home <isolated-home> --port 8013 --out <dir> [--cases a,b,c,d,e,...]

Each case sends N requests at temperature 0.8 to /v1/chat/completions and counts valid outputs.
Cases ending in "_none" repeat a case with `tool_choice: "none"`, which removes the Engine's own
functions from the prompt (api/src/handlers/chat/preflight.rs:692). Cases f and g send the constraint
together with one client function. Results go to <out>/results.jsonl and <out>/summary.jsonl
(appended, so a second call with --cases adds to the same files).
"""
import argparse
import json
import sys
import time
from pathlib import Path

REPO = Path(__file__).resolve().parents[4]
sys.path.insert(0, str(REPO / "test" / "release_soak"))
from soak import Engine, Journal  # noqa: E402

MODEL = "qwen3-0.6b-gguf-Q4_K_M"
PROMPT = "Give me a JSON object with name and age of a fictional person"
SCHEMA = {"type": "object", "properties": {"name": {"type": "string"}, "age": {"type": "integer"}},
          "required": ["name", "age"], "additionalProperties": False}
GBNF = 'root ::= "yes" | "no"'
WEATHER = [{"type": "function", "function": {"name": "get_weather", "description": "Current weather of a city",
            "parameters": {"type": "object", "properties": {"city": {"type": "string"}}, "required": ["city"]}}}]


def base(prompt=PROMPT):
    return {"model": MODEL, "messages": [{"role": "user", "content": prompt}], "temperature": 0.8,
            "max_tokens": 200, "think_mode": "off", "stream": False}


def parse_json(text):
    try:
        return True, json.loads(text.strip())
    except (json.JSONDecodeError, AttributeError):
        return False, None


def matches_schema(value):
    return (isinstance(value, dict) and set(value) == {"name", "age"} and isinstance(value["name"], str)
            and isinstance(value["age"], int) and not isinstance(value["age"], bool))


def judge_json(reply, schema):
    content = reply["choices"][0]["message"].get("content") or ""
    ok, value = parse_json(content)
    valid = ok and (matches_schema(value) if schema else isinstance(value, dict))
    return valid, {"content": content}


def judge_yes_no(reply):
    content = reply["choices"][0]["message"].get("content") or ""
    return content in ("yes", "no"), {"content": content}


def judge_tool(reply):
    message = reply["choices"][0]["message"]
    calls = message.get("tool_calls") or []
    detail = {"content": message.get("content"), "tool_calls": calls,
              "finish_reason": reply["choices"][0].get("finish_reason")}
    if len(calls) < 1:
        return False, detail
    fn = calls[0].get("function", {})
    raw = fn.get("arguments")
    args = raw if isinstance(raw, dict) else parse_json(raw)[1] if isinstance(raw, str) else None
    ok = fn.get("name") == "get_weather" and isinstance(args, dict) and isinstance(args.get("city"), str) and args["city"].strip() != ""
    return ok, detail


def cases():
    yes_no = base("Is the sky blue on a clear day? Answer yes or no.")
    weather = dict(base("What is the weather in Paris right now?"), tools=WEATHER, tool_choice="required")
    out = {
        "a_plain": (base(), lambda r: judge_json(r, False)),
        "b_json_object": (dict(base(), response_format={"type": "json_object"}), lambda r: judge_json(r, False)),
        "c_json_schema": (dict(base(), response_format={"type": "json_schema", "json_schema": {"name": "person", "strict": True, "schema": SCHEMA}}),
                          lambda r: judge_json(r, True)),
        "d_grammar": (dict(yes_no, grammar=GBNF), judge_yes_no),
        "e_tool_required": (weather, judge_tool),
    }
    for key in ("b_json_object", "c_json_schema", "d_grammar"):
        body, judge = out[key]
        out[key + "_none"] = (dict(body, tool_choice="none"), judge)
    # The same constraints while a function is offered (tool_choice auto): the path an install with
    # tools takes, where the Engine's own functions are always in the prompt
    out["f_json_schema_with_tool"] = (dict(out["c_json_schema"][0], tools=WEATHER), lambda r: judge_json(r, True))
    out["g_grammar_with_tool"] = (dict(yes_no, grammar=GBNF, tools=WEATHER), judge_yes_no)
    # tool_choice "required" with a prompt that does not need the tool: is a call forced?
    out["h_required_unrelated"] = (dict(base("Say hello to me in one short sentence."), tools=WEATHER, tool_choice="required"), judge_tool)
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
    journal = Journal(args.out, args.home)
    engine = Engine(args, journal)
    all_cases = cases()
    wanted = [c for c in args.cases.split(",") if c] or list(all_cases)
    summary = {}
    try:
        engine.start()
        # Warm-up load, not counted
        engine.request("POST", "/v1/chat/completions", dict(base("hi"), max_tokens=8), timeout=600)
        for name in wanted:
            body, judge = all_cases[name]
            valid, times, tokens = 0, [], []
            for i in range(args.n):
                started = time.perf_counter()
                status, reply = engine.request("POST", "/v1/chat/completions", body, timeout=600)
                elapsed = time.perf_counter() - started
                if status == 200 and isinstance(reply, dict) and reply.get("choices"):
                    ok, detail = judge(reply)
                    tokens.append((reply.get("usage") or {}).get("completion_tokens"))
                else:
                    ok, detail = False, {"status": status, "reply": reply}
                valid += ok
                times.append(round(elapsed, 3))
                journal.write("results.jsonl", {"case": name, "i": i, "valid": ok, "elapsed_s": round(elapsed, 3),
                                                "completion_tokens": tokens[-1] if tokens else None, **detail})
                print(name, i, "VALID" if ok else "INVALID", json.dumps(detail, ensure_ascii=False)[:300])
            summary[name] = {"valid": valid, "n": args.n, "median_s": sorted(times)[len(times) // 2],
                             "completion_tokens": tokens}
            journal.write("summary.jsonl", {"case": name, **summary[name]})
    finally:
        engine.stop()
    print(json.dumps(summary, indent=1))


if __name__ == "__main__":
    main()
