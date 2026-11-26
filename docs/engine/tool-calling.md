<!-- name: Call your own functions (tool calling)
description: Give the model your functions in an OpenAI-style request; the Engine returns the call, you run it and send the result back for the final answer. -->

# Call your own functions

The model can ask to use a function you define, such as `get_weather`. The Engine does not run it: it
returns the call to you, you run it, and you send the result back. Then the model writes the answer.

This is the same request shape as OpenAI's function calling, so an OpenAI client works unchanged.

## Example

Send your function in `tools`:

```bash
curl http://127.0.0.1:8000/v1/chat/completions -H "Content-Type: application/json" -d '{
  "model": "qwen3.5-2b-mtp-gguf-Q4_K_M",
  "messages": [{"role": "user", "content": "What is the weather in Paris right now? Use the get_weather tool."}],
  "tools": [{"type": "function", "function": {
    "name": "get_weather",
    "description": "Current weather for a city.",
    "parameters": {"type": "object", "properties": {"city": {"type": "string"}}, "required": ["city"]}
  }}],
  "temperature": 0, "max_tokens": 128, "think_mode": "off"
}'
```

The answer is the call, not text (`finish_reason: "tool_calls"`):

```json
"tool_calls": [{
  "id": "call_1795519888034",
  "type": "function",
  "function": {"name": "get_weather", "arguments": "{\"city\":\"Paris\"}"}
}]
```

Run your function with those arguments, then send the conversation again with the call and its result:

```json
{"role": "assistant", "tool_calls": [ ...the call above... ]},
{"role": "tool", "tool_call_id": "call_1795519888034", "content": "{\"temp_c\": 18, \"sky\": \"clear\"}"}
```

The model now answers in `content`.

Recorded in run 80 (Qwen3-0.6B Q4_K_M, RTX 3050 laptop): a weather question with `get_weather` returned a
call in 10 of 10 tries. The weather values in the tool message are only an example.

## Choosing when the model calls

| `tool_choice` | What happens |
|---|---|
| `"auto"` (default) | the model decides |
| `"none"` | no call is asked for; a call the model writes anyway comes back as text in `content` |
| `"required"` | the first answer must be a call |
| `{"type": "function", "function": {"name": "X"}}` | only X is offered and must be called; X not in `tools` is HTTP 400 |

## Good to know

- One call per answer is returned.
- Streaming: the call arrives as `delta.tool_calls`, then `finish_reason: "tool_calls"`.
- `max_tokens` (or `max_completion_tokens`) is one budget for the whole turn, tool steps included. When it
  runs out, the reply ends with `finish_reason: "length"`, and a call cut in half is not returned.
- `response_format` together with your `tools`: calls keep working, but the JSON shape is not enforced.
  Send `response_format` without `tools` when you need the shape held.
- `grammar` together with your `tools` is refused (HTTP 400), as llama-server does.

Every field: [Chat completions reference](../api/inference/chat_completions.md).

## How it is built

The chat pipeline and each decision behind these rules, with the runs that prove them:
[api/src/handlers/chat/README.md](../../api/src/handlers/chat/README.md).
