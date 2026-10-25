"""Shared helpers for the context runtime suites: start and stop an isolated Engine, and talk to it
like the App does (streaming, approvals) or like a plain client (non-streaming).

An Engine is either started here (`--exe` and `--home`: the suite can restart it, e.g. for J10) or
one that already runs (`--url` and `--token`). Never point these at the user's own server.
"""
import json
import os
import subprocess
import time
import urllib.error
import urllib.request
from pathlib import Path


class Engine:
    def __init__(self, url="http://127.0.0.1:8011", token=None, exe=None, home=None, workspace=None, log=None):
        self.url, self.token, self.exe, self.home = url.rstrip("/"), token, exe, home
        self.workspace, self.log_path, self.proc = workspace, log, None

    @property
    def port(self):
        return self.url.rsplit(":", 1)[-1]

    def start(self, timeout=90):
        """Starts the Engine (when it is ours) and waits until it answers"""
        if self.exe:
            env = dict(os.environ, CLUAIZ_HOME=str(self.home), CLUAIZ_PORT=self.port, CLUAIZ_HOST="127.0.0.1")
            log = open(self.log_path, "w", encoding="utf-8") if self.log_path else subprocess.DEVNULL
            self.proc = subprocess.Popen([self.exe, "serve"], env=env, stdout=log, stderr=subprocess.STDOUT)
        started = time.time()
        while time.time() - started < timeout:
            try:
                urllib.request.urlopen(self.url + "/health", timeout=2).read()
                break
            except Exception:
                time.sleep(0.5)
        else:
            raise RuntimeError("the Engine did not answer within %d s" % timeout)
        ready = time.time() - started
        if self.home and not self.token:
            self.token = Path(self.home, "session.token").read_text().strip()
        if self.workspace:
            self.post("/v1/workspace", {"workspace": str(self.workspace)})
        return ready

    def stop(self):
        if self.proc:
            # The whole tree: MCP servers are children of the Engine
            subprocess.run(["taskkill", "/PID", str(self.proc.pid), "/T", "/F"], capture_output=True)
            self.proc.wait(timeout=30)
            self.proc = None

    def restart(self):
        if not self.exe:
            raise RuntimeError("restart needs --exe and --home")
        self.stop()
        time.sleep(2)
        self.token = None
        return self.start()

    def pid(self):
        return self.proc.pid if self.proc else None

    def _request(self, method, path, body=None, timeout=900):
        data = json.dumps(body).encode() if body is not None else None
        req = urllib.request.Request(self.url + path, data=data, method=method,
                                     headers={"Authorization": "Bearer " + (self.token or ""), "Content-Type": "application/json"})
        return urllib.request.urlopen(req, timeout=timeout)

    def get(self, path):
        with self._request("GET", path) as r:
            return json.load(r)

    def post(self, path, body, timeout=900):
        with self._request("POST", path, body, timeout) as r:
            return json.load(r)

    def delete(self, path):
        with self._request("DELETE", path) as r:
            return json.load(r)

    def cep(self, method, **params):
        """One CEP call (`POST /v1/cep`); returns the result, raises on an error"""
        reply = self.post("/v1/cep", {"jsonrpc": "2.0", "id": 1, "method": method, "params": params})
        if "error" in reply:
            raise CepCallError(method, reply["error"])
        return reply["result"]

    def chat(self, body):
        """Non-streaming chat; returns the whole response"""
        return self.post("/v1/chat/completions", dict(body, stream=False))

    def stream(self, body, decide="approve"):
        """Streaming chat like the App. `decide`: "approve", "reject", "ignore" (let it time out) or
        a function (permission_request) -> "approve" | "reject". Returns the answer text, the calls,
        the permission requests, the `context` event and the closing engine chunk."""
        out = {"text": "", "calls": [], "asked": [], "context": None, "engine": None, "error": None, "seconds": 0.0}
        started = time.time()
        try:
            with self._request("POST", "/v1/chat/completions", dict(body, stream=True)) as r:
                for raw in r:
                    line = raw.decode("utf-8", "replace").strip()
                    if not line.startswith("data:") or line[5:].strip() == "[DONE]":
                        continue
                    try:
                        ev = json.loads(line[5:])
                    except ValueError:
                        continue
                    if ev.get("error"):
                        out["error"] = ev["error"]
                    if ev.get("context"):
                        out["context"] = ev["context"]
                    if ev.get("engine") is not None or ev.get("context_report") is not None:
                        out["engine"] = ev
                    for ch in ev.get("choices", []):
                        d = ch.get("delta", {}) or {}
                        out["text"] += d.get("content") or ""
                        for tc in d.get("tool_calls") or []:
                            fn = tc.get("function", {})
                            out["calls"].append({"tool": fn.get("name", ""), "name": fn.get("sub_function") or fn.get("name", ""),
                                                 "arguments": fn.get("arguments", ""), "auto": bool(fn.get("auto"))})
                        pr = d.get("permission_request")
                        if pr:
                            out["asked"].append(pr)
                            choice = decide(pr) if callable(decide) else decide
                            if choice in ("approve", "reject"):
                                self.post("/v1/system/permission/" + choice, {"request_id": pr["request_id"]}, timeout=60)
        except urllib.error.HTTPError as e:
            out["error"] = {"status": e.code, "body": e.read()[:300].decode("utf-8", "replace")}
        except Exception as e:  # network or timeout
            out["error"] = {"message": f"{type(e).__name__}: {e}"}
        out["seconds"] = round(time.time() - started, 1)
        return out


class CepCallError(RuntimeError):
    """A CEP error reply: `code` and `data` as the Engine sent them"""

    def __init__(self, method, error):
        super().__init__(f"{method}: {error}")
        self.code, self.data = error.get("code"), error.get("data") or {}


def add_engine_args(parser):
    parser.add_argument("--url", default="http://127.0.0.1:8011")
    parser.add_argument("--token", default=os.environ.get("CLUAIZ_TOKEN"))
    parser.add_argument("--exe", help="start this Engine (target/release/cluaiz.exe) instead of using a running one")
    parser.add_argument("--home", default=os.environ.get("CLUAIZ_HOME"), help="CLUAIZ_HOME of the isolated Engine")
    parser.add_argument("--workspace", help="project folder for the tools (the test fixture with notes.txt)")
    parser.add_argument("--log", help="Engine log file (with --exe)")


def engine_from(args):
    engine = Engine(args.url, args.token, args.exe, args.home, args.workspace, args.log)
    if not engine.token and not engine.exe and engine.home:
        engine.token = Path(engine.home, "session.token").read_text().strip()
    return engine
