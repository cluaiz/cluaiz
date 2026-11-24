"""Run an isolated, replayable Engine chat soak. Python standard library only.

Example: python test/release_soak/soak.py --exe target/debug/deps/cluaiz.exe
    --home %LOCALAPPDATA%/cluaiz-test/release-soak --out test/cloud/runs/<run>/raw
    --model <installed-model-id> --minutes 60 --seed 52
"""
import argparse
import ctypes
import http.client
import json
import os
from pathlib import Path
import random
import re
import subprocess
import sys
import threading
import time
from concurrent.futures import ThreadPoolExecutor


def redact(value, secrets, home):
    text = json.dumps(value, ensure_ascii=False) if not isinstance(value, str) else value
    for secret in secrets:
        if secret:
            text = text.replace(secret, "<redacted>")
    text = text.replace(str(home), "<engine-home>")
    text = text.replace(str(home).replace("\\", "\\\\"), "<engine-home>")
    text = text.replace(os.environ.get("USERNAME", "\0"), "<user>")
    return re.sub(r"(?<![\d.])(?:\d{1,3}\.){3}\d{1,3}(?![\d.])", "<ip>", text)


class Journal:
    def __init__(self, folder, home):
        self.folder, self.home = folder, home
        self.secrets = []
        self.lock = threading.Lock()
        folder.mkdir(parents=True, exist_ok=True)

    def write(self, name, data):
        line = redact(data, self.secrets, self.home)
        with self.lock, (self.folder / name).open("a", encoding="utf-8") as stream:
            stream.write(line + "\n")
            stream.flush()


class Engine:
    def __init__(self, args, journal):
        self.args, self.journal = args, journal
        self.process = None
        self.log = None

    def start(self):
        env = os.environ.copy()
        env.update(CLUAIZ_HOME=str(self.args.home), CLUAIZ_PORT=str(self.args.port), CLUAIZ_HOST="127.0.0.1")
        self.journal.write("engine.log", "<engine-start>")
        self.process = subprocess.Popen([str(self.args.exe), "serve"], env=env, stdout=subprocess.PIPE,
                                        stderr=subprocess.STDOUT, text=True, errors="replace")
        self.log = threading.Thread(target=self.capture_log, daemon=True)
        self.log.start()
        deadline = time.monotonic() + 90
        while time.monotonic() < deadline:
            if self.process.poll() is not None:
                raise RuntimeError(f"Engine exited at startup: {self.process.returncode}")
            try:
                token_file = self.args.home / "session.token"
                if token_file.exists():
                    token = token_file.read_text(encoding="utf-8").strip()
                    if token not in self.journal.secrets:
                        self.journal.secrets.append(token)
                status, _ = self.request("GET", "/health", None, record=False)
                if status == 200:
                    self.journal.write("events.jsonl", {"event": "engine_started", "pid": self.process.pid, "time": time.time()})
                    return
            except (OSError, TimeoutError):
                pass
            time.sleep(0.5)
        raise RuntimeError("Engine health did not become ready in 90 seconds")

    def stop(self):
        if self.process and self.process.poll() is None:
            self.process.terminate()
            try:
                self.process.wait(timeout=15)
            except subprocess.TimeoutExpired:
                self.process.kill()
                self.process.wait(timeout=5)
        if self.log:
            self.log.join(timeout=5)
        self.journal.write("events.jsonl", {"event": "engine_stopped", "time": time.time()})

    def capture_log(self):
        for line in self.process.stdout:
            self.journal.write("engine.log", line.rstrip())

    def request(self, method, path, body, record=True, stream=False, timeout=180, raw_body=False):
        token_file = self.args.home / "session.token"
        headers = {"Content-Type": "application/json"}
        if token_file.exists():
            headers["Authorization"] = "Bearer " + token_file.read_text(encoding="utf-8").strip()
        conn = http.client.HTTPConnection("127.0.0.1", self.args.port, timeout=timeout)
        started = time.perf_counter()
        payload = body if raw_body else json.dumps(body) if body is not None else None
        if record:
            self.journal.write("requests.jsonl", {"time": time.time(), "method": method, "path": path, "body": body})
        try:
            conn.request(method, path, payload, headers)
            response = conn.getresponse()
            raw = response.read().decode("utf-8", "replace")
            status = response.status
            if stream:
                reply = raw
            else:
                try:
                    reply = json.loads(raw)
                except json.JSONDecodeError:
                    reply = raw
            if record:
                self.journal.write("responses.jsonl", {"time": time.time(), "status": status,
                                   "elapsed_ms": round((time.perf_counter() - started) * 1000, 2), "reply": reply})
            return status, reply
        except Exception as exc:
            if record:
                self.journal.write("responses.jsonl", {"time": time.time(), "error": repr(exc),
                                   "elapsed_ms": round((time.perf_counter() - started) * 1000, 2)})
            raise
        finally:
            conn.close()


