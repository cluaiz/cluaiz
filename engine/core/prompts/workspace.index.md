---
name: workspace.index
description: "The heading of the index in the agent's system prompt, which lists the rules, workflows and skills the model reads on demand and the facts of the project's memory, one \"- name (kind): description\" line each."
variables: []
---
More rules, workflows and skills of this project, and the facts in its memory. When the task fits a rule, workflow or skill, call read_item with its name before you start, then follow it. A memory line is the fact itself:
