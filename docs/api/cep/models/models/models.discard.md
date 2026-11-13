---
method: "models.discard"
transport: "POST /v1/cep"
category: "models"
category_title: "Models & Engine Vault"
group: "models"
summary: "Deletes what a stopped models.download left (its `.part` and finished files), so the next download starts over; an installed model is never touched"
scope: "write"
execution: "sync"
mcp: true
owner_only: false
destructive: true
idempotent: true
params:
  - name: "model"
    type: "string"
    req: true
    desc: "A catalog id, or a Hugging Face repo `owner/name` (optionally `:QUANT`, an `hf://` or https URL)"
    example: "owner/name-GGUF:Q4_K_M"
request_payload: |
  {
    "id": 1,
    "jsonrpc": "2.0",
    "method": "models.discard",
    "params": {
      "model": "owner/name-GGUF:Q4_K_M"
    }
  }
response: |
  {
    "id": 1,
    "jsonrpc": "2.0",
    "result": {
      "model": "owner/name-GGUF:Q4_K_M",
      "removed": "models/chat/name-GGUF-Q4_K_M"
    }
  }
errors:
  - code: -32602
    type: "invalid_params"
    when: "`model` is empty, over 512 characters, or has control characters"
  - code: -32005
    type: "provider_model"
    when: "The model is a provider's (`<provider>/<model>` in providers.json); this method works on downloads only"
  - code: -32005
    type: "pull_running"
    when: "This model is still downloading; stop it with `tasks.cancel` first"
  - code: -32010
    type: "discard_failed"
    when: "Resolving the model, or deleting its files, failed (the folder holds an installed model, or a file is in use)"
---

# models.discard

Deletes what a stopped `models.download` left on disk: the `.part` file and any finished files of that download. Use it to cancel a paused download; downloading the same `model` again then starts from the beginning.

## Endpoint, execution and permissions

| Property | Contract |
|---|---|
| Wire endpoint | `POST /v1/cep`; logical method `models.discard` |
| Execution | Immediate |
| Required scope | `write` |
| Side effects | Deletes the model's download folder in the vault |

Send a Bearer token when API authentication is enabled. See [the common CEP contract](../../README.md).

## How it works

1. Validate `model` and refuse a provider model.
2. Wait up to 10 seconds for a download of the same `model` that is still stopping; one still running is refused with `pull_running`.
3. Resolve `model` as `models.download` does. An installed model, or one already on disk, is never touched: the answer is `"removed": null`.
4. Delete the vault folder the download used, unless an installed model is registered in it.

```mermaid
flowchart TD
    C["Client request"] --> V["Validate model"]
    V --> W{"Download of it still running?"}
    W -- "stops within 10 s" --> R["Resolve as models.download does"]
    W -- "still running" --> E["pull_running"]
    R --> I{"Installed or on disk?"}
    I -- yes --> N["removed: null"]
    I -- no --> D["Delete its download folder"] --> F["removed: folder"]
```

## Parameters

| Parameter | Type | Required | Meaning |
|---|---|---|---|
| `model` | string | Yes | The same reference given to `models.download` (catalog id, or `owner/name[:QUANT]`) |

## Request example

```json
{
  "jsonrpc": "2.0",
  "id": 1,
  "method": "models.discard",
  "params": {
    "model": "owner/name-GGUF:Q4_K_M"
  }
}
```

## Response

`removed` is the folder deleted, or `null` when nothing was left or the model is installed.

```json
{
  "jsonrpc": "2.0",
  "id": 1,
  "result": {
    "model": "owner/name-GGUF:Q4_K_M",
    "removed": "models/chat/name-GGUF-Q4_K_M"
  }
}
```

## Errors and recovery

| Code / type | Meaning | Recovery |
|---|---|---|
| `-32602` / `invalid_params` | `model` is empty, too long or has control characters | Send the reference used for `models.download` |
| `-32005` / `provider_model` | The model is a provider's | Providers have nothing to download |
| `-32005` / `pull_running` | The download is still running | Stop it with `tasks.cancel`, then call again |
| `-32010` / `discard_failed` | Resolving failed, the folder holds an installed model, or a file is in use | Read the message; use `models.remove` for an installed model |

