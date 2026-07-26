import requests
import json
import os

url = "http://localhost:8000/v1/audio/execute"

payload = {
    "model": "auto",
    "task": "auto",
    "instruction": "Transcribe this audio cleanly.",
    "input_source": {
        "type": "url",
        "data": r"C:\Users\Aryan\Downloads\NoteGPT_Speech_8.webm"
    },
    "parameters": {
        "language": "en"
    },
    "keep_alive": 10
}

headers = {
    "Content-Type": "application/json"
}

print(f"[TEST] Sending WebM File Payload to {url}...")
try:
    response = requests.post(url, json=payload, headers=headers, timeout=60)
    print(f"STATUS CODE: {response.status_code}")
    print("RESPONSE RAW JSON:")
    print(json.dumps(response.json(), indent=2))
except Exception as e:
    print(f"❌ HTTP Execution Error: {e}")
