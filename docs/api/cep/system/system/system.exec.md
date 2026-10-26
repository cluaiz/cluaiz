---
method: "system.exec"
transport: "POST /v1/cep"
category: "system"
category_title: "System & Infrastructure"
group: "system"
summary: "Runs a program on the host and returns its output; the owner approves each call"
scope: "admin"
execution: "sync"
mcp: false
owner_only: false
destructive: true
idempotent: false
params:
  - name: "args"
    type: "string[]"
    req: false
    desc: "Its arguments, each passed as it is"
    example: ["--version"]
  - name: "command"
    type: "string | null"
    req: false
    desc: "One shell line instead of `program`; only when permission.json `exec.allow_shell` is on"
    example: null
  - name: "program"
    type: "string | null"
    req: false
    desc: "The program to run (a name on PATH or a path); not passed through a shell"
    example: "git"
request_payload: |
  {
    "id": 1,
    "jsonrpc": "2.0",
    "method": "system.exec",
    "params": {
      "args": [
        "--version"
      ],
      "program": "git"
    }
  }
response: |
  {
    "id": 1,
    "jsonrpc": "2.0",
    "result": {
      "exit_code": 0,
      "stderr": "",
      "stdout": "git version 2.47.0\n"
    }
  }
errors:
  - code: -32003
    type: "approval_required"
    when: "Every call, until the owner approves it (`data.approval_id`); send it again with `_meta.approval`"
  - code: -32602
    type: "invalid_params"
    when: "Both or neither of `program` / `command`, `args` with `command`, or a shell line while `exec.allow_shell` is off"
  - code: -32006
    type: "too_large"
    when: "The command is over 8192 characters"
  - code: -32010
    type: "process_failed"
    when: "The program could not be started (not found, not executable)"
---

# system.exec

Runs a program on the host and answers its exit code and output. The owner approves every call, and by default only a program with its arguments runs, not a shell line.

## What it does

1. Reads the owner's `exec` policy from `permission.json` (`config` section `engine`): `allow_shell` and `without_approval`, both `false` by default.
2. Checks the params: exactly one of `program` (with optional `args`) or `command`. `command` is refused unless `allow_shell` is on. Program and arguments together may be at most 8192 characters.
3. Unless `without_approval` is on, asks the owner: the first call answers `-32003` with `data.approval_id`. After [`approvals.approve`](../../agents-tools/approvals/approvals.approve.md), the same call sent again with `"_meta": {"approval": "<id>"}` runs. An approval is used once.
4. Runs the program and waits for it to end. `program` + `args` start it directly (no shell, so no quoting or expansion); `command` runs through `cmd /C` on Windows and `sh -c` elsewhere.
5. Answers `exit_code` (`null` when the process was killed by a signal), `stdout` and `stderr` as text.

## Who may call it

`admin` scope: the owner over `/v1/cep` or the C library. It is never an MCP tool. Every call is written to the audit log ([`system.audit`](system.audit.md)).

## Example

First call (asks), then the approval, then the same call with the approval:

```bash
curl -s http://127.0.0.1:8000/v1/cep -H "Content-Type: application/json" \
  -H "Authorization: Bearer $TOKEN" \
  -d '{"jsonrpc":"2.0","id":1,"method":"system.exec","params":{"program":"git","args":["--version"]}}'
```

Leave out the `Authorization` header when API authentication is off.

```json
{"jsonrpc":"2.0","id":2,"method":"approvals.approve","params":{"approval_id":"ap_41c2…"}}
{"jsonrpc":"2.0","id":3,"method":"system.exec","params":{"program":"git","args":["--version"],"_meta":{"approval":"ap_41c2…"}}}
```
