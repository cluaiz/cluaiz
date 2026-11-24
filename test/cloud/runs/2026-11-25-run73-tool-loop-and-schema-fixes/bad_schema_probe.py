"""A json_schema llama.cpp cannot turn into a grammar (a $ref that resolves nowhere), no tools: is the answer parser
(llama.cpp's template call for the answer) kept?

Starts one isolated `cluaiz serve` (Server from run 70's loop_probe.py). `tool_choice: "none"` keeps the Engine's
own functions out, so the request takes the no-tools answer-parser path in native/stream.rs. Records status,
content, reasoning, and the log lines that show the parser ([NativeTools] Format ..., [NativeTemplate] ...,
schema warnings).

python -B bad_schema_probe.py --exe <cluaiz.exe> --home <home> --port 8013 --out <dir> [--n 3] [--think on|off]
"""
import argparse
import json
import time
from pathlib import Path

from loop_probe import Server

SCHEMA = {"type": "json_schema", "json_schema": {"name": "broken", "schema": {
    "type": "object", "properties": {"name": {"$ref": "#/definitions/missing"}}, "required": ["name"]}}}
MARKERS = ("[NativeTools] Format", "[NativeTemplate]", "JSON schema", "Answer constrained", "not constrained", "schema")


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("--exe", type=Path, required=True)
    p.add_argument("--home", type=Path, required=True)
    p.add_argument("--port", type=int, default=8013)
    p.add_argument("--out", type=Path, required=True)
    p.add_argument("--model", default="qwen3-0.6b-gguf-Q4_K_M")
    p.add_argument("--n", type=int, default=3)
    p.add_argument("--think", default="on")
    p.add_argument("--label", default="")
    args = p.parse_args()
    args.out.mkdir(parents=True, exist_ok=True)
    server = Server(args.exe, args.home, args.port, args.out / "engine.log")
    server.start()
    try:
        for i in range(args.n):
            body = {"model": args.model, "messages": [{"role": "user", "content": "Give me a JSON object with the name of a fictional person."}],
                    "response_format": SCHEMA, "tool_choice": "none", "temperature": 0.8, "max_tokens": 1024,
                    "think_mode": args.think, "stream": False}
            at = server.mark()
            status, reply = server.request("POST", "/v1/chat/completions", body)
            time.sleep(0.5)
            lines = server.since(at)
            choice = (reply.get("choices") or [{}])[0] if isinstance(reply, dict) else {}
            message = choice.get("message") or {}
            row = {"label": args.label, "i": i, "status": status, "content": message.get("content"),
                   "reasoning_len": len(message.get("reasoning_content") or ""), "finish_reason": choice.get("finish_reason"),
                   "parser_line": any("[NativeTools] Format" in l for l in lines),
                   "log": [l[l.find("["):][:200] for l in lines if any(m in l for m in MARKERS)],
                   "error": reply.get("error") if isinstance(reply, dict) else reply}
            print(json.dumps({k: row[k] for k in ("i", "status", "parser_line", "reasoning_len", "finish_reason")}),
                  "|", repr((row["content"] or "")[:80]), flush=True)
            with open(args.out / "results.jsonl", "a", encoding="utf-8") as f:
                f.write(json.dumps(row) + "\n")
    finally:
        server.stop()


if __name__ == "__main__":
    main()
