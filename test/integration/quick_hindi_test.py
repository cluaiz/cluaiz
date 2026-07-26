"""
Quick Hindi + Auto-Detect Audio Test
Tests the two critical scenarios that were broken.
Run AFTER server restarts: python test/integration/quick_hindi_test.py
"""
import json, urllib.request, urllib.error, time

BASE_URL   = "http://localhost:8000/v1/audio/execute"
AUDIO_FILE = "C:\\Users\\Aryan\\Downloads\\Recording.m4a"

def post(payload):
    data = json.dumps(payload).encode()
    req = urllib.request.Request(BASE_URL, data=data,
                                 headers={"Content-Type": "application/json"})
    try:
        with urllib.request.urlopen(req, timeout=120) as r:
            return json.loads(r.read())
    except urllib.error.HTTPError as e:
        return {"error": e.read().decode()}

print("\n" + "="*60)
print("  Hindi Audio Quick Test")
print("="*60)

# Test 1: Explicit Hindi
print("\n[TEST 1] language='hi' (explicit Hindi)")
t0 = time.time()
r = post({
    "model": "whisper-large-v3-turbo-INT8",
    "task": "speech_to_text",
    "input_source": {"type": "url", "data": AUDIO_FILE},
    "parameters": {"language": "hi", "temperature": 0}
})
t1 = time.time()
text = r.get("output", {}).get("text", r.get("error", "ERROR"))
print(f"  Time   : {t1-t0:.1f}s")
print(f"  Result : '{text}'")
ok1 = bool(text) and text not in ("|", "!", ".", "") and not text.startswith("Error")
print(f"  Status : {'PASS OK' if ok1 else 'FAIL NO'}")

# Test 2: Auto-detect (no language)
print("\n[TEST 2] language='' (auto-detect)")
t0 = time.time()
r = post({
    "model": "whisper-large-v3-turbo-INT8",
    "task": "speech_to_text",
    "input_source": {"type": "url", "data": AUDIO_FILE},
    "parameters": {"language": "", "temperature": 0}
})
t1 = time.time()
text = r.get("output", {}).get("text", r.get("error", "ERROR"))
print(f"  Time   : {t1-t0:.1f}s")
print(f"  Result : '{text}'")
ok2 = bool(text) and text not in ("|", "!", ".", "") and not text.startswith("Error")
print(f"  Status : {'PASS OK' if ok2 else 'FAIL NO'}")

# Test 3: Auto keyword
print("\n[TEST 3] language='auto'")
t0 = time.time()
r = post({
    "model": "whisper-large-v3-turbo-INT8",
    "task": "speech_to_text",
    "input_source": {"type": "url", "data": AUDIO_FILE},
    "parameters": {"language": "auto", "temperature": 0}
})
t1 = time.time()
text = r.get("output", {}).get("text", r.get("error", "ERROR"))
print(f"  Time   : {t1-t0:.1f}s")
print(f"  Result : '{text}'")
ok3 = bool(text) and text not in ("|", "!", ".", "") and not text.startswith("Error")
print(f"  Status : {'PASS OK' if ok3 else 'FAIL NO'}")

print("\n" + "="*60)
all_ok = ok1 and ok2 and ok3
print(f"  Overall: {'ALL PASS' if all_ok else 'SOME FAILED'}")
print("="*60)
