"""Run 77: Run58 prompts and exact grading with 5, Engine-plus-5, or 20 tools.

Runs on the VM only. The local server uses an isolated CLUAIZ_HOME and port 8013.
Each invocation runs one model, setup and thinking mode; append-only raw files allow
interrupted runs to resume without turning missing replies into scores.
"""
import argparse
import ast
import json
import os
import re
import statistics
import subprocess
import threading
import time
from pathlib import Path
from urllib.request import Request, urlopen
from urllib.error import HTTPError


def run58():
    """Extract only the unchanged data and grading functions; never run its CLI loop."""
    tree = ast.parse(Path(__file__).with_name("run58_probe.py").read_text(encoding="utf-8"))
    keep = []
    for node in tree.body:
        if isinstance(node, ast.FunctionDef) and node.name in ("fn", "same", "grade"):
            keep.append(node)
        elif isinstance(node, ast.Assign) and any(isinstance(t, ast.Name) and t.id in ("TOOLS", "TASKS") for t in node.targets):
            keep.append(node)
    ns = {"json": json}
    exec(compile(ast.Module(body=keep, type_ignores=[]), "run58_probe.py", "exec"), ns)
    return ns


BASE = run58()
fn, TOOLS, TASKS, grade = (BASE[k] for k in ("fn", "TOOLS", "TASKS", "grade"))
DUMMIES = [
    fn("get_calendar_events", "List events on a date", {"date": {"type": "string"}}, ["date"]),
    fn("create_calendar_event", "Create a calendar event", {"title": {"type": "string"}, "start": {"type": "string"}}, ["title", "start"]),
    fn("find_contacts", "Search contacts by name", {"name": {"type": "string"}}, ["name"]),
    fn("get_stock_quote", "Get a current stock price", {"symbol": {"type": "string"}}, ["symbol"]),
    fn("search_web", "Search public web pages", {"query": {"type": "string"}}, ["query"]),
    fn("get_news", "Find recent news", {"topic": {"type": "string"}}, ["topic"]),
    fn("translate_text", "Translate text", {"text": {"type": "string"}, "language": {"type": "string"}}, ["text", "language"]),
    fn("summarize_document", "Summarize a document by path", {"path": {"type": "string"}}, ["path"]),
    fn("list_tasks", "List current tasks", {}, []),
    fn("create_task", "Create a task", {"title": {"type": "string"}}, ["title"]),
    fn("get_map_route", "Find a route", {"origin": {"type": "string"}, "destination": {"type": "string"}}, ["origin", "destination"]),
    fn("book_ride", "Request a ride", {"destination": {"type": "string"}}, ["destination"]),
    fn("get_package_status", "Check a shipment", {"tracking_id": {"type": "string"}}, ["tracking_id"]),
    fn("set_timer", "Start a timer", {"minutes": {"type": "number"}}, ["minutes"]),
    fn("control_light", "Turn a room light on or off", {"room": {"type": "string"}, "state": {"type": "string"}}, ["room", "state"]),
]


def redact(value):
    if isinstance(value, str):
        value = re.sub(r"/home/[^/\\\s\"']+", "/home/<vps>", value)
        return re.sub(r"(?<![\w.])(?:\d{1,3}\.){3}\d{1,3}(?![\w.])", "<vps>", value)
    if isinstance(value, dict):
        return {k: redact(v) for k, v in value.items()}
    if isinstance(value, list):
        return [redact(v) for v in value]
    return value


class Server:
    def __init__(self, exe, home, out):
        self.lines, self.lock = [], threading.Lock()
        self.log = open(out / "server.log", "a", encoding="utf-8")
        env = dict(os.environ, CLUAIZ_HOME=str(home), CLUAIZ_PORT="8013", CLUAIZ_HOST="localhost")
        self.proc = subprocess.Popen([str(exe), "serve"], env=env, stdout=subprocess.PIPE,
                                     stderr=subprocess.STDOUT, text=True, errors="replace")
        threading.Thread(target=self.read, daemon=True).start()
        for _ in range(240):
            if self.proc.poll() is not None:
                raise RuntimeError(f"server exited {self.proc.returncode}")
            try:
                self.request("/health", None, home, method="GET", timeout=3)
                break
            except Exception:
                time.sleep(.5)
        else:
            raise RuntimeError("server not ready")

    def read(self):
        for line in self.proc.stdout:
            line = redact(line.rstrip())
            with self.lock:
                self.lines.append(line)
            self.log.write(line + "\n")
            self.log.flush()

    def mark(self):
        with self.lock:
            return len(self.lines)

    def since(self, n):
        with self.lock:
            return self.lines[n:]

    def request(self, route, body, home, method="POST", timeout=300):
        headers = {"Content-Type": "application/json"}
        token = home / "session.token"
        if token.exists():
            headers["Authorization"] = "Bearer " + token.read_text().strip()
        req = Request("http://localhost:8013" + route,
                      None if body is None else json.dumps(body).encode(), headers, method=method)
        try:
            with urlopen(req, timeout=timeout) as r:
                return r.status, json.loads(r.read())
        except HTTPError as e:
            return e.code, {"error": e.read().decode(errors="replace")[:1000]}

    def stop(self):
        self.proc.terminate()
        try:
            self.proc.wait(timeout=20)
        except subprocess.TimeoutExpired:
            self.proc.kill()
        self.log.close()


