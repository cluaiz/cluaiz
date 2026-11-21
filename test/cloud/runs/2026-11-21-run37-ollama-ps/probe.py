"""Run 37: `client.ps()` of the Ollama Python library before and after a chat loads a model.

Usage: python probe.py <host> <token_file> <model>
"""
import sys

import ollama

HOST, TOKEN_FILE, MODEL = sys.argv[1], sys.argv[2], sys.argv[3]
TOKEN = open(TOKEN_FILE, encoding="utf-8").read().strip()
client = ollama.Client(host=HOST, headers={"Authorization": f"Bearer {TOKEN}"})
print("before a chat:", client.ps())
client.chat(model=MODEL, messages=[{"role": "user", "content": "hi"}], think=False, options={"num_predict": 4})
print("after a chat:", client.ps())
