"""Tool-loop final answer probe (run: tool loop final answer).

Starts one isolated `cluaiz serve` (CLUAIZ_HOME, port, 127.0.0.1), sends long prompts that make a small
model keep calling the Engine's own tools, and records for every request: the reply, the wall time,
the engine log lines of that request, the tool steps the Engine ran (`Step N: generating again`), whether
the turn stopped at the step / repeat limit, and whether the final no-tools turn ran.

python -B loop_probe.py --exe <cluaiz.exe> --home <isolated-home> --port 8014 --out <dir>
       --model qwen3-0.6b-gguf-Q4_K_M --case marker --n 10 [--stream] [--max-steps N] [--client-tools]

A request "ends with a real answer" when `choices[0].message.content` (streaming: the joined content
deltas) is not empty, holds no `<tool_call>` and the reply is not a client `tool_calls` reply. The marker
case also records whether the answer holds the marker number.
The session token is read from the home and never written to the output.
"""
import argparse
import http.client
import json
import os
import subprocess
import threading
import time
from pathlib import Path

FILLER = "The quarterly report lists numbers for each region and each product line. "


def marker_prompt(i):
    """Run 52 / run 55 `long_input`: one number at the start, 1,500 or more repetitions of `word`, then the
    question (i = 0 is run 52's prompt exactly). Each request gets its own marker and length, so the 10
    requests are 10 different prompts. (A first look with other filler sentences made the model answer
    at once in 2 of 3 requests; only the `word` filler looped, so every request uses it.)"""
    marker = 4917 + i * 111
    return marker, f"The marker is {marker}. " + "word " * (1500 + i * 37) + " What is the marker?"


def math_prompt(i):
    """A long text with a calculation request in it: the math plugin is the right tool once."""
    a, b = 1234 + i * 17, 56 + i
    filler = FILLER * (120 + i * 5)
    return a * b, filler + f" Using the numbers above as context, calculate {a} * {b} and tell me the result."


CASES = {"marker": marker_prompt, "math": math_prompt}

CLIENT_TOOL = {"type": "function", "function": {"name": "lookup_marker", "description": "Looks up a marker number.",
                                                "parameters": {"type": "object", "properties": {"q": {"type": "string"}},
                                                               "required": ["q"]}}}


class Server:
    def __init__(self, exe, home, port, log_path):
        self.exe, self.home, self.port = exe, home, port
        self.lines = []
        self.lock = threading.Lock()
        self.log_file = open(log_path, "a", encoding="utf-8")

    def start(self):
        env = os.environ.copy()
        env.update(CLUAIZ_HOME=str(self.home), CLUAIZ_PORT=str(self.port), CLUAIZ_HOST="127.0.0.1")
        self.proc = subprocess.Popen([str(self.exe), "serve"], env=env, stdout=subprocess.PIPE,
                                     stderr=subprocess.STDOUT, text=True, errors="replace", encoding="utf-8")
        threading.Thread(target=self._read, daemon=True).start()
        deadline = time.monotonic() + 120
        while time.monotonic() < deadline:
            if self.proc.poll() is not None:
                raise RuntimeError(f"server exited: {self.proc.returncode}")
            try:
                status, _ = self.request("GET", "/health", None, timeout=5)
                if status == 200:
                    return
            except OSError:
                pass
            time.sleep(0.5)
        raise RuntimeError("server not ready in 120 s")

    def _read(self):
        for line in self.proc.stdout:
            line = line.rstrip()
            with self.lock:
                self.lines.append(line)
            self.log_file.write(line + "\n")
            self.log_file.flush()

    def mark(self):
        with self.lock:
            return len(self.lines)

    def since(self, at):
        with self.lock:
            return self.lines[at:]

    def stop(self):
        if self.proc.poll() is None:
            self.proc.terminate()
            try:
                self.proc.wait(timeout=20)
            except subprocess.TimeoutExpired:
                self.proc.kill()
        self.log_file.close()

    def request(self, method, path, body, timeout=900, stream=False):
        headers = {"Content-Type": "application/json"}
        token = self.home / "session.token"
        if token.exists():
            headers["Authorization"] = "Bearer " + token.read_text(encoding="utf-8").strip()
        conn = http.client.HTTPConnection("127.0.0.1", self.port, timeout=timeout)
        try:
            conn.request(method, path, json.dumps(body) if body is not None else None, headers)
            resp = conn.getresponse()
            raw = resp.read().decode("utf-8", "replace")
        finally:
            conn.close()
        if stream:
            return resp.status, raw
        try:
            return resp.status, json.loads(raw)
        except ValueError:
            return resp.status, raw


def streamed_reply(raw):
    """Joins the content and reasoning deltas of an SSE reply; keeps tool_calls deltas and finish reasons."""
    content, reasoning, client_calls, finish, tools = [], [], [], [], []
    for line in raw.splitlines():
        if not line.startswith("data: ") or line.strip() == "data: [DONE]":
            continue
        try:
            chunk = json.loads(line[6:])
        except ValueError:
            continue
        for choice in chunk.get("choices") or []:
            delta = choice.get("delta") or {}
            if delta.get("content"):
                content.append(delta["content"])
            if delta.get("reasoning_content"):
                reasoning.append(delta["reasoning_content"])
            for call in delta.get("tool_calls") or []:
                if call.get("type") == "function":
                    client_calls.append(call)
                else:
                    tools.append((call.get("function") or {}).get("name"))
            if choice.get("finish_reason"):
                finish.append(choice["finish_reason"])
    return {"content": "".join(content), "reasoning_content": "".join(reasoning), "client_tool_calls": client_calls,
            "finish_reason": finish[-1] if finish else None, "engine_tool_deltas": tools}