def openai_shape(reply, setup):
    if setup == "B":
        return reply
    msg = reply.get("message") or {}
    calls = []
    for c in msg.get("tool_calls") or []:
        f = c.get("function") or {}
        args = f.get("arguments") or {}
        calls.append({"function": {"name": f.get("name"), "arguments": args if isinstance(args, str) else json.dumps(args)}})
    return {"choices": [{"message": {"content": msg.get("content"), "tool_calls": calls}}]}


def call_names(lines):
    return [line.split("[API] Tool call:", 1)[1].split(" with ", 1)[0].strip()
            for line in lines if "[API] Tool call:" in line]


def main():
    p = argparse.ArgumentParser()
    for arg in ("exe", "home", "out", "model"):
        p.add_argument("--" + arg, required=True)
    p.add_argument("--setup", choices="ABC", required=True)
    p.add_argument("--think", choices=("on", "off"), required=True)
    p.add_argument("--start", type=int, default=0)
    p.add_argument("--limit", type=int, default=40)
    args = p.parse_args()
    home, out = Path(args.home), Path(args.out)
    out.mkdir(parents=True, exist_ok=True)
    server = Server(Path(args.exe), home, out)
    raw = out / "raw.jsonl"
    try:
        for i in range(args.start, min(args.start + args.limit, 40)):
            kind, prompt, want_tool, want_args = TASKS[i]
            tools = TOOLS + (DUMMIES if args.setup == "C" else [])
            if args.setup == "B":
                route = "/v1/chat/completions"
                body = {"model": args.model, "messages": [{"role": "user", "content": prompt}],
                        "temperature": 0, "max_tokens": 1536 if args.think == "on" else 256,
                        "think_mode": args.think, "tools": tools, "stream": False}
            else:
                route = "/api/chat"
                body = {"model": args.model, "messages": [{"role": "user", "content": prompt}],
                        "stream": False, "think": args.think == "on", "tools": tools,
                        "options": {"temperature": 0, "num_predict": 1536 if args.think == "on" else 256}}
            mark = server.mark()
            start = time.perf_counter()
            try:
                status, reply = server.request(route, body, home)
            except Exception as exc:
                status, reply = 0, {"error": str(exc)}
            seconds = round(time.perf_counter() - start, 2)
            time.sleep(.1)
            lines = server.since(mark)
            if status == 200:
                verdict, leak, client_calls, content = grade(kind, want_tool, want_args, openai_shape(reply, args.setup))
            else:
                verdict, leak, client_calls, content = "error", False, [], ""
            names = call_names(lines)
            row = {"i": i, "kind": kind, "prompt": prompt, "want_tool": want_tool,
                   "want_args": want_args, "status": status, "seconds": seconds, "verdict": verdict,
                   "leak": leak, "client_calls": client_calls, "engine_calls": names,
                   "steps": sum("generating again with the tool result" in l for l in lines),
                   "repeat_limit": any("stopped at step/repeat limit" in l for l in lines),
                   "wrong_no_tool": kind == "none" and bool(client_calls or names),
                   "content": content, "reply": reply, "log_lines": lines}
            with raw.open("a", encoding="utf-8") as f:
                f.write(json.dumps(redact(row), ensure_ascii=False) + "\n")
            print(f"{i:02d} {kind} {verdict} {seconds}s steps={row['steps']} engine={len(names)}", flush=True)
        if args.setup == "B" and args.start == 0 and args.limit == 40:
            body = {"model": args.model, "messages": [{"role": "user", "content": "6*7"}],
                    "temperature": 0, "max_tokens": 1536 if args.think == "on" else 256,
                    "think_mode": args.think, "tools": TOOLS, "stream": False}
            mark, start = server.mark(), time.perf_counter()
            status, reply = server.request("/v1/chat/completions", body, home)
            lines = server.since(mark)
            with (out / "math.json").open("w", encoding="utf-8") as f:
                json.dump(redact({"status": status, "seconds": round(time.perf_counter()-start, 2),
                           "steps": sum("generating again with the tool result" in l for l in lines),
                           "engine_calls": call_names(lines), "reply": reply, "log_lines": lines}), f, indent=2)
    finally:
        server.stop()


if __name__ == "__main__":
    main()
