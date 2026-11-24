"""Before/after probe for the run 57 fixes: Ollama /api/generate, the run 52 long prompt, chat tools.

python -B probe.py --exe <cluaiz.exe> --home <isolated-home> --port 8014 --label <name>

Writes raw/<label>/ (engine log, requests, responses) and raw/<label>/results.jsonl.
"""
import argparse
import json
from pathlib import Path
import sys

REPO = Path(__file__).resolve().parents[4]
sys.path.insert(0, str(REPO / "test" / "release_soak"))
from soak import Engine, Journal  # noqa: E402

MODEL = "qwen3.5-0.8b-gguf-Q4_K_M"
WEATHER = [{"type": "function", "function": {"name": "get_weather", "description": "Current weather of a city",
            "parameters": {"type": "object", "properties": {"city": {"type": "string"}}, "required": ["city"]}}}]


def main():
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--exe", type=Path, required=True)
    parser.add_argument("--home", type=Path, required=True)
    parser.add_argument("--port", type=int, default=8014)
    parser.add_argument("--label", required=True)
    args = parser.parse_args()
    args.exe, args.home = args.exe.resolve(), args.home.resolve()
    if args.port in (8000, 8011) or args.home == Path.home() / ".cluaiz":
        parser.error("use an isolated home and port")
    folder = Path(__file__).resolve().parent / "raw" / args.label
    journal = Journal(folder, args.home)
    engine = Engine(args, journal)
    results = []

    def record(name, ok, detail):
        results.append({"check": name, "pass": bool(ok), **detail})
        journal.write("results.jsonl", results[-1])
        print(name, "PASS" if ok else "FAIL", json.dumps(detail)[:300])

    try:
        engine.start()
        # 1. The run 53 check, unchanged (release_e2e.py `ollama_generate`)
        body = {"model": MODEL, "prompt": "The capital of France is", "stream": False, "think": False,
                "options": {"temperature": 0, "num_predict": 16}}
        status, reply = engine.request("POST", "/api/generate", body)
        text = reply.get("response", "") if isinstance(reply, dict) else ""
        record("ollama_generate", status == 200 and "paris" in text.lower(),
               {"status": status, "response": text, "prompt_eval_count": reply.get("prompt_eval_count") if isinstance(reply, dict) else None})

        # 2. The same request streamed (NDJSON): the last object carries the counts
        status, raw = engine.request("POST", "/api/generate", dict(body, stream=True), stream=True)
        lines = [json.loads(line) for line in raw.splitlines() if line.strip().startswith("{")] if status == 200 else []
        streamed = "".join(item.get("response", "") for item in lines)
        record("ollama_generate_stream", status == 200 and "paris" in streamed.lower(),
               {"status": status, "response": streamed, "prompt_eval_count": lines[-1].get("prompt_eval_count") if lines else None})

        # 3. What a plain /v1/chat/completions request carries in this home (project files, tools)
        body = {"model": MODEL, "messages": [{"role": "user", "content": "The capital of France is"}],
                "temperature": 0, "max_tokens": 16, "think_mode": "off", "return_prompt": True}
        status, reply = engine.request("POST", "/v1/chat/completions", body)
        report = reply.get("context_report", {}) if isinstance(reply, dict) else {}
        plan = report.get("plan", {}) if isinstance(report, dict) else {}
        project = plan.get("project") if isinstance(plan, dict) else None
        record("chat_plain_project", status == 200,
               {"status": status, "content": reply["choices"][0]["message"].get("content") if status == 200 else None,
                "prompt_tokens": reply.get("usage", {}).get("prompt_tokens") if isinstance(reply, dict) else None,
                "project_root": project.get("root") if isinstance(project, dict) else None,
                "project_chars": project.get("chars") if isinstance(project, dict) else None,
                "tools": report.get("tools") if isinstance(report, dict) else None})

        # 4. The run 52 long_input scenario, unchanged (soak.py)
        body = {"model": MODEL, "messages": [{"role": "user", "content": "The marker is 4917. " + "word " * 1500 + " What is the marker?"}],
                "temperature": 0, "max_tokens": 48, "think_mode": "off"}
        status, reply = engine.request("POST", "/v1/chat/completions", body, timeout=600)
        message = reply["choices"][0]["message"] if status == 200 else {}
        record("long_input", status == 200 and "4917" in json.dumps(reply),
               {"status": status, "content": message.get("content"), "reasoning_content": message.get("reasoning_content"),
                "finish_reason": reply["choices"][0].get("finish_reason") if status == 200 else None,
                "prompt_tokens": reply.get("usage", {}).get("prompt_tokens") if isinstance(reply, dict) else None,
                "tool_events": len((reply.get("context_report", {}).get("tools") or {}).get("events", [])) if isinstance(reply, dict) else None})

        # 5. A client tool on /v1/chat/completions still comes back as OpenAI tool_calls (run 49 chat_tools)
        body = {"model": MODEL, "messages": [{"role": "user", "content": "What is the weather in Paris right now? Use the tool."}],
                "tools": WEATHER, "max_tokens": 512}
        status, reply = engine.request("POST", "/v1/chat/completions", body, timeout=600)
        calls = reply["choices"][0]["message"].get("tool_calls") if status == 200 else None
        name = calls[0]["function"]["name"] if calls else None
        arguments = calls[0]["function"].get("arguments") if calls else None
        record("chat_tools", status == 200 and name == "get_weather" and "paris" in (arguments or "").lower(),
               {"status": status, "call": name, "arguments": arguments,
                "finish_reason": reply["choices"][0].get("finish_reason") if status == 200 else None})
    finally:
        engine.stop()
    passed = sum(item["pass"] for item in results)
    print(f"{args.label}: {passed}/{len(results)} passed")
    return 0


if __name__ == "__main__":
    sys.exit(main())
