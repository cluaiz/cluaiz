---
name: agent.final_answer
description: "Added as the last user message when a chat answer reached its tool step limit or kept repeating the same tool call. The Engine then asks the model once more, with no tools offered, and returns that reply as the answer."
variables: []
---
No more tool calls are possible for this answer. Answer my request now in plain text, using only what the tool results above already show. If something I asked for is still missing, say what is missing.
