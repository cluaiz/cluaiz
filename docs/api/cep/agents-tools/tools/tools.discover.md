---
method: "tools.discover"
transport: "POST /v1/cep"
category: "agents-tools"
category_title: "Agents, MCP & Tools"
group: "tools"
summary: "Dry-run the existing installed-tool matcher"
scope: "read"
execution: "sync"
mcp: true
owner_only: false
destructive: false
idempotent: true
params:
  - name: "explain"
    type: "boolean"
    req: false
    desc: "Also answer `evidence`: each matching tool's score, matched words and where they matched"
    example: null
  - name: "previous"
    type: "string | null"
    req: false
    desc: "The message before it (follow-ups such as \"and in Hindi?\" use it)"
    example: null
  - name: "query"
    type: "string"
    req: true
    desc: "A user message, as the chat would see it"
    example: "read notes.txt from my project"
request_payload: |
  {
    "id": 1,
    "jsonrpc": "2.0",
    "method": "tools.discover",
    "params": {
      "query": "read notes.txt from my project"
    }
  }
response: |
  {
    "id": 1,
    "jsonrpc": "2.0",
    "result": {
      "actionable": true,
      "candidates": [
        {
          "category": "mcp",
          "id": "filesystem",
          "level": "card",
          "load_cost_tokens": 1650,
          "matched": [
            "read",
            "project",
            "file"
          ],
          "score": 9.56,
          "summary": "Official Model Context Protocol secure file system bridge"
        },
        {
          "category": "mcp",
          "id": "pynotes",
          "level": "title",
          "load_cost_tokens": 55,
          "matched": [
            "read"
          ],
          "score": 3.58,
          "summary": "Test notes server"
        }
      ],
      "installed": 10,
      "query": "read notes.txt from my project"
    }
  }
errors: []
---

# tools.discover

A dry run of the tool matcher: which installed tools the chat would suggest for a message, without loading or starting anything. Use it to see why a tool is or is not suggested.

## What it does

1. Builds the catalog of installed, enabled tools (names, descriptions, functions, triggers).
2. Scores each against `query` (and `previous`, so a follow-up like "and in Hindi?" keeps its subject). A file name or link in the message counts as a file or a web address, not by its words.
3. Answers the candidates best first.

## Result

| Field | Meaning |
|---|---|
| `actionable` | Whether the message asks for something a tool could do (a greeting is not) |
| `candidates[]` | `id`, `category`, `score`, `matched` words, `level` (`card`: shown with its description; `title`: only its name), `load_cost_tokens`, `summary` |
| `installed` | How many tools were considered |
| `evidence` | With `explain: true`: for every tool with any match, its score, matched words, and whether the match was on its name, a function or a trigger |

## Example

```bash
curl -s http://127.0.0.1:8000/v1/cep -H "Content-Type: application/json" \
  -H "Authorization: Bearer $TOKEN" \
  -d '{"jsonrpc":"2.0","id":1,"method":"tools.discover","params":{"query":"read notes.txt from my project","explain":true}}'
```

Leave out the `Authorization` header when API authentication is off.
