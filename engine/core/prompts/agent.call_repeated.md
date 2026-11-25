---
name: agent.call_repeated
description: "The result the model gets when it repeats a tool call with the same name and arguments in one answer, with no new call in between: the call is not run again, and the earlier result comes back with this note. The next identical call ends the tool steps."
variables: [name, result]
---
You already called {{name}} with these same arguments, so it was not run again. Its result was:

{{result}}

Do not repeat this call. Use this result, make a different call, or give your final answer.
