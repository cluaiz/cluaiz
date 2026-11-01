---
name: workflow.run
description: What the agent sends when the user types /<name> of a project workflow (.agents/workflows/<name>.md). The workflow's steps and what the user typed after the name are filled in.
variables: [name, steps, arguments]
---
Run the workflow "{{name}}" of this project now. Do its steps in order with the tools. If a step fails or cannot be done, stop there and say what happened. When it is done, say briefly what each step did.

Steps:
{{steps}}

The user added: {{arguments}}
