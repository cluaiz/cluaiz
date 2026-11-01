---
name: chat.card
description: Asks the loaded model, in the background, for the search card of the last exchange. The JSON keys (title, motive, summary, keywords) are fixed by a grammar; change the wording, not the keys.
variables: []
---
Write a search card for my previous message and your answer to it (not for this request). Reply with only one JSON object and nothing else: {"title": "what that exchange was about, at most 10 words", "motive": "why I asked, at most 12 words", "summary": "at most 2 short sentences", "keywords": ["up to 6 words, in English and in my language"]}