def sample(engine, stop):
    while not stop.wait(5):
        process = engine.process
        if process is None or process.poll() is not None:
            continue
        ram_kib = None
        if os.name == "nt":
            try:
                class MemoryCounters(ctypes.Structure):
                    _fields_ = [("size", ctypes.c_ulong), ("faults", ctypes.c_ulong)] + [
                        (name, ctypes.c_size_t) for name in (
                            "peak_working_set", "working_set", "quota_peak_paged", "quota_paged",
                            "quota_peak_nonpaged", "quota_nonpaged", "pagefile", "peak_pagefile", "private")]
                kernel = ctypes.WinDLL("kernel32", use_last_error=True)
                kernel.OpenProcess.restype = ctypes.c_void_p
                handle = kernel.OpenProcess(0x1000, False, process.pid)
                if handle:
                    counters = MemoryCounters()
                    counters.size = ctypes.sizeof(counters)
                    if ctypes.WinDLL("psapi", use_last_error=True).GetProcessMemoryInfo(
                            ctypes.c_void_p(handle), ctypes.byref(counters), counters.size):
                        ram_kib = counters.working_set // 1024
                    kernel.CloseHandle(ctypes.c_void_p(handle))
            except (ValueError, OSError, AttributeError):
                pass
        gpu_mib = None
        try:
            output = subprocess.run(["nvidia-smi", "--query-gpu=memory.used,memory.free",
                "--format=csv,noheader,nounits"], capture_output=True, text=True, timeout=5)
            gpu_mib = output.stdout.strip()
        except (OSError, subprocess.TimeoutExpired):
            pass
        engine.journal.write("memory.jsonl", {"time": time.time(), "pid": process.pid,
                             "ram_kib": ram_kib, "gpu_used_free_mib": gpu_mib})


