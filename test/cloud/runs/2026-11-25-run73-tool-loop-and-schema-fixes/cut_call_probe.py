"""Half tool call at max_tokens probe.

Starts one isolated `cluaiz serve` (CLUAIZ_HOME, port, 127.0.0.1; `Server` from run 70's loop_probe.py,
copied next to this file), offers one client function with a long text argument, asks the model to call
it with a long text and sets a small `max_tokens`, so generation is cut inside the `<tool_call>` block.
Records per request: HTTP status, `content`, whether it holds `<tool_call>`, `finish_reason`, client
`tool_calls`, and the engine log lines of the request.

python -B cut_call_probe.py --exe <cluaiz.exe> --home <home> --port 8013 --out <dir>
       --model qwen3.5-0.8b-gguf-Q4_K_M [--stream] [--n 5] [--max-tokens 24]
"""
import argparse
import json
import time
from pathlib import Path

from loop_probe import Server, streamed_reply

TOOL = {"type": "function", "function": {
    "name": "save_note", "description": "Saves a note with the given text.",
    "parameters": {"type": "object", "properties": {"text": {"type": "string", "description": "The full note text"}},
                   "required": ["text"]}}}
TOPICS = ["a lighthouse keeper", "a lost cat", "a rainy market day", "an old clock maker", "a first snowfall",
          "a river boat", "a mountain village", "a night train", "a small bakery", "a winter garden"]


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("--exe", type=Path, required=True)
    p.add_argument("--home", type=Path, required=True)
    p.add_argument("--port", type=int, default=8013)
    p.add_argument("--out", type=Path, required=True)
    p.add_argument("--model", required=True)
    p.add_argument("--n", type=int, default=5)
    p.add_argument("--max-tokens", type=int, default=24)
    p.add_argument("--stream", action="store_true")
    p.add_argument("--label", default="")
    args = p.parse_args()
    args.out.mkdir(parents=True, exist_ok=True)
    server = Server(args.exe, args.home, args.port, args.out / "engine.log")
    server.start()
    rows = []
    try:
        for i in range(args.n):
            text = f"Call the save_note tool now. Its text argument is a 200-word story about {TOPICS[i % len(TOPICS)]}. Write nothing outside the tool call."
            body = {"model": args.model, "messages": [{"role": "user", "content": text}], "tools": [TOOL],
                    "temperature": 0, "max_tokens": args.max_tokens, "think_mode": "off", "stream": args.stream}
            at = server.mark()
            started = time.perf_counter()
            status, reply = server.request("POST", "/v1/chat/completions", body, stream=args.stream)
            seconds = time.perf_counter() - started
            time.sleep(1.0)
            lines = server.since(at)
            if args.stream:
                message = streamed_reply(reply) if status == 200 else {"content": "", "error": reply}
                calls = message.get("client_tool_calls") or []
                finish = message.get("finish_reason")
                saved = message
            else:
                choice = (reply.get("choices") or [{}])[0] if isinstance(reply, dict) else {}
                message = choice.get("message") or {}
                calls = message.get("tool_calls") or []
                finish = choice.get("finish_reason")
                saved = reply
            content = message.get("content") or ""
            row = {"label": args.label, "i": i, "stream": args.stream, "status": status, "seconds": round(seconds, 2),
                   "content": content, "half_call_in_content": "<tool_call>" in content,
                   "finish_reason": finish, "client_tool_calls": calls,
                   "reasoning_content": message.get("reasoning_content") or ""}
            rows.append(row)
            print(json.dumps({k: row[k] for k in ("i", "status", "half_call_in_content", "finish_reason")}),
                  "|", repr(content[:120]), flush=True)
            with open(args.out / "results.jsonl", "a", encoding="utf-8") as f:
                f.write(json.dumps(row) + "\n")
            with open(args.out / "replies.jsonl", "a", encoding="utf-8") as f:
                f.write(json.dumps({"i": i, "reply": saved}) + "\n")
            with open(args.out / "request_logs.jsonl", "a", encoding="utf-8") as f:
                f.write(json.dumps({"i": i, "lines": lines}) + "\n")
    finally:
        server.stop()
    summary = {"label": args.label, "n": len(rows), "stream": args.stream,
               "half_call_in_content": sum(r["half_call_in_content"] for r in rows),
               "finish_length": sum(r["finish_reason"] == "length" for r in rows),
               "client_tool_calls": sum(bool(r["client_tool_calls"]) for r in rows)}
    print("SUMMARY", json.dumps(summary), flush=True)
    with open(args.out / "summary.jsonl", "a", encoding="utf-8") as f:
        f.write(json.dumps(summary) + "\n")


if __name__ == "__main__":
    main()
