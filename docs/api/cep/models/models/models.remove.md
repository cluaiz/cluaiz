---
method: "models.remove"
transport: "POST /v1/cep"
category: "models"
category_title: "Models & Engine Vault"
group: "models"
summary: "Unloads the selected model if active and removes its installed package"
scope: "write"
execution: "sync"
mcp: true
owner_only: false
destructive: true
idempotent: false
params:
  - name: "model"
    type: "string"
    req: true
    desc: "An installed model id (from `models.list`)"
    example: "installed-model"
request_payload: |
  {
    "id": 1,
    "jsonrpc": "2.0",
    "method": "models.remove",
    "params": {
      "model": "installed-model"
    }
  }
response: |
  {
    "id": 1,
    "jsonrpc": "2.0",
    "result": {
      "dir": "models/chat/installed-model",
      "model": "installed-model"
    }
  }
errors:
  - code: -32004
    type: "model_not_found"
    when: "No installed model with this id, or its weight file is missing"
  - code: -32602
    type: "invalid_params"
    when: "`model` is empty or looks like a path"
  - code: -32002
    type: "forbidden"
    when: "The model's folder is outside the caller's roots (`data.needs: \"roots\"`)"
  - code: -32010
    type: "model_remove_failed"
    when: "The folder or the registry entry could not be removed"
---

# models.remove

Removes an installed model: unloads it when it is the loaded model, deletes its folder, and removes it from the registry. `cluaiz rm` does the same.

## What it does

1. Finds the model and checks its folder against the caller's roots.
2. When the loaded model's file is inside that folder, unloads it first.
3. Deletes the folder and the registry entry. Answers the model id and the folder it removed.

Files the model shares with nothing else are gone after this; to use the model again, download it again ([`models.download`](models.download.md)).

## Example

```bash
curl -s http://127.0.0.1:8000/v1/cep -H "Content-Type: application/json" \
  -H "Authorization: Bearer $TOKEN" \
  -d '{"jsonrpc":"2.0","id":1,"method":"models.remove","params":{"model":"old-model-Q4_K_M"}}'
```

Leave out the `Authorization` header when API authentication is off.
