---
name: chat.noted_similar
description: "Added to the remember tool's result in a chat when the saved note may repeat notes the user keeps: one line per note follows."
variables: [notes]
---
Notes already kept that may be about the same thing:
{{notes}}
If the new note changes or corrects one of them (a new value for the same thing, or the old one was wrong), call forget with that note's id. If they are about other people, things or facts, do nothing.