def chat(engine, model, messages, **options):
    body = {"model": model, "messages": messages, "temperature": 0, "max_tokens": 48,
            "think_mode": "off", **options}
    status, reply = engine.request("POST", "/v1/chat/completions", body, stream=options.get("stream", False))
    if options.get("stream"):
        ok = status == 200 and "data: [DONE]" in reply and "data: {" in reply
    else:
        ok = status == 200 and isinstance(reply, dict) and bool(reply.get("choices"))
    return ok, status, reply


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("--exe", type=Path, required=True)
    p.add_argument("--home", type=Path, required=True)
    p.add_argument("--out", type=Path, required=True)
    p.add_argument("--model", required=True)
    p.add_argument("--port", type=int, default=8013)
    p.add_argument("--minutes", type=float, default=60)
    p.add_argument("--seed", type=int, default=52)
    p.add_argument("--users", type=int, default=3)
    p.add_argument("--restart-every", type=int, default=1,
                   help="restart after this many complete shuffled scenario sweeps")
    p.add_argument("--exclude", default="", help="comma-separated scenario names excluded from this run")
    args = p.parse_args()
    args.home, args.out, args.exe = args.home.resolve(), args.out.resolve(), args.exe.resolve()
    if args.port in (8000, 8011) or not (1 <= args.port <= 65535):
        p.error("use an isolated port other than 8000 or 8011")
    owner_home = Path.home() / ".cluaiz"
    if args.home == owner_home or owner_home in args.home.parents or args.home == Path.home():
        p.error("never use the owner's home")
    if not args.exe.is_file() or args.minutes <= 0 or not (3 <= args.users <= 5) or args.restart_every < 1:
        p.error("exe must exist, minutes positive, users 3..5, restart-every positive")
    args.home.mkdir(parents=True, exist_ok=True)
    journal = Journal(args.out, args.home)
    journal.write("events.jsonl", {"event": "setup", "seed": args.seed, "minutes": args.minutes,
                  "users": args.users, "restart_every": args.restart_every,
                  "excluded": args.exclude, "model": args.model, "exe": str(args.exe), "time": time.time()})
    engine = Engine(args, journal)
    stop = threading.Event()
    monitor = threading.Thread(target=sample, args=(engine, stop), daemon=True)
    passed = failed = 0
    try:
        engine.start()
        monitor.start()
        scenarios = ["single", "stream", "multi", "concurrent", "bad_json", "unknown_model",
                     "empty", "n", "logprobs", "max_tokens", "long_input", "think_on", "cancel_stream"]
        excluded = {item for item in args.exclude.split(",") if item}
        if excluded - set(scenarios):
            p.error(f"unknown scenario exclusions: {sorted(excluded - set(scenarios))}")
        scenarios = [name for name in scenarios if name not in excluded]
        rng = random.Random(args.seed)
        deadline = time.monotonic() + args.minutes * 60
        turn = 0
        sweep = 0
        while time.monotonic() < deadline:
            sweep += 1
            rng.shuffle(scenarios)
            for scenario in scenarios:
                if time.monotonic() >= deadline:
                    break
                turn += 1
                try:
                    if scenario == "bad_json":
                        status, reply = engine.request("POST", "/v1/chat/completions", "{bad json", raw_body=True)
                        ok = status in (400, 422)
                    elif scenario == "unknown_model":
                        ok, status, reply = chat(engine, "missing-model-release-soak", [{"role": "user", "content": "Hi"}])
                        ok = status in (400, 404)
                    elif scenario == "empty":
                        ok, status, reply = chat(engine, args.model, [])
                        ok = status in (400, 422)
                    elif scenario in ("n", "logprobs"):
                        key, value = ("n", 2) if scenario == "n" else ("logprobs", True)
                        ok, status, reply = chat(engine, args.model, [{"role": "user", "content": "Hi"}], **{key: value})
                        ok = status == 400 and key in json.dumps(reply)
                    elif scenario == "max_tokens":
                        ok, status, reply = chat(engine, args.model, [{"role": "user", "content": "Count from one to twenty."}],
                                                  max_completion_tokens=5)
                        ok = ok and reply.get("usage", {}).get("completion_tokens", 999) <= 5
                    elif scenario == "long_input":
                        ok, status, reply = chat(engine, args.model, [{"role": "user", "content":
                            "The marker is 4917. " + "word " * 1500 + " What is the marker?"}])
                        ok = ok and "4917" in json.dumps(reply)
                    elif scenario == "think_on":
                        ok, status, reply = chat(engine, args.model, [{"role": "user", "content": "What is 2+2?"}],
                                                  think_mode="on", max_tokens=128)
                        ok = ok and isinstance(reply.get("choices", [{}])[0].get("message", {}), dict)
                    elif scenario == "cancel_stream":
                        conn = http.client.HTTPConnection("127.0.0.1", args.port, timeout=30)
                        token = (args.home / "session.token").read_text(encoding="utf-8").strip()
                        body = {"model": args.model, "stream": True, "think_mode": "off", "max_tokens": 256,
                                "messages": [{"role": "user", "content": "Count from one to one hundred."}]}
                        journal.write("requests.jsonl", {"time": time.time(), "method": "POST",
                                      "path": "/v1/chat/completions", "body": body, "cancel_after_first_line": True})
                        try:
                            conn.request("POST", "/v1/chat/completions", json.dumps(body),
                                         {"Content-Type": "application/json", "Authorization": "Bearer " + token})
                            response = conn.getresponse()
                            first = response.readline().decode("utf-8", "replace")
                            status, reply = response.status, first
                            ok = status == 200 and first.startswith("data: ")
                            journal.write("responses.jsonl", {"time": time.time(), "status": status,
                                          "first_line": first, "client_closed_stream": True})
                        finally:
                            conn.close()
                    elif scenario == "concurrent":
                        with ThreadPoolExecutor(max_workers=args.users) as pool:
                            results = list(pool.map(lambda _: chat(engine, args.model,
                                [{"role": "user", "content": "What is 2+2?"}]), range(args.users)))
                        ok = all(valid and "4" in json.dumps(answer) for valid, _, answer in results)
                        status, reply = 200 if ok else 500, {"users": args.users, "results": results}
                    elif scenario == "multi":
                        messages = []
                        ok = True
                        for index in range(10):
                            messages.append({"role": "user", "content": f"Reply with the number {index}."})
                            valid, status, reply = chat(engine, args.model, messages)
                            ok = ok and valid
                            if not valid:
                                break
                            messages.append({"role": "assistant", "content": reply["choices"][0]["message"].get("content") or ""})
                    else:
                        ok, status, reply = chat(engine, args.model, [{"role": "user", "content": "What is 2+2?"}],
                                                  **({"stream": True} if scenario == "stream" else {}))
                        ok = ok and ("4" in json.dumps(reply) if scenario == "single" else True)
                    passed += int(ok)
                    failed += int(not ok)
                    journal.write("checks.jsonl", {"turn": turn, "scenario": scenario, "pass": ok, "status": status})
                except Exception as exc:
                    failed += 1
                    journal.write("checks.jsonl", {"turn": turn, "scenario": scenario, "pass": False, "error": repr(exc)})
            if sweep % args.restart_every == 0 and time.monotonic() < deadline:
                engine.stop()
                engine.start()
        journal.write("events.jsonl", {"event": "finished", "passed": passed, "failed": failed,
                      "sweeps": sweep, "time": time.time()})
    finally:
        stop.set()
        engine.stop()
        if monitor.is_alive():
            monitor.join(timeout=6)
    print(f"seed={args.seed} passed={passed} failed={failed} raw={args.out}")
    return int(failed > 0)


if __name__ == "__main__":
    sys.exit(main())
