import requests
import json

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

res = requests.post(url, json=payload)
print("Status Code:", res.status_code)
print("Response:", res.text)