def gpu_state():
    """Free / used VRAM and GPU use (nvidia-smi), to show whether other work shared the GPU"""
    try:
        out = subprocess.run(["nvidia-smi", "--query-gpu=memory.used,memory.free,utilization.gpu", "--format=csv,noheader"],
                             capture_output=True, text=True, timeout=20).stdout.strip()
        return out
    except (OSError, subprocess.SubprocessError):
        return "?"


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("--exe", type=Path, required=True)
    p.add_argument("--home", type=Path, required=True)
    p.add_argument("--port", type=int, default=8014)
    p.add_argument("--out", type=Path, required=True)
    p.add_argument("--model", required=True)
    p.add_argument("--case", choices=sorted(CASES), default="marker")
    p.add_argument("--n", type=int, default=10)
    p.add_argument("--start", type=int, default=0)
    p.add_argument("--stream", action="store_true")
    p.add_argument("--max-steps", type=int)
    p.add_argument("--max-tokens", type=int, default=48)
    p.add_argument("--client-tools", action="store_true", help="also send a client function (the client runs its calls)")
    p.add_argument("--label", default="")
    args = p.parse_args()
    args.out.mkdir(parents=True, exist_ok=True)
    server = Server(args.exe, args.home, args.port, args.out / "engine.log")
    server.start()
    results = []
    try:
        for i in range(args.start, args.start + args.n):
            expect, text = CASES[args.case](i)
            body = {"model": args.model, "messages": [{"role": "user", "content": text}], "temperature": 0,
                    "max_tokens": args.max_tokens, "think_mode": "off", "stream": args.stream}
            if args.max_steps is not None:
                body["max_steps"] = args.max_steps
            if args.client_tools:
                body["tools"] = [CLIENT_TOOL]
            gpu_before = gpu_state()
            at = server.mark()
            started = time.perf_counter()
            status, reply = server.request("POST", "/v1/chat/completions", body, stream=args.stream)
            seconds = time.perf_counter() - started
            time.sleep(1.0)  # late log lines of this request
            lines = server.since(at)
            if args.stream:
                message = streamed_reply(reply) if status == 200 else {"content": "", "error": reply}
                client_calls = message.get("client_tool_calls") or []
                finish = message.get("finish_reason")
                saved_reply = message
            else:
                choice = (reply.get("choices") or [{}])[0] if isinstance(reply, dict) else {}
                message = choice.get("message") or {}
                client_calls = message.get("tool_calls") or []
                finish = choice.get("finish_reason")
                saved_reply = reply
            content = (message.get("content") or "").strip()
            real = status == 200 and bool(content) and "<tool_call>" not in content and not client_calls
            row = {
                "label": args.label, "case": args.case, "i": i, "stream": args.stream, "status": status,
                "seconds": round(seconds, 2),
                "steps": sum(1 for l in lines if "generating again with the tool result" in l),
                "tool_calls_run": sum(1 for l in lines if "[API] Tool call:" in l),
                "step_limit": any("stopped at step/repeat limit" in l for l in lines),
                "final_no_tools_turn": any("asking once more for a final answer without tools" in l for l in lines),
                "final_still_called": any("final answer without tools still made a call" in l for l in lines),
                "real_answer": real,
                "has_expected": str(expect) in content,
                "content": content[:400],
                "reasoning_content": (message.get("reasoning_content") or "")[:300],
                "client_tool_calls": client_calls,
                "finish_reason": finish,
                "prompt_chars": len(text),
                "gpu_before": gpu_before,
                "prefill": [l.split("timings: ")[1][:60] for l in lines if "llama.cpp timings:" in l],
            }
            results.append(row)
            print(json.dumps({k: row[k] for k in ("i", "status", "seconds", "steps", "tool_calls_run", "step_limit",
                                                   "final_no_tools_turn", "real_answer", "has_expected", "finish_reason", "gpu_before")}),
                  "|", row["content"][:100].replace("\n", " "), flush=True)
            with open(args.out / "results.jsonl", "a", encoding="utf-8") as f:
                f.write(json.dumps(row) + "\n")
            with open(args.out / "replies.jsonl", "a", encoding="utf-8") as f:
                f.write(json.dumps({"i": i, "body_chars": len(text), "reply": saved_reply}) + "\n")
            with open(args.out / "request_logs.jsonl", "a", encoding="utf-8") as f:
                f.write(json.dumps({"i": i, "lines": lines}) + "\n")
    finally:
        server.stop()
    n = len(results)
    secs = sorted(r["seconds"] for r in results)
    summary = {
        "label": args.label, "case": args.case, "model": args.model, "stream": args.stream, "n": n,
        "real_answers": sum(r["real_answer"] for r in results),
        "with_expected": sum(r["has_expected"] for r in results),
        "step_limit": sum(r["step_limit"] for r in results),
        "final_no_tools_turn": sum(r["final_no_tools_turn"] for r in results),
        "final_still_called": sum(r["final_still_called"] for r in results),
        "steps_total": sum(r["steps"] for r in results),
        "steps_max": max((r["steps"] for r in results), default=0),
        "median_s": secs[n // 2] if n else None,
        "total_s": round(sum(secs), 1),
    }
    print("SUMMARY", json.dumps(summary), flush=True)
    with open(args.out / "summary.jsonl", "a", encoding="utf-8") as f:
        f.write(json.dumps(summary) + "\n")


if __name__ == "__main__":
    main()
