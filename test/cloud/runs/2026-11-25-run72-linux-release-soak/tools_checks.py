"""Run 72 (copied from run 49 unchanged except this line): tools, approvals, workspace and flow-run checks (App release bugs A1, A2, A4).

    python tools_checks.py --home <CLUAIZ_HOME> --port 8013 --out raw/tools_checks.jsonl

Needs `math` (plugin) and `filesystem` (MCP) installed and the run49-check flow saved (cli_checks.sh).
"""
import argparse
import json
import os
import sys
import threading
import time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from release_e2e import CHAT, Client, trim  # noqa: E402


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--home", required=True)
    parser.add_argument("--port", default="8013")
    parser.add_argument("--out", required=True)
    args = parser.parse_args()
    if args.port == "8000":
        sys.exit("never port 8000")
    c = Client(args.port, open(os.path.join(args.home, "session.token"), encoding="utf-8").read().strip())
    out = open(args.out, "a", encoding="utf-8")

    def record(name, ok, detail, reply=None):
        out.write(json.dumps({"name": name, "pass": bool(ok), "detail": detail, "reply": trim(reply) if reply is not None else None,
                              "at": time.strftime("%Y-%m-%dT%H:%M:%S")}, ensure_ascii=False) + "\n")
        out.flush()
        print(f"{'PASS' if ok else 'FAIL'} {name:38s} {detail}", flush=True)

    def call_approved(params):
        """tools.call; when the Engine asks, approve that one call and send it again"""
        first = c.cep("tools.call", **params)
        err = first.get("error") if isinstance(first, dict) else None
        if err and (err.get("data") or {}).get("type") == "approval_required":
            approval = err["data"]["approval_id"]
            c.cep("approvals.approve", approval_id=approval)
            return first, c.cep("tools.call", **dict(params, _meta={"approval": approval}))
        return first, first

    # A4: the math WebAssembly plugin in the default (sandboxed) mode
    mode = c.cep("config.get", section="engine")["result"]
    mode = mode.get("engine", mode).get("agent_security_mode")
    first, _ = call_approved({"kind": "plugin", "name": "math", "fn": "math_evaluate", "input": {"expression": "2^10+sqrt(16)"}})
    asked = isinstance(first, dict) and "error" in first
    value = json.dumps(first.get("result")) if isinstance(first, dict) else ""
    record("A4_math_cep_no_approval", not asked and "1028" in value, f"mode {mode}; asked {asked}; result {value[:80]}", first)

    # A4 in a chat: the stream must not carry a permission_request for math
    body = {"model": CHAT, "temperature": 0, "stream": True, "think_mode": "off", "max_tokens": 400,
            "messages": [{"role": "user", "content": "Use the math tool to calculate 2^10+sqrt(16). Then tell me the result."}]}
    status, lines = c.stream("/v1/chat/completions", body)
    text = "\n".join(lines)
    asked = "permission_request" in text
    called = "math" in text and ("1028" in text)
    record("A4_math_chat_no_approval", status == 200 and not asked, f"HTTP {status}; permission_request {asked}; math result 1028 seen {called}; {len(lines)} lines", lines[-6:])
    with open(args.out.replace(".jsonl", "_math_chat_stream.txt"), "w", encoding="utf-8") as f:
        f.write(text)

    # A2: no folder opened; the files tool must not see the Engine home
    ws = c.cep("workspace.get")
    ws_path = json.dumps(ws.get("result")) if isinstance(ws, dict) else str(ws)
    first, final = call_approved({"kind": "mcp", "name": "filesystem", "fn": "list_directory", "input": {"path": "."}})
    listing = json.dumps(final.get("result") if isinstance(final, dict) else final)
    leaked = any(name in listing for name in ["session.token", "permission.json", "config"])
    record("A2_files_tool_default_workspace", isinstance(final, dict) and "result" in final and not leaked,
           f"workspace {ws_path[:120]}; listing leaks home: {leaked}; listing {listing[:200]}", [first, final])
    first, final = call_approved({"kind": "mcp", "name": "filesystem", "fn": "list_directory", "input": {"path": ".."}})
    listing = json.dumps(final.get("result") if isinstance(final, dict) else final)
    leaked = "session.token" in listing
    record("A2_files_tool_parent_refused", not leaked, f"'..' listing leaks session.token: {leaked}; {listing[:200]}", [first, final])
    first, final = call_approved({"kind": "mcp", "name": "filesystem", "fn": "read_text_file", "input": {"path": "../session.token"}})
    token = open(os.path.join(args.home, "session.token"), encoding="utf-8").read().strip()
    leaked = token in json.dumps(final)
    record("A2_files_tool_token_unreadable", not leaked, f"read ../session.token leaked the token: {leaked}", {"error": (final or {}).get("error")})
    # The home itself as the workspace (a client that remembers it)
    setr = c.cep("workspace.set", path=args.home)
    record("A2_workspace_set_home", isinstance(setr, dict) and "error" in setr,
           f"workspace.set <home>: {'refused ' + json.dumps(setr.get('error'))[:160] if isinstance(setr, dict) and 'error' in setr else 'accepted ' + json.dumps(setr.get('result') if isinstance(setr, dict) else setr)}", setr)
    if isinstance(setr, dict) and "result" in setr:
        first, final = call_approved({"kind": "mcp", "name": "filesystem", "fn": "list_directory", "input": {"path": "."}})
        listing = json.dumps(final.get("result") if isinstance(final, dict) else final)
        record("A2_files_tool_in_home_workspace", "session.token" not in listing, f"with <home> as workspace the listing has session.token: {'session.token' in listing}", final)
        c.cep("workspace.set", path=os.path.join(args.home, "workspace"))

    # A1: flows.run_get while a step runs
    run = c.cep("flows.run", name="run49-check")
    rid = None
    if isinstance(run, dict) and "result" in run:
        res = run["result"]
        rid = res.get("run_id") or (res.get("run") or {}).get("id") or (res.get("task") or {}).get("run_id")
    seen = []
    for _ in range(12):
        time.sleep(1)
        got = c.cep("flows.run_get", run_id=rid) if rid else None
        steps = ((got or {}).get("result") or {}).get("steps") or []
        seen.append([(s.get("id"), s.get("kind"), s.get("status")) for s in steps])
        if any(s.get("status") == "running" for s in steps):
            break
    running = [s for snap in seen for s in snap if s[2] == "running"]
    record("A1_flows_run_get_running_step", running and all(s[0] for s in running), f"run {rid}; running steps seen {running}; snapshots {seen[-2:]}", {"run": run, "last": got})
    for _ in range(60):
        got = c.cep("flows.run_get", run_id=rid) if rid else None
        status = (((got or {}).get("result") or {}).get("run") or {}).get("status")
        if status in ("succeeded", "failed", "cancelled"):
            break
        time.sleep(1)
    record("A1_flow_run_completes", status == "succeeded", f"final status {status}", got)


if __name__ == "__main__":
    main()
