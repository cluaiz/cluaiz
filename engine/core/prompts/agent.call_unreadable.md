---
name: agent.call_unreadable
description: "Told to the model when its last answer had a tool call that could not be read (half a call, arguments that are not a JSON object, or markup its format does not have); nothing was run, and the model answers again."
variables: [call]
---
Your last tool call could not be read, so nothing was run:

{{call}}

Send the call again, complete, in your tool-call format and outside your thinking, with the arguments as one JSON object. Or answer without a tool.
