---
method: "tools.set_settings"
transport: "POST /v1/cep"
category: "agents-tools"
category_title: "Agents, MCP & Tools"
group: "tools"
summary: "Validate and save a tool settings patch"
scope: "write"
execution: "sync"
mcp: false
owner_only: false
destructive: false
idempotent: false
params:
  - name: "kind"
    type: "\"skill\" | \"plugin\" | \"mcp\""
    req: true
    desc: "skill, plugin or mcp"
    example: "skill"
  - name: "name"
    type: "string"
    req: true
    desc: "The tool's folder name: 1–64 lowercase letters, digits, dots, underscores or hyphens"
    example: "installed-tool"
  - name: "patch"
    type: "object"
    req: true
    desc: "Only the fields to change"
    example: {"enabled":true}
    fields:
      - name: "enabled"
        type: "boolean | null"
        req: false
        desc: "false: the tool is not offered to the model (an MCP server is also stopped)"
        example: true
      - name: "execution_mode"
        type: "string | null"
        req: false
        desc: "auto, manual or confirm_destructive"
        example: null
      - name: "security_mode"
        type: "string | null"
        req: false
        desc: "inherit, require_approval or always_allow"
        example: null
      - name: "trust_annotations"
        type: "boolean | null"
        req: false
        desc: "Trust the MCP server's own read-only / destructive hints"
        example: null
request_payload: |
  {
    "id": 1,
    "jsonrpc": "2.0",
    "method": "tools.set_settings",
    "params": {
      "kind": "skill",
      "name": "installed-tool",
      "patch": {
        "enabled": true
      }
    }
  }
response: |
  {
    "id": 1,
    "jsonrpc": "2.0",
    "result": {
      "schema": {},
      "values": {
        "enabled": true
      }
    }
  }
errors:
  - code: -32004
    type: "tool_not_found"
    when: "No installed tool of this kind and name"
  - code: -32602
    type: "invalid_params"
    when: "An unknown `security_mode` / `execution_mode`, or a bad `name`"
  - code: -32010
    type: "tool_operation_failed"
    when: "The tool registry could not be saved"
---

# tools.set_settings

Validate and save a supported tool-policy patch while leaving omitted policy fields unchanged. This is a small typed policy contract rather than an arbitrary package JSON editor.

## Endpoint, execution and permissions

| Property | Contract |
|---|---|
| Wire endpoint | `POST /v1/cep`; logical method `tools.set_settings` |
| Execution | Synchronous domain result in the JSON-RPC result field |
| Required scope | `write` |
| MCP facade | Not exposed by the current read-only MCP facade |
| Side effects | Updates the state described below |

Send a Bearer token when API authentication is enabled. Authentication failures are HTTP 401; ordinary CEP method errors are JSON-RPC error objects, normally with HTTP 200. A successful HTTP status alone does not prove the operation succeeded. See [the common CEP contract](../../README.md).

## How it works

1. Validate optional execution/security enum values
2. Resolve/authorize target and update the existing tool-registry entry
3. Save; if an MCP package was disabled, stop its server

```mermaid
flowchart TD
    C["Client request"] --> A["Authentication, principal and typed params"]
    A --> P0["Validate optional execution/security enum values"]
    P0 --> P1["Resolve/authorize target and update the existing tool-registry entry"]
    P1 --> P2["Save; if an MCP package was disabled, stop its server"]
    P2 --> R["JSON-RPC result or explicit error"]
```

## Parameters, defaults and value effects

| Parameter | Type | Required | Meaning, default and constraints |
|---|---|---|---|
| `kind` | "skill" \| "plugin" \| "mcp" | Yes | skill, plugin or mcp Required enum: skill (instructions), plugin (WASM/native package), mcp (server package). Kind and name must identify the same installed entry. |
| `name` | string | Yes | The tool's folder name: 1–64 lowercase letters, digits, dots, underscores or hyphens Required package/registry name, 1–64 ASCII lowercase letters/digits/dots/underscores/hyphens; first character must be a letter or digit. Not a path or arbitrary repository URL. |
| `patch` | object | Yes | Only the fields to change Required object containing only supported fields; omitted members remain unchanged. |
| `patch.enabled` | boolean \| null | No | false: the tool is not offered to the model (an MCP server is also stopped) Optional boolean. false disables offering and also stops the named MCP server. |
| `patch.execution_mode` | string \| null | No | auto, manual or confirm_destructive auto, manual or confirm_destructive. |
| `patch.security_mode` | string \| null | No | inherit, require_approval or always_allow inherit, require_approval or always_allow. |
| `patch.trust_annotations` | boolean \| null | No | Trust the MCP server's own read-only / destructive hints Optional boolean deciding whether to trust server read-only/destructive annotations. |

