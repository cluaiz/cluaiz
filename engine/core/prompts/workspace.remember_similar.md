---
name: workspace.remember_similar
description: "The agent's remember tool result when the new fact may repeat facts already in the project's memory: their names and texts follow, and nothing was saved yet."
variables: [facts]
---
Not saved yet. The project's memory already has facts that may be about the same thing:
{{facts}}
Decide, then call remember again now with the same fact:
- replaces set to a fact's name only if the new fact changes or corrects that very fact (its file gets the new text);
- replaces "none" if it is about another thing, even when words are shared.
