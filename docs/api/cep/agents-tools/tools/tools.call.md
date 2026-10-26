---
method: "tools.call"
transport: "POST /v1/cep"
category: "agents-tools"
category_title: "Agents, MCP & Tools"
group: "tools"
summary: "Execute an installed tool using the existing ToolsEngine"
scope: "exec"
execution: "sync"
mcp: true
owner_only: false
destructive: false
idempotent: false
params:
  - name: "fn"
    type: "string | null"
    req: false
    desc: "The function to run (a plugin export or an MCP tool); the tool's default when absent"
    example: "run"
  - name: "input"
    type: "any"
    req: true
    desc: "The function's arguments, as JSON"
    example: {}
  - name: "kind"
    type: "\"skill\" | \"plugin\" | \"mcp\""
    req: true
    desc: "skill, plugin or mcp"
    example: "plugin"
  - name: "name"
    type: "string"
    req: true
    desc: "The tool's folder name: 1–64 lowercase letters, digits, dots, underscores or hyphens"
    example: "installed-tool"
request_payload: |
  {
    "id": 1,
    "jsonrpc": "2.0",
    "method": "tools.call",
    "params": {
      "fn": "run",
      "input": {},
      "kind": "plugin",
      "name": "installed-tool"
    }
  }
response: |
  {
    "id": 1,
    "jsonrpc": "2.0",
    "result": {
      "result": "tool output"
    }
  }
errors:
  - code: -32004
    type: "tool_not_found"
    when: "No installed tool of this kind and name"
  - code: -32602
    type: "invalid_params"
    when: "A bad `name`, or `fn` looks like a path"
  - code: -32002
    type: "forbidden"
    when: "The tool folder is outside the caller's roots (`data.needs: \"roots\"`)"
  - code: -32010
    type: "tool_operation_failed"
    when: "The installer, registry or tool itself failed (its message is passed on)"
---

# tools.call

Invoke one installed tool through existing ToolsEngine execution. The actual result/behavior depends on kind: MCP function calls, plugin execution, declarative runners or skill instruction content are different contracts.

## Endpoint, execution and permissions

| Property | Contract |
|---|---|
| Wire endpoint | `POST /v1/cep`; logical method `tools.call` |
| Execution | Synchronous domain result in the JSON-RPC result field |
| Required scope | `exec` |
| MCP facade | Not exposed by the current read-only MCP facade |
| Side effects | Updates the state described below |

Send a Bearer token when API authentication is enabled. Authentication failures are HTTP 401; ordinary CEP method errors are JSON-RPC error objects, normally with HTTP 200. A successful HTTP status alone does not prove the operation succeeded. See [the common CEP contract](../../README.md).

## How it works

1. Validate kind/name/function and authorize the package path
2. For a skill, verify the actual SKILL.md exists inside the package
3. Dispatch through existing ToolsEngine, then parse returned JSON when valid or keep the raw string

```mermaid
flowchart TD
    C["Client request"] --> A["Authentication, principal and typed params"]
    A --> P0["Validate kind/name/function and authorize the package path"]
    P0 --> P1["For a skill, verify the actual SKILL.md exists inside the package"]
    P1 --> P2["Dispatch through existing ToolsEngine, then parse returned JSON when valid or keep the raw string"]
    P2 --> R["JSON-RPC result or explicit error"]
```

## Parameters, defaults and value effects

| Parameter | Type | Required | Meaning, default and constraints |
|---|---|---|---|
| `fn` | string \| null | No | The function to run (a plugin export or an MCP tool); the tool's default when absent Optional exported function/MCP tool name; the owner default is used when omitted. Empty, .. or path-like function strings are invalid. |
| `input` | any | Yes | The function's arguments, as JSON Required JSON value passed as the selected function’s arguments. Use the function inputSchema from tools.list; there is no one universal input shape. |
| `kind` | "skill" \| "plugin" \| "mcp" | Yes | skill, plugin or mcp Required enum: skill (instructions), plugin (WASM/native package), mcp (server package). Kind and name must identify the same installed entry. |
| `name` | string | Yes | The tool's folder name: 1–64 lowercase letters, digits, dots, underscores or hyphens Required package/registry name, 1–64 ASCII lowercase letters/digits/dots/underscores/hyphens; first character must be a letter or digit. Not a path or arbitrary repository URL. |

Unknown top-level method parameters are rejected. `params._meta` is protocol metadata, not a domain option; approval retries must preserve the same method and domain parameters. Where a parameter is optional, omission follows the method-specific rules above rather than implying a universal zero or empty-string default.

## Request examples

Examples describe contracts, not measured execution. Replace IDs, paths and installed-package names with real values from your Engine. Do not execute write/delete/process examples against valuable data merely to read the documentation.

### 1. Basic request

This request uses the documented selection. Read the result semantics below before acting on it.

```json
{
  "jsonrpc": "2.0",
  "id": 1,
  "method": "tools.call",
  "params": {
    "fn": "run",
    "input": {},
    "kind": "plugin",
    "name": "installed-tool"
  }
}
```

### 2. Call a function discovered from a real MCP package

Replace package/function/arguments according to that installed server’s actual schema.

```json
{
  "jsonrpc": "2.0",
  "id": 2,
  "method": "tools.call",
  "params": {
    "kind": "mcp",
    "name": "installed-server",
    "fn": "read_text_file",
    "input": {
      "path": "notes.txt"
    }
  }
}
```

