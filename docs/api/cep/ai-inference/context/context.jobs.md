---
method: "context.jobs"
transport: "POST /v1/cep"
category: "ai-inference"
category_title: "AI & Inference Core"
group: "context"
summary: "The idle-time scheduler's queue, counts and recent results"
scope: "read"
execution: "sync"
mcp: true
owner_only: false
destructive: false
idempotent: true
params: []
request_payload: |
  {
    "id": 1,
    "jsonrpc": "2.0",
    "method": "context.jobs",
    "params": {}
  }
response: |
  {
    "id": 1,
    "jsonrpc": "2.0",
    "result": {
      "counts": {
        "done": 12,
        "failed": 0,
        "skipped": 0,
        "stopped": 0
      },
      "indexer": "auto",
      "queue": [
        {
          "answer_pos": 3,
          "chat_id": "c1",
          "kind": "cold"
        }
      ],
      "recent": [
        {
          "answer_pos": 5,
          "chat": "c2",
          "cold": false,
          "ms": 8017,
          "outcome": "done"
        }
      ],
      "requests_in_flight": 0,
      "running": false
    }
  }
errors: []
---

# context.jobs

The idle-time scheduler: while no chat request runs, the model writes a short card for each answered turn (title, motive, summary, keywords) so recall and the Chat Map get better. A request that arrives stops the job at once.

## Result

| Field | Meaning |
|---|---|
| `indexer` | The `config` context setting: `auto`, `always`, `rules_only` or `off` |
| `requests_in_flight` | Chat requests running now (jobs wait for 0) |
| `running` | Whether a job runs now |
| `queue[]` | `chat_id`, `answer_pos`, `kind` (`hot`: the chat just answered; `cold`: an older turn found by the scan) |
| `counts` | `done`, `failed`, `stopped`, `skipped` since the Engine started |
| `recent[]` | The newest results: `chat`, `answer_pos`, `cold`, `outcome`, `ms` |

## Example

```bash
curl -s http://127.0.0.1:8000/v1/cep -H "Content-Type: application/json" \
  -H "Authorization: Bearer $TOKEN" \
  -d '{"jsonrpc":"2.0","id":1,"method":"context.jobs","params":{}}'
```

Leave out the `Authorization` header when API authentication is off.
