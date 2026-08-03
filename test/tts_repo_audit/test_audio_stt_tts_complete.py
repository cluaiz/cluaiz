import json
import urllib.request
import urllib.error
import time

BASE_URL = "http://localhost:8000/v1/audio/execute"

def post(payload):
    data = json.dumps(payload).encode('utf-8')
    req = urllib.request.Request(BASE_URL, data=data, headers={"Content-Type": "application/json"})
    try:
        with urllib.request.urlopen(req, timeout=120) as r:
            res_text = r.read().decode('utf-8')
            return json.loads(res_text)
    except urllib.error.HTTPError as e:
        err_body = e.read().decode('utf-8')
        try:
            return json.loads(err_body)
        except Exception:
            return {"error": f"HTTP {e.code}: {err_body}"}
    except Exception as e:
        return {"error": str(e)}

def run_tests():
    print("\n" + "=" * 70)
    print("  EXACT USER PAYLOADS INTEGRATION TEST")
    print("=" * 70)

    # ─────────────────────────────────────────────────────────────
    # PAYLOAD 1: Audio file path passed as text -> Auto-route to STT
    # ─────────────────────────────────────────────────────────────
    print("\n[PAYLOAD 1] Audio WebM Path -> STT Auto-Detect")
    p1 = {
        "model": "auto",
        "task": "auto",
        "instruction": "",
        "input_source": {
            "type": "text",
            "data": "C:\\Users\\Aryan\\Downloads\\test.webm"
        },
        "stream": False,
        "parameters": {
            "temperature": 0,
            "language": "",
            "speed": 1,
            "voice_id": " ",
            "translate_to": "",
            "timestamps": False,
            "beam_size": 5,
            "vad_filter": True,
            "speaker_labels": False
        }
    }
    t0 = time.time()
    res1 = post(p1)
    dt1 = time.time() - t0
    print(f"  Time: {dt1:.2f}s")
    print(f"  Result: {json.dumps(res1, indent=2)}")

    # ─────────────────────────────────────────────────────────────
    # PAYLOAD 2: Raw text string -> Auto-route to TTS
    # ─────────────────────────────────────────────────────────────
    print("\n[PAYLOAD 2] Spoken Text -> TTS Auto-Detect")
    p2 = {
        "model": "auto",
        "task": "auto",
        "instruction": "",
        "input_source": {
            "type": "text",
            "data": "hello bro how are u"
        },
        "stream": False,
        "parameters": {
            "temperature": 0,
            "language": "",
            "speed": 1,
            "voice_id": " ",
            "translate_to": "",
            "timestamps": False,
            "beam_size": 5,
            "vad_filter": True,
            "speaker_labels": False
        }
    }
    t0 = time.time()
    res2 = post(p2)
    dt2 = time.time() - t0
    print(f"  Time: {dt2:.2f}s")
    print(f"  Result: {json.dumps(res2, indent=2)}")

if __name__ == "__main__":
    run_tests()
