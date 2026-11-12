---
method: "flows.get"
transport: "POST /v1/cep"
category: "automation"
category_title: "Automation"
group: "flows"
draft: true
summary: "One flow: its summary, its file as text and as JSON, and its triggers' state"
scope: "read"
execution: "sync"
mcp: true
owner_only: false
destructive: false
idempotent: true
params:
  - name: "as"
    type: "\"both\" | \"text\" | \"definition\" | null"
    req: false
    desc: "What to return besides the summary (default both)"
    example: "both"
  - name: "name"
    type: "string"
    req: true
    desc: "The flow's name"
    example: "weekly-issues"
  - name: "scope"
    type: "\"project\" | \"user\" | null"
    req: false
    desc: "Which flow of that name. Absent: the project's, then the user's"
    example: "user"
request_payload: |
  {
    "id": 1,
    "jsonrpc": "2.0",
    "method": "flows.get",
    "params": {
      "name": "weekly-issues"
    }
  }
response: |
  {
    "id": 1,
    "jsonrpc": "2.0",
    "result": {
      "flow": {
        "name": "weekly-issues",
        "scope": "user",
        "path": "~/.cluaiz/flows/weekly-issues.flow.yaml",
        "format": "yaml",
        "description": "Every Friday evening, a short report of the issues closed this week.",
        "enabled": true,
        "triggers": [
          {
            "id": "schedule_0",
            "kind": "schedule",
            "status": "active",
            "next_at": "2026-11-13T18:00:00+05:30"
          }
        ],
        "hash": "sha256:4b1e9c…",
        "updated_at": "2026-11-10T17:40:02+05:30",
        "last_run": {
          "run_id": "run_8f2c51d0",
          "status": "succeeded",
          "finished_at": "2026-11-06T18:00:41+05:30"
        },
        "warnings": 0,
        "tags": [
          "github",
          "reports"
        ]
      },
      "format": "yaml",
      "text": "flow: cluaiz.flow/1\nname: weekly-issues\n…",
      "definition": {
        "flow": "cluaiz.flow/1",
        "name": "weekly-issues",
        "on": [
          {
            "schedule": "0 18 * * 5"
          }
        ],
        "steps": [
          {
            "id": "closed",
            "tool": "mcp:github/list_issues",
            "with": {
              "repo": "my/repo",
              "state": "closed",
              "closed_in_last_days": 7
            }
          },
          {
            "id": "report",
            "ask": "Write a short weekly report of these closed issues.",
            "with": {
              "issues": "${{ steps.closed.output }}"
            }
          },
          {
            "tool": "mcp:notify/send",
            "with": {
              "text": "${{ steps.report.output }}"
            }
          }
        ]
      },
      "triggers": [
        {
          "id": "schedule_0",
          "kind": "schedule",
          "status": "active",
          "next_at": "2026-11-13T18:00:00+05:30",
          "last_fired_at": "2026-11-06T18:00:00+05:30",
          "last_error": null,
          "skipped": 0
        }
      ]
    }
  }
errors:
  - code: -32004
    type: "flow_not_found"
    when: "No flow with this name in the scope (or in either scope when `scope` is absent)"
---

# flows.get

> **Built in the first release of Automation (draft).** Names and fields may still change before a release that has them. See [Automation](../README.md).

Returns a saved flow. `text` is the file exactly as stored (YAML keeps its comments); `definition` is the same flow as JSON. Trigger states show whether each trigger is active, paused for permissions, or failing (a `poll` that cannot reach its tool).

## Endpoint, execution and permissions

| Property | Contract |
|---|---|
| Wire endpoint | `POST /v1/cep`; logical method `flows.get` |
| Execution | Synchronous: the result is in the JSON-RPC `result` |
| Required scope | `read` |
| MCP facade | Listed for MCP clients (read-only) |
| Called from a flow (`call` step) | Yes |
| Side effects | None (read-only) |

Send a Bearer token when API authentication is enabled. Authentication failures are HTTP 401; method errors are JSON-RPC error objects with HTTP 200. See [the common CEP contract](../../api/cep/README.md).

## How it works

1. Find the flow by `name` and `scope`
2. Read the file, parse it, and attach trigger states and the last run

```mermaid
flowchart TD
    C["Client request"] --> A["Authentication, principal and typed params"]
    A --> P0["Find the flow by name and scope"]
    P0 --> P1["Read the file, parse it, and attach trigger states and the last run"]
    P1 --> R["JSON-RPC result or explicit error"]
```

## Parameters, defaults and value effects

| Parameter | Type | Required | Meaning, default and constraints |
|---|---|---|---|
| `name` | string | Yes | The flow's name |
| `scope` | "project" \| "user" \| null | No | Which flow of that name. Absent: the project's, then the user's |
| `as` | "both" \| "text" \| "definition" \| null | No | What to return besides the summary (default both) |

Unknown parameters are rejected (`-32602`). `params._meta` is protocol metadata (such as an approval id), not an option of this method.

## Request examples

Examples describe the contract. Replace names, ids and paths with your own.

