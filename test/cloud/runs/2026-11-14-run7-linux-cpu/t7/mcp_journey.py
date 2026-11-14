"""MCP over CEP (04-implementation-plan P4, tests CEP-M1..M5) against an isolated Engine.

    python test/cep/mcp_journey.py --exe target/release/cluaiz.exe --home <isolated CLUAIZ_HOME> [--port 8011]

Starts `cluaiz serve` with that home, checks `/mcp` over HTTP, then the same through
`cluaiz mcp-server` (stdio), and stops what it started. Never use the owner's own home or port.
"""
import argparse
import json
import os
import subprocess
import sys
import time
import urllib.request


def post(url, token, body):
    request = urllib.request.Request(url, data=json.dumps(body).encode(), method="POST",
                                     headers={"Content-Type": "application/json", "Authorization": f"Bearer {token}"})
    with urllib.request.urlopen(request, timeout=120) as response:
        text = response.read()
        return response.status, (json.loads(text) if text else None)


def check(results, name, ok, detail):
    results.append(ok)
    print(f"{'PASS' if ok else 'FAIL'} {name:34s} {detail}", flush=True)


def journey(results, call):
    """`call(message) -> reply or None`: the same checks over HTTP and over stdio"""
    init = call({"jsonrpc": "2.0", "id": 1, "method": "initialize",
                 "params": {"protocolVersion": "2025-06-18", "capabilities": {}, "clientInfo": {"name": "journey", "version": "1"}}})
    check(results, "initialize negotiates", init["result"]["protocolVersion"] == "2025-06-18", init["result"]["serverInfo"])
    check(results, "initialized notification", call({"jsonrpc": "2.0", "method": "notifications/initialized"}) is None, "no reply")
    tools = call({"jsonrpc": "2.0", "id": 2, "method": "tools/list"})["result"]["tools"]
    names = [t["name"] for t in tools]
    read_only = all(t["annotations"]["readOnlyHint"] for t in tools)
    check(results, "tools/list is read-only", read_only and "system.info" in names and "fs.write" not in names,
          f"{len(names)} tools, e.g. {names[:5]}")
    info = call({"jsonrpc": "2.0", "id": 3, "method": "tools/call", "params": {"name": "system.info", "arguments": {}}})["result"]
    check(results, "tools/call system.info", not info["isError"] and info["structuredContent"]["engine"] == "Cluaiz Engine",
          info["structuredContent"].get("version"))
    denied = call({"jsonrpc": "2.0", "id": 4, "method": "tools/call", "params": {"name": "fs.write", "arguments": {"path": "x.txt", "text": "no"}}})["result"]
    check(results, "write method refused", denied["isError"] and "forbidden" in denied["content"][0]["text"], denied["content"][0]["text"][:90])
    bad = call({"jsonrpc": "2.0", "id": 5, "method": "tools/call", "params": {"name": "system.infoo", "arguments": {}}})
    check(results, "unknown tool", bad.get("error", {}).get("code") == -32602, bad.get("error"))
    wrong = call({"jsonrpc": "2.0", "id": 6, "method": "tools/call", "params": {"name": "memory.events", "arguments": {"chat": "x"}}})["result"]
    check(results, "invalid arguments back to model", wrong["isError"] and "-32602" in wrong["content"][0]["text"], wrong["content"][0]["text"][:90])


def main():
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--exe", required=True)
    parser.add_argument("--home", required=True)
    parser.add_argument("--port", default="8011")
    args = parser.parse_args()
    env = dict(os.environ, CLUAIZ_HOME=args.home, CLUAIZ_PORT=args.port, CLUAIZ_HOST="127.0.0.1")
    server = subprocess.Popen([args.exe, "serve"], env=env, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    url = f"http://127.0.0.1:{args.port}"
    results = []
    try:
        for _ in range(90):
            try:
                urllib.request.urlopen(url + "/health", timeout=2)
                break
            except Exception:
                time.sleep(1)
        token = open(os.path.join(args.home, "session.token"), encoding="utf-8").read().strip()

        def http(message):
            status, reply = post(url + "/mcp", token, message)
            return None if status == 202 else reply
        print("-- HTTP /mcp")
        journey(results, http)

        print("-- stdio: cluaiz mcp-server")
        bridge = subprocess.Popen([args.exe, "mcp-server"], env=env, stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                                  stderr=subprocess.DEVNULL, text=True, encoding="utf-8")

        def stdio(message):
            bridge.stdin.write(json.dumps(message) + "\n")
            bridge.stdin.flush()
            if "id" not in message:
                return None
            return json.loads(bridge.stdout.readline())
        try:
            journey(results, stdio)
        finally:
            bridge.stdin.close()
            bridge.wait(timeout=30)
    finally:
        server.kill()
    print(f"\n{sum(results)} of {len(results)} passed")
    return 0 if all(results) else 1


if __name__ == "__main__":
    sys.exit(main())