Unknown top-level method parameters are rejected. `params._meta` is protocol metadata, not a domain option; approval retries must preserve the same method and domain parameters. Where a parameter is optional, omission follows the method-specific rules above rather than implying a universal zero or empty-string default.

## Request examples

Examples describe contracts, not measured execution. Replace IDs, paths and installed-package names with real values from your Engine. Do not execute write/delete/process examples against valuable data merely to read the documentation.

### 1. Basic request

This request uses the documented selection. Read the result semantics below before acting on it.

```json
{
  "jsonrpc": "2.0",
  "id": 1,
  "method": "tools.set_settings",
  "params": {
    "kind": "skill",
    "name": "installed-tool",
    "patch": {
      "enabled": true
    }
  }
}
```

### 2. Disable an installed MCP server

The owner stops this server; other package settings remain as saved.

```json
{
  "jsonrpc": "2.0",
  "id": 2,
  "method": "tools.set_settings",
  "params": {
    "kind": "mcp",
    "name": "installed-server",
    "patch": {
      "enabled": false
    }
  }
}
```

### 3. Require deliberate tool approvals

Policy names are exact accepted strings, not free-text descriptions.

```json
{
  "jsonrpc": "2.0",
  "id": 3,
  "method": "tools.set_settings",
  "params": {
    "kind": "plugin",
    "name": "installed-plugin",
    "patch": {
      "security_mode": "require_approval",
      "execution_mode": "confirm_destructive"
    }
  }
}
```

## Response and field meanings

schema/values are the same policy projection as tools.get_settings, after saving. schema remains {}, not a generated per-plugin custom configuration schema.

The following response is an illustrative shape. IDs, byte counts, timings and array contents depend on the actual request and machine; shortened nested objects are not a second schema or a measured result.

```json
{
  "id": 1,
  "jsonrpc": "2.0",
  "result": {
    "schema": {},
    "values": {
      "enabled": true
    }
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
| `-32602` / `invalid_params` | An unknown `security_mode` / `execution_mode`, or a bad `name` | Correct the option/value combination; install required assets where relevant |
| `-32010` / `tool_operation_failed` | The tool registry could not be saved | Read the error message, correct the backend/filesystem/configuration condition, then retry deliberately |

### Invalid-field example

This is deliberately invalid. The extra field must be removed; it is not an alternate feature toggle.

```text
{
  "jsonrpc": "2.0",
  "id": 90,
  "method": "tools.set_settings",
  "params": {
    "kind": "skill",
    "name": "installed-tool",
    "patch": {
      "enabled": true
    },
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
  "method": "tools.set_settings",
  "params": {
    "kind": "skill",
    "name": "installed-tool",
    "patch": {
      "enabled": true
    }
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
  "method": "tools.set_settings",
  "params": {
    "kind": "skill",
    "name": "installed-tool",
    "patch": {
      "enabled": true
    }
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

Unknown typed patch fields or enum strings fail; this method does not install tools, change global Engine permission roots or turn annotation hints into a security guarantee. Disabling stops an MCP process after registry persistence; arbitrary concurrent work is not transactionally rolled back. Package locations come from the existing registry/installer and pass caller roots. Relative file operations stay inside that package even for the unrestricted owner; absolute paths, .. and escaping symlinks are not accepted as assets.

- [`tools.get_settings`](tools.get_settings.md): use its separate documented contract.
- [`tools.list`](tools.list.md): use its separate documented contract.
- [`approvals.list`](../approvals/approvals.list.md): use its separate documented contract.

## Implementation reference

- [api/src/cep/methods/tools/catalog.rs](../../../../../api/src/cep/methods/tools/catalog.rs)