### 1. By name

The project's flow first, then the user's.

```json
{
  "jsonrpc": "2.0",
  "id": 1,
  "method": "flows.get",
  "params": {
    "name": "weekly-issues"
  }
}
```

### 2. Text only

For an editor that shows the file.

```json
{
  "jsonrpc": "2.0",
  "id": 2,
  "method": "flows.get",
  "params": {
    "name": "weekly-issues",
    "scope": "user",
    "as": "text"
  }
}
```

## Response and field meanings

An illustrative answer; ids, times and contents will differ:

```json
{
  "id": 1,
  "jsonrpc": "2.0",
  "result": {
    "flow": {
      "name": "weekly-issues",
      "scope": "user",
      "path": "~/.cluaiz/flows/weekly-issues.flow.yaml",
      "format": "yaml",
      "description": "Every Friday evening, a short report of the issues closed this week.",
      "enabled": true,
      "triggers": [
        {
          "id": "schedule_0",
          "kind": "schedule",
          "status": "active",
          "next_at": "2026-11-13T18:00:00+05:30"
        }
      ],
      "hash": "sha256:4b1e9c…",
      "updated_at": "2026-11-10T17:40:02+05:30",
      "last_run": {
        "run_id": "run_8f2c51d0",
        "status": "succeeded",
        "finished_at": "2026-11-06T18:00:41+05:30"
      },
      "warnings": 0,
      "tags": [
        "github",
        "reports"
      ]
    },
    "format": "yaml",
    "text": "flow: cluaiz.flow/1\nname: weekly-issues\n…",
    "definition": {
      "flow": "cluaiz.flow/1",
      "name": "weekly-issues",
      "on": [
        {
          "schedule": "0 18 * * 5"
        }
      ],
      "steps": [
        {
          "id": "closed",
          "tool": "mcp:github/list_issues",
          "with": {
            "repo": "my/repo",
            "state": "closed",
            "closed_in_last_days": 7
          }
        },
        {
          "id": "report",
          "ask": "Write a short weekly report of these closed issues.",
          "with": {
            "issues": "${{ steps.closed.output }}"
          }
        },
        {
          "tool": "mcp:notify/send",
          "with": {
            "text": "${{ steps.report.output }}"
          }
        }
      ]
    },
    "triggers": [
      {
        "id": "schedule_0",
        "kind": "schedule",
        "status": "active",
        "next_at": "2026-11-13T18:00:00+05:30",
        "last_fired_at": "2026-11-06T18:00:00+05:30",
        "last_error": null,
        "skipped": 0
      }
    ]
  }
}
```

| Field | Meaning |
|---|---|
| `flow` | The summary (see `flows.list`) |
| `format` | `json` or `yaml` |
| `text` | The file's text |
| `definition` | The flow as JSON |
| `triggers[]` | `status`: `active`, `disabled`, `waiting_for_permission`, `error`; `next_at` for time triggers; `last_error` (such as a failing poll); `skipped` events dropped by `concurrency` or `filter` |

## Errors and recovery

| Code / boundary | Meaning | Recovery |
|---|---|---|
| HTTP 401 | Missing or wrong token while authentication is on | Send the configured token |
| `-32602` / `invalid_params` | Missing required fields, wrong types or unknown fields | Correct the request; do not retry it unchanged |
| `-32002` / `forbidden` | The caller lacks the scope, owner role or roots; `data.needs` says which | Use an authorized caller |
| `-32004` / `flow_not_found` | No flow with this name in the scope (or in either scope when `scope` is absent) | List flows with `flows.list`; check `scope` |

### Invalid-field example

Deliberately invalid: the extra field must be removed.

```text
{
  "jsonrpc": "2.0",
  "id": 90,
  "method": "flows.get",
  "params": {
    "name": "weekly-issues",
    "__unknown_option": true
  }
}
```

The answer is `{"jsonrpc":"2.0","id":90,"error":{"code":-32602,"message":"...","data":{"type":"invalid_params","errors":[...]}}}`. Use the code and `data`, not the message text.

## cURL

Save one request above as `request.json`:

```bash
curl --fail-with-body http://127.0.0.1:8000/v1/cep \
  -H "Content-Type: application/json" \
  -H "Authorization: Bearer $CLUAIZ_TOKEN" \
  --data-binary @request.json
```

## Python

```python
import json
import os
import requests

payload = json.loads(r'''{
  "id": 1,
  "jsonrpc": "2.0",
  "method": "flows.get",
  "params": {
    "name": "weekly-issues"
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

Use an owner token only in a trusted client.

```javascript
const payload = {
  "id": 1,
  "jsonrpc": "2.0",
  "method": "flows.get",
  "params": {
    "name": "weekly-issues"
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

Read-only. Secrets are never in a flow file, so the text can be shown as it is.

- [`flows.list`](flows.list.md): every flow
- [`flows.save`](flows.save.md): change it
- [Automation](../README.md)

## Implementation reference

- Code: `automation/host/src/methods/files.rs` (the method) and `automation/core` (the format, checks and runner).
