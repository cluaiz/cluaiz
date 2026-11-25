"""max_tokens budget probe (run 78).

Starts one isolated `cluaiz serve` (run 73's Server class: CLUAIZ_HOME, port, 127.0.0.1) and sends a prompt that
makes the model call the Engine's math tool, in four cases, n times each:
  limit48  max_tokens 48            both  max_tokens 48 + max_completion_tokens 5 (soak.py's pair)
  nolimit  no limit field           stream  max_tokens 48, stream true
Records completion_tokens, finish_reason, content length and the tool steps the Engine ran (log lines
`Step N: generating again`) per request into <out>/<label>.jsonl.

python -B budget_probe.py --exe <cluaiz.exe> --home <home> --port 8013 --out <dir> --label before
       --model qwen3-0.6b-gguf-Q4_K_M --n 5 [--think-off]
The session token is read from the home and never written to the output.
"""
import argparse, json, sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent.parent / "2026-11-25-run73-tool-loop-and-schema-fixes"))
from loop_probe import Server, streamed_reply  # noqa: E402

PROMPT = ("Use the math tool for each step: compute 1234 * 56, then 789 * 12, then 4321 + 987, "
          "then explain every result in a long paragraph.")
CASES = {"limit48": {"max_tokens": 48}, "both": {"max_tokens": 48, "max_completion_tokens": 5},
         "nolimit": {}, "stream": {"max_tokens": 48, "stream": True, "stream_options": {"include_usage": True}}}


def main():
    a = argparse.ArgumentParser()
    for k in ("exe", "home", "out", "label", "model"):
        a.add_argument("--" + k, required=True)
    a.add_argument("--port", type=int, default=8013)
    a.add_argument("--n", type=int, default=5)
    a.add_argument("--think-off", action="store_true", help='send think_mode "off" (the answer limit then covers all generated text)')
    o = a.parse_args()
    out = Path(o.out); out.mkdir(parents=True, exist_ok=True)
    srv = Server(Path(o.exe), Path(o.home), o.port, out / f"{o.label}_server.log")
    srv.start()
    try:
        with open(out / f"{o.label}.jsonl", "w", encoding="utf-8") as f:
            for case, extra in CASES.items():
                for i in range(o.n):
                    body = {"model": o.model, "messages": [{"role": "user", "content": PROMPT}], **extra}
                    if o.think_off:
                        body["think_mode"] = "off"
                    at = srv.mark()
                    stream = bool(extra.get("stream"))
                    status, reply = srv.request("POST", "/v1/chat/completions", body, stream=stream)
                    steps = sum("generating again" in l for l in srv.since(at))
                    if stream:
                        r = streamed_reply(reply)
                        usage = None
                        for line in reply.splitlines():
                            if line.startswith("data: {") and '"usage"' in line:
                                u = json.loads(line[6:]).get("usage") or {}
                                usage = u.get("completion_tokens", usage)
                        row = {"case": case, "i": i, "status": status, "completion_tokens": usage,
                               "finish_reason": r["finish_reason"], "content_chars": len(r.get("content") or "")}
                    else:
                        ch = (reply.get("choices") or [{}])[0] if isinstance(reply, dict) else {}
                        row = {"case": case, "i": i, "status": status,
                               "completion_tokens": (reply.get("usage") or {}).get("completion_tokens") if isinstance(reply, dict) else None,
                               "finish_reason": ch.get("finish_reason"),
                               "content_chars": len((ch.get("message") or {}).get("content") or "")}
                    row["tool_steps"] = steps
                    print(json.dumps(row), flush=True)
                    f.write(json.dumps(row) + "\n")
    finally:
        srv.stop()


if __name__ == "__main__":
    main()
