"""CEP end to end (04-implementation-plan §11.4 CEP-E0..E3, CEP-S17, CEP-P1) against an isolated Engine.

    python test/cep/journeys.py --exe target/release/cluaiz.exe --home <isolated CLUAIZ_HOME> [--port 8011]

Starts `cluaiz serve` with that home and stops it after. Never use the owner's own home or port.
"""
import argparse
import http.client
import json
import os
import statistics
import subprocess
import sys
import time
import urllib.request


class Client:
    """One keep-alive connection, as a real client would hold"""

    def __init__(self, port, token):
        self.connection = http.client.HTTPConnection("127.0.0.1", int(port), timeout=120)
        self.headers = {"Content-Type": "application/json", "Authorization": f"Bearer {token}"}

    def post(self, path, body):
        self.connection.request("POST", path, json.dumps(body), self.headers)
        response = self.connection.getresponse()
        text = response.read()
        return response.status, (json.loads(text) if text else None)

    def cep(self, method, **params):
        return self.post("/v1/cep", {"jsonrpc": "2.0", "id": 1, "method": method, "params": params})[1]


def main():
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--exe", required=True)
    parser.add_argument("--home", required=True)
    parser.add_argument("--port", default="8011")
    args = parser.parse_args()
    env = dict(os.environ, CLUAIZ_HOME=args.home, CLUAIZ_PORT=args.port, CLUAIZ_HOST="127.0.0.1")
    server = subprocess.Popen([args.exe, "serve"], env=env, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    results = []

    def check(name, ok, detail=""):
        results.append(ok)
        print(f"{'PASS' if ok else 'FAIL'} {name:40s} {detail}", flush=True)

    try:
        started = time.time()
        for _ in range(90):
            try:
                urllib.request.urlopen(f"http://127.0.0.1:{args.port}/health", timeout=2)
                break
            except Exception:
                time.sleep(1)
        check("E0 Engine answers /health", True, f"{time.time() - started:.1f} s")
        client = Client(args.port, open(os.path.join(args.home, "session.token"), encoding="utf-8").read().strip())

        # E1: discovery and batch
        full = client.cep("rpc.discover")["result"]
        names = [m["name"] for m in full["methods"]]
        every_example = all(m["examples"] and m["x-cep"]["scope"] for m in full["methods"])
        check("E1 rpc.discover", full["openrpc"] == "1.3.2" and every_example, f"{len(names)} methods, each with scope and example")
        group = client.cep("rpc.discover", group="notes")["result"]["methods"]
        check("E1 rpc.discover group", {m["name"] for m in group} == {n for n in names if n.startswith("notes.")}, f"{len(group)} notes methods")
        status, batch = client.post("/v1/cep", [{"jsonrpc": "2.0", "id": 1, "method": "system.info"},
                                                {"jsonrpc": "2.0", "id": 2, "method": "notes.lst"},
                                                {"jsonrpc": "2.0", "method": "tasks.list"}])
        check("E1 batch", status == 200 and [r["id"] for r in batch] == [1, 2] and batch[1]["error"]["data"]["did_you_mean"][0] == "notes.list",
              f"ids {[r['id'] for r in batch]}, did_you_mean {batch[1]['error']['data']['did_you_mean']}")
        status, nothing = client.post("/v1/cep", {"jsonrpc": "2.0", "method": "tasks.list"})
        check("E1 notification only", status == 202 and nothing is None, f"HTTP {status}")

        # E3: a task round trip (reads the App's chat folders only, imports nothing: dry run, unknown chat)
        task = client.cep("chat.import_app", chat_id="cep-journey-no-such-chat", dry_run=True)["result"]["task"]
        result = client.cep("tasks.result", task_id=task["task_id"], wait_ms=30000)
        view = client.cep("tasks.get", task_id=task["task_id"])["result"]
        check("E3 task round trip", task["status"] == "working" and result["result"]["chats"] == [] and view["status"] == "completed",
              f"{task['task_id'][:10]}… → {view['status']}")

        # S17: a write is audited with the params as a hash only; reads are not
        marker = f"cep-journey-{int(time.time())}.txt"
        client.cep("fs.write", path=marker, text="secret-content-not-in-audit")
        client.cep("fs.delete", path=marker)
        lines = client.cep("system.audit", limit=20)["result"]["lines"]
        writes = [l for l in lines if l["method"] == "fs.write" and l["principal"] == "owner"]
        leaked = any("secret-content" in json.dumps(l) for l in lines)
        reads = [l for l in lines if l["method"] == "rpc.discover"]
        check("S17 audit", bool(writes) and len(writes[0]["params_sha256"]) == 64 and not leaked and not reads,
              f"{len(lines)} lines, fs.write audited, content not in it, reads not audited")

        # P1: overhead of a small read over loopback HTTP (keep-alive), as the client sees it
        timings = []
        for _ in range(300):
            begin = time.perf_counter()
            client.cep("tasks.list")
            timings.append((time.perf_counter() - begin) * 1000)
        timings.sort()
        p50, p95 = statistics.median(timings), timings[int(0.95 * (len(timings) - 1))]
        check("P1 loopback HTTP tasks.list", p95 <= 3.0, f"p50 {p50:.2f} ms, p95 {p95:.2f} ms (target p95 ≤ 3 ms)")
    finally:
        server.kill()
    print(f"\n{sum(results)} of {len(results)} passed")
    return 0 if all(results) else 1


if __name__ == "__main__":
    sys.exit(main())
