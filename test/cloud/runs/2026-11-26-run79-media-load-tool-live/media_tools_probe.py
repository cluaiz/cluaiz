"""Image requests with the Engine's own tools: tool_choice "required" on tool steps, and the final answer at
the step limit.

Starts one isolated `cluaiz serve` (Server from run 70's loop_probe.py), sends an image (run 64's red square)
with a math question and records per request: the tool steps (`Step N: generating again`), Engine tool calls
run, whether the step / repeat limit and the final no-tools turn happened, the NativeTools log lines of each
generation (grammar / format / parsed calls), and the reply.

python -B media_tools_probe.py --exe <cluaiz.exe> --home <home> --port 8013 --out <dir> --case required|final [--n 3]
       [--small-model-tools lean|full]
  required: tool_choice "required", max_steps default: after the Engine ran the math call, later steps
            should not be forced to call again.
  final:    max_steps 1, no tool_choice: the turn reaches the step limit after one call; the final answer
            should get no tool schemas.
  --small-model-tools: sent as `context_policy.small_model_tools` (run 79 added it; omitted = the Engine
            default, "lean" for models under 2 billion weights; run 73 ran before that rule existed).
Run 79 copy: the model can be set with the PROBE_MODEL environment variable.
"""
import argparse
import json
import os
import sys
import time
from pathlib import Path

from loop_probe import Server

RUNS = Path(os.environ["PROBE_RUNS"])
sys.path.insert(0, str(RUNS / "2026-11-24-run64-vision-hint-layouts"))
from vision_hint import IMAGE  # noqa: E402  (run 64's red PNG)

MODEL = os.environ.get("PROBE_MODEL", "qwen3.5-2b-mtp-gguf-Q4_K_M")
QUESTIONS = ["What is 23 * 19? Use the math tool.", "What is 47 * 12? Use the math tool.", "What is 315 / 7? Use the math tool."]


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("--exe", type=Path, required=True)
    p.add_argument("--home", type=Path, required=True)
    p.add_argument("--port", type=int, default=8013)
    p.add_argument("--out", type=Path, required=True)
    p.add_argument("--case", choices=["required", "final"], required=True)
    p.add_argument("--n", type=int, default=3)
    p.add_argument("--label", default="")
    p.add_argument("--small-model-tools", choices=["lean", "full"], default=None)
    args = p.parse_args()
    args.out.mkdir(parents=True, exist_ok=True)
    server = Server(args.exe, args.home, args.port, args.out / "engine.log")
    server.start()
    try:
        for i in range(args.n):
            body = {"model": MODEL, "messages": [{"role": "user", "content": [IMAGE, {"type": "text", "text": QUESTIONS[i % 3]}]}],
                    "temperature": 0, "max_tokens": 256, "think_mode": "off", "stream": False}
            if args.small_model_tools:
                body["context_policy"] = {"small_model_tools": args.small_model_tools}
            if args.case == "required":
                body["tool_choice"] = "required"
            else:
                body["max_steps"] = 1
            at = server.mark()
            started = time.perf_counter()
            status, reply = server.request("POST", "/v1/chat/completions", body)
            seconds = time.perf_counter() - started
            time.sleep(1.0)
            lines = server.since(at)
            choice = (reply.get("choices") or [{}])[0] if isinstance(reply, dict) else {}
            message = choice.get("message") or {}
            row = {"label": args.label, "case": args.case, "i": i, "status": status, "seconds": round(seconds, 2),
                   "steps": sum("generating again with the tool result" in l for l in lines),
                   "tool_calls_run": sum("[API] Tool call:" in l for l in lines),
                   "step_limit": any("stopped at step/repeat limit" in l for l in lines),
                   "final_no_tools_turn": any("asking once more for a final answer without tools" in l for l in lines),
                   "native_tools_lines": [l[l.find("[NativeTools]"):][:120] for l in lines if "[NativeTools]" in l or "memory check" in l.lower()],
                   "content": (message.get("content") or "")[:300], "finish_reason": choice.get("finish_reason"),
                   "error": reply.get("error") if isinstance(reply, dict) else reply}
            print(json.dumps({k: row[k] for k in ("i", "status", "seconds", "steps", "tool_calls_run", "step_limit",
                                                   "final_no_tools_turn", "finish_reason")}), "|", row["content"][:80].replace("\n", " "), flush=True)
            with open(args.out / "results.jsonl", "a", encoding="utf-8") as f:
                f.write(json.dumps(row) + "\n")
            with open(args.out / "request_logs.jsonl", "a", encoding="utf-8") as f:
                f.write(json.dumps({"i": i, "lines": lines}) + "\n")
    finally:
        server.stop()


if __name__ == "__main__":
    main()
