---
name: tools.call_format_hint
description: "In the <tools_hint> block, when the system prompt has no tool block: how to call a tool, the tools that can be called now and the folder they run in."
variables: [tools, workspace]
---
To call a function, reply with only:
<tool_call>
{"name": "function_name", "arguments": {"arg": "value"}}
</tool_call>
Functions you can call now:
{{tools}}
{{workspace}}
