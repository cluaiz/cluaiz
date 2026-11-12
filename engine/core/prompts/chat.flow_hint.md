---
name: chat.flow_hint
description: "In the <tools_hint> block, when saved flows fit the message and are offered as tools."
variables: [flows]
---
Saved flows that fit this message are ready to call as tools, without load_tool: {{flows}}. Call one once when the user asks for what it does.