### 3. Read plain skill instructions through its owner

A content result is instructions, not a generated assistant answer or simulated successful execution.

```json
{
  "jsonrpc": "2.0",
  "id": 3,
  "method": "tools.call",
  "params": {
    "kind": "skill",
    "name": "installed-tool",
    "input": {}
  }
}
```

## Response and field meanings

result is the tool’s actual parsed JSON value, or a string for non-JSON output. It is not always {result:"..."}; CEP itself wraps that domain value under result. A plain skill with no execution manifest returns instruction text, not proof that a workflow was executed.

The following response is an illustrative shape. IDs, byte counts, timings and array contents depend on the actual request and machine; shortened nested objects are not a second schema or a measured result.

```json
{
  "id": 1,
  "jsonrpc": "2.0",
  "result": {
    "result": "tool output"
  }
}
```

## Errors and recovery

| Code / boundary | Meaning | Recovery |
|---|---|---|
| HTTP 401 | Missing/invalid token while authentication is enabled | Supply the configured token; a CEP params change cannot repair authentication |
| `-32602` / typed params | Missing required fields, wrong JSON types or unknown fields | Correct the request against this page; do not retry the same invalid payload |
| `-32002` / principal | Required scope/owner/root authorization is absent | Inspect `error.data.needs`; use an authorized caller or permitted target |
| `-32004` / `tool_not_found` | No installed tool of this kind and name | Refresh the relevant inventory/IDs and check whether the resource expired |
| `-32602` / `invalid_params` | A bad `name`, or `fn` looks like a path | Correct the option/value combination; install required assets where relevant |
| `-32002` / `forbidden` | The tool folder is outside the caller's roots (`data.needs: "roots"`) | Read the error message, correct the backend/filesystem/configuration condition, then retry deliberately |
| `-32010` / `tool_operation_failed` | The installer, registry or tool itself failed (its message is passed on) | Read the error message, correct the backend/filesystem/configuration condition, then retry deliberately |

### Invalid-field example

This is deliberately invalid. The extra field must be removed; it is not an alternate feature toggle.

```text
{
  "jsonrpc": "2.0",
  "id": 90,
  "method": "tools.call",
  "params": {
    "fn": "run",
    "input": {},
    "kind": "plugin",
    "name": "installed-tool",
    "__unknown_option": true
  }
}
```

The error envelope is `{"jsonrpc":"2.0","id":90,"error":{"code":-32602,"message":"...","data":{"type":"invalid_params", "errors":[...]}}}`. Exact message and field diagnostics are generated by typed deserialization; callers should use codes and structured data, not match a sentence.

## cURL

Save one valid request above as `request.json`:

```bash
curl --fail-with-body http://127.0.0.1:8000/v1/cep \
  -H "Content-Type: application/json" \
  -H "Authorization: Bearer $CLUAIZ_TOKEN" \
  --data-binary @request.json
```

## Python

Install `requests` in your client environment. This executes the basic request; edit `params` for a different valid case.

```python
import json
import os
import requests

payload = json.loads(r'''{
  "id": 1,
  "jsonrpc": "2.0",
  "method": "tools.call",
  "params": {
    "fn": "run",
    "input": {},
    "kind": "plugin",
    "name": "installed-tool"
  }
}''')
response = requests.post(
    "http://127.0.0.1:8000/v1/cep",
    headers={"Authorization": "Bearer " + os.environ["CLUAIZ_TOKEN"]},
    json=payload, timeout=60,
)
response.raise_for_status()
reply = response.json()
if "error" in reply:
    raise RuntimeError(json.dumps(reply["error"], ensure_ascii=False))
print(reply["result"])
```

## JavaScript / Node.js

Use an owner token only in a trusted client. Do not ship it inside a public web page.

```javascript
const payload = {
  "id": 1,
  "jsonrpc": "2.0",
  "method": "tools.call",
  "params": {
    "fn": "run",
    "input": {},
    "kind": "plugin",
    "name": "installed-tool"
  }
};
const response = await fetch("http://127.0.0.1:8000/v1/cep", {
  method: "POST",
  headers: { "content-type": "application/json", authorization: `Bearer ${process.env.CLUAIZ_TOKEN}` },
  body: JSON.stringify(payload),
});
const raw = await response.text();
if (!response.ok) throw new Error(`HTTP ${response.status}: ${raw}`);
const reply = JSON.parse(raw);
if (reply.error) throw new Error(JSON.stringify(reply.error));
console.log(reply.result);
```

## Limits, state and related operations

Exec scope is required; not advertised by the current read-only MCP facade. Package execution can have its own policy, environment setup, process startup, network or filesystem side effects. Scope/roots checks on the package are not a claim that all third-party code is sandboxed. Disabled packages fail. This method has no universal timeout/cancellation/streaming guarantee. Package locations come from the existing registry/installer and pass caller roots. Relative file operations stay inside that package even for the unrestricted owner; absolute paths, .. and escaping symlinks are not accepted as assets.

- [`tools.list`](tools.list.md): use its separate documented contract.
- [`tools.get_settings`](tools.get_settings.md): use its separate documented contract.
- [`chat.set_tools`](../../ai-inference/chat/chat.set_tools.md): use its separate documented contract.

## Implementation reference

- [api/src/cep/methods/tools/lifecycle.rs](../../../../../api/src/cep/methods/tools/lifecycle.rs)
- [agents/src/lib.rs](../../../../../agents/src/lib.rs)
