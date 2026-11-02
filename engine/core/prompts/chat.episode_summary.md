---
name: chat.episode_summary
description: Asks the loaded model, in the background, for the title and summary of an earlier part of a chat; the part's messages follow it after a blank line. The JSON keys (title, summary, keywords) are fixed by a grammar; change the wording, not the keys.
variables: []
---
Below is part of a chat. Reply with only one JSON object and nothing else: {"title": "what it was about, at most 10 words", "summary": "2-3 sentences: what was asked, decided and done", "keywords": ["up to 6 words"]}
