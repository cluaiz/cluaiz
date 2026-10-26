---
method: "memory.search"
transport: "POST /v1/cep"
category: "ai-inference"
category_title: "AI & Inference Core"
group: "memory"
summary: "Messages of a chat that fit a query, or text from the other chats"
scope: "read"
execution: "sync"
mcp: true
owner_only: false
destructive: false
idempotent: true
params:
  - name: "chat_id"
    type: "string"
    req: true
    desc: "The chat: the same value as `session_id` in chat completions"
    example: "c1"
  - name: "limit"
    type: "integer | null"
    req: false
    desc: "Most results (default 5; at most 50 in a chat, 20 across chats)"
    example: null
  - name: "query"
    type: "string"
    req: true
    desc: "Words to look for (matched against the cards' titles, keywords and text)"
    example: "deploy port"
  - name: "scope"
    type: "string | null"
    req: false
    desc: "`chat` (default), or `workspace` / `all` to search the other chats (not with cross_chat: deny)"
    example: null
request_payload: |
  {
    "id": 1,
    "jsonrpc": "2.0",
    "method": "memory.search",
    "params": {
      "chat_id": "c1",
      "query": "deploy port"
    }
  }
response: |
  {
    "id": 1,
    "jsonrpc": "2.0",
    "result": {
      "chat_id": "c1",
      "query": "deploy port",
      "results": [
        {
          "card": {},
          "ref": "m:12",
          "score": 3.2,
          "why": [
            "port"
          ]
        }
      ]
    }
  }
errors:
  - code: -32002
    type: "forbidden"
    when: "`scope` workspace / all while `config` context `cross_chat` is deny (`data.needs: \"cross_chat\"`)"
  - code: -32602
    type: "invalid_params"
    when: "`scope` is not chat, workspace or all"
  - code: -32010
    type: "event_log_unreadable"
    when: "The chat's event log could not be read"
---

# memory.search

Finds the messages of a chat that fit a query, with the reasons they matched. With `scope` `workspace` or `all`, it searches the other chats instead.

A chat is named by `chat_id`: the same string a client sends as `session_id` in its `/v1/chat/completions` requests.

## In one chat (`scope: chat`, the default)

Every message of the chat is a candidate. The cards' titles, keywords, summaries and text are scored against `query`; the best `limit` (default 5, at most 50) come back as `ref` (`m:<position>`, for [`memory.item`](memory.item.md)), `score`, `why` (the matched words) and the `card`.

Time words in the query ("kal", "parso", "Monday", "23 Oct", "kal raat", "3 ghante pehle", "last week") limit the search to that window, in the user's own days.

## Across chats (`scope: workspace` / `all`)

The search runs over the other chats instead: chats written to in the last 90 days, at most 50, newer first; `workspace` keeps only chats used in the current workspace. The answer is `text`, one line per message (`[chat <id> · m:<pos> · time · role] …`), at most 20 results.

The owner's `config` context setting `cross_chat` decides: `deny` refuses with `-32002` (`data.needs: "cross_chat"`). `ask` and `allow` answer; the `ask` prompt is for the model's `recall` tool inside a chat, not for this direct call.

## Example

```bash
curl -s http://127.0.0.1:8000/v1/cep -H "Content-Type: application/json" \
  -H "Authorization: Bearer $TOKEN" \
  -d '{"jsonrpc":"2.0","id":1,"method":"memory.search","params":{"chat_id":"c1","query":"kal deploy port kya tha"}}'
```

Leave out the `Authorization` header when API authentication is off.
