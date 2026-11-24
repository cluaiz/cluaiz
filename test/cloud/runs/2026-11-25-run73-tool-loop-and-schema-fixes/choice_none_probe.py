"""tool_choice "none" with a client function: is a call the model writes anyway returned as a client call?

Starts one isolated `cluaiz serve` (Server from run 70's loop_probe.py). Each request offers `get_weather`, sets
`tool_choice: "none"` and asks a weather question that names the tool. Records status, content, reasoning,
client `tool_calls`, finish_reason and the NativeTools log lines.

python -B choice_none_probe.py --exe <cluaiz.exe> --home <home> --port 8013 --out <dir> [--model M] [--n 10] [--stream]
"""
import argparse
import json
import time
from pathlib import Path

from loop_probe import Server, streamed_reply

WEATHER = {"type": "function", "function": {"name": "get_weather", "description": "Current weather of a city",
                                            "parameters": {"type": "object", "properties": {"city": {"type": "string"}},
                                                           "required": ["city"]}}}


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("--exe", type=Path, required=True)
    p.add_argument("--home", type=Path, required=True)
    p.add_argument("--port", type=int, default=8013)
    p.add_argument("--out", type=Path, required=True)
    p.add_argument("--model", default="qwen3-0.6b-gguf-Q4_K_M")
    p.add_argument("--n", type=int, default=10)
    p.add_argument("--stream", action="store_true")
    p.add_argument("--label", default="")
    args = p.parse_args()
    args.out.mkdir(parents=True, exist_ok=True)
    server = Server(args.exe, args.home, args.port, args.out / "engine.log")
    server.start()
    rows = []
    try:
        for i in range(args.n):
            body = {"model": args.model, "messages": [{"role": "user", "content": "What is the weather in Paris right now? Use the get_weather tool."}],
                    "tools": [WEATHER], "tool_choice": "none", "temperature": 0.8, "max_tokens": 200, "think_mode": "off",
                    "stream": args.stream}
            at = server.mark()
            status, reply = server.request("POST", "/v1/chat/completions", body, stream=args.stream)
            time.sleep(0.5)
            lines = server.since(at)
            if args.stream:
                message = streamed_reply(reply) if status == 200 else {"content": "", "error": reply}
                calls, finish = message.get("client_tool_calls") or [], message.get("finish_reason")
            else:
                choice = (reply.get("choices") or [{}])[0] if isinstance(reply, dict) else {}
                message = choice.get("message") or {}
                calls, finish = message.get("tool_calls") or [], choice.get("finish_reason")
            content = message.get("content") or ""
            row = {"label": args.label, "i": i, "stream": args.stream, "status": status, "client_tool_calls": calls,
                   "finish_reason": finish, "content": content, "reasoning_content": message.get("reasoning_content") or "",
                   "call_text_in_content": "<tool_call>" in content,
                   "log": [l[l.find("["):][:140] for l in lines if "[NativeTools]" in l or "[API] Tool call" in l]}
            rows.append(row)
            print(json.dumps({k: row[k] for k in ("i", "status", "finish_reason", "call_text_in_content")}),
                  "calls", len(calls), "|", repr(content[:90]), flush=True)
            with open(args.out / "results.jsonl", "a", encoding="utf-8") as f:
                f.write(json.dumps(row) + "\n")
    finally:
        server.stop()
    summary = {"label": args.label, "n": len(rows), "client_calls": sum(bool(r["client_tool_calls"]) for r in rows),
               "call_text_in_content": sum(r["call_text_in_content"] for r in rows),
               "call_text_in_reasoning": sum("<tool_call>" in r["reasoning_content"] for r in rows)}
    print("SUMMARY", json.dumps(summary), flush=True)
    with open(args.out / "summary.jsonl", "a", encoding="utf-8") as f:
        f.write(json.dumps(summary) + "\n")


if __name__ == "__main__":
    main()
