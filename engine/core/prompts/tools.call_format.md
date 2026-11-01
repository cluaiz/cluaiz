---
name: tools.call_format
description: How such a model must write a tool call, after the <tools> block. The Engine reads <tool_call> blocks with "name" and "arguments"; keep that form.
variables: []
---
When you need to call a tool, you MUST output a <tool_call> XML block with a JSON object containing "name" and "arguments":
<tool_call>
{"name": "tool_name", "arguments": {"arg_name": "value"}}
</tool_call>
Do not output conversational filler text before or after the <tool_call> block.
