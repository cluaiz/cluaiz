"""Run 62 probe: what the Engine adds to Ollama /api/chat, before and after; client tools; /v1 unchanged.

python -B probe.py --exe <cluaiz.exe> --home <isolated-home> --port 8013 --label <name> --out <dir>
"""
import argparse
import json
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parents[4]
sys.path.insert(0, str(REPO / "test" / "release_soak"))
from soak import Engine, Journal  # noqa: E402

MODEL = "qwen3.5-0.8b-gguf-Q4_K_M"
WEATHER = [{"type": "function", "function": {"name": "get_weather", "description": "Current weather of a city",
            "parameters": {"type": "object", "properties": {"city": {"type": "string"}}, "required": ["city"]}}}]


def main():
    p = argparse.ArgumentParser()
    p.add_argument("--exe", type=Path, required=True)
    p.add_argument("--home", type=Path, required=True)
    p.add_argument("--port", type=int, default=8013)
    p.add_argument("--label", required=True)
    p.add_argument("--out", type=Path, required=True)
    args = p.parse_args()
    args.exe, args.home = args.exe.resolve(), args.home.resolve()
    if args.port in (8000, 8011) or args.home == Path.home() / ".cluaiz":
        p.error("use an isolated home and port")
    journal = Journal(args.out / args.label, args.home)
    engine = Engine(args, journal)
    results = []

    def record(name, ok, detail):
        results.append({"check": name, "pass": bool(ok), **detail})
        journal.write("results.jsonl", results[-1])
        print(name, "PASS" if ok else "FAIL", json.dumps(detail)[:600])

    try:
        engine.start()
        # 1. Ollama /api/chat, "hi", no tools (non-streaming)
        body = {"model": MODEL, "messages": [{"role": "user", "content": "hi"}], "stream": False, "think": False,
                "options": {"temperature": 0, "num_predict": 32}}
        status, reply = engine.request("POST", "/api/chat", body)
        ok = isinstance(reply, dict)
        record("ollama_chat_hi", status == 200,
               {"status": status, "content": reply.get("message", {}).get("content") if ok else reply,
                "prompt_eval_count": reply.get("prompt_eval_count") if ok else None})

        # 2. Same, streamed (Ollama's default)
        status, raw = engine.request("POST", "/api/chat", dict(body, stream=True), stream=True)
        lines = [json.loads(l) for l in raw.splitlines() if l.strip().startswith("{")] if status == 200 else []
        record("ollama_chat_hi_stream", status == 200,
               {"status": status, "content": "".join(i.get("message", {}).get("content", "") for i in lines),
                "prompt_eval_count": lines[-1].get("prompt_eval_count") if lines else None})

        # 3. /api/chat with a client tool: the call comes back in message.tool_calls
        body = {"model": MODEL, "messages": [{"role": "user", "content": "What is the weather in Paris right now? Use the tool."}],
                "tools": WEATHER, "stream": False, "options": {"num_predict": 512}}
        status, reply = engine.request("POST", "/api/chat", body, timeout=600)
        calls = reply.get("message", {}).get("tool_calls") if isinstance(reply, dict) else None
        name = calls[0]["function"]["name"] if calls else None
        args_ = calls[0]["function"].get("arguments") if calls else None
        record("ollama_chat_tools", status == 200 and name == "get_weather" and "paris" in json.dumps(args_).lower(),
               {"status": status, "call": name, "arguments": args_,
                "prompt_eval_count": reply.get("prompt_eval_count") if isinstance(reply, dict) else None})

        # 4. /v1/chat/completions "hi" with return_prompt: unchanged OpenAI route (the Engine additions stay)
        body = {"model": MODEL, "messages": [{"role": "user", "content": "hi"}], "temperature": 0, "max_tokens": 32,
                "think_mode": "off", "return_prompt": True}
        status, reply = engine.request("POST", "/v1/chat/completions", body)
        report = reply.get("context_report", {}) if isinstance(reply, dict) else {}
        tools = report.get("tools") if isinstance(report, dict) else None
        record("v1_chat_hi", status == 200,
               {"status": status, "prompt_tokens": reply.get("usage", {}).get("prompt_tokens") if isinstance(reply, dict) else None,
                "tools": {k: tools.get(k) for k in ("meta_tools", "hint_tokens", "system_tools_tokens", "template_tools_tokens", "format")} if isinstance(tools, dict) else None})
        journal.write("v1_hi_rendered_prompt.txt", (reply.get("engine") or {}).get("context", {}).get("prompt") if isinstance(reply, dict) else None)

        # 5. /v1/chat/completions with the client tool: unchanged
        body = {"model": MODEL, "messages": [{"role": "user", "content": "What is the weather in Paris right now? Use the tool."}],
                "tools": WEATHER, "max_tokens": 512}
        status, reply = engine.request("POST", "/v1/chat/completions", body, timeout=600)
        calls = reply["choices"][0]["message"].get("tool_calls") if status == 200 else None
        record("v1_chat_tools", status == 200 and calls and calls[0]["function"]["name"] == "get_weather",
               {"status": status, "call": calls[0]["function"]["name"] if calls else None,
                "arguments": calls[0]["function"].get("arguments") if calls else None,
                "prompt_tokens": reply.get("usage", {}).get("prompt_tokens") if isinstance(reply, dict) else None})
    finally:
        engine.stop()
    print(f"{args.label}: {sum(r['pass'] for r in results)}/{len(results)} passed")


if __name__ == "__main__":
    main()
