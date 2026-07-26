import json
import urllib.request
import urllib.error
import sys

BASE_URL = "http://localhost:8000/v1/audio/execute"
API_KEY = "sk-cluaiz-eaeae61cd0dd1bf368144c29bafc172d"

def run_test(test_name, payload):
    print(f"\n==========================================")
    print(f"RUNNING TEST: {test_name}")
    print(f"==========================================")
    print(f"Request Payload:\n{json.dumps(payload, indent=2)}")

    req = urllib.request.Request(
        BASE_URL,
        data=json.dumps(payload).encode('utf-8'),
        headers={
            "Content-Type": "application/json",
            "Authorization": f"Bearer {API_KEY}"
        },
        method="POST"
    )

    try:
        with urllib.request.urlopen(req) as response:
            status_code = response.getcode()
            res_body = json.loads(response.read().decode('utf-8'))
            print(f"\nSUCCESS: STATUS {status_code} OK")
            print(f"Response Output:\n{json.dumps(res_body, indent=2)}")
            return True, res_body
    except urllib.error.HTTPError as e:
        status_code = e.code
        error_body = json.loads(e.read().decode('utf-8'))
        print(f"\nHTTP ERROR STATUS: {status_code}")
        print(f"Error Body:\n{json.dumps(error_body, indent=2)}")
        return False, error_body
    except Exception as e:
        print(f"\nCONNECTION ERROR: {e}")
        return False, str(e)

def main():
    print("Initiating Deep Audio API Modality & Execution Integration Suite...")
    
    # ── Test 1: Speech-to-Text via Local Audio File Path (URL mode) ──
    run_test(
        "1. Speech-to-Text (STT) - Local File URL Input",
        {
            "model": "auto",
            "task": "auto",
            "instruction": "Transcribe this audio cleanly into Hindi.",
            "input_source": {
                "type": "url",
                "data": "C:\\Users\\Aryan\\Downloads\\Recording.m4a"
            },
            "parameters": {
                "temperature": 0,
                "language": "hi",
                "beam_size": 5
            }
        }
    )

    # ── Test 2: Speech-to-Text via Base64 Audio Chunk Payload ──
    run_test(
        "2. Speech-to-Text (STT) - Base64 Data Payload",
        {
            "model": "auto",
            "task": "speech_to_text",
            "instruction": "Transcribe base64 input.",
            "input_source": {
                "type": "base64",
                "data": "data:audio/webm;base64,GkXfo59ChoEBQveBAULygQ3CS..."
            },
            "parameters": {
                "language": "auto"
            }
        }
    )

    # ── Test 3: Text-to-Speech (TTS) - Text Input Auto-Routing ──
    run_test(
        "3. Text-to-Speech (TTS) - Text Input Auto-Routing",
        {
            "model": "auto",
            "task": "auto",
            "instruction": "Synthesize speech.",
            "input_source": {
                "type": "text",
                "data": "Namaste Cluaiz AI, testing text to speech modal routing."
            },
            "parameters": {
                "voice_id": "alloy",
                "speed": 1.0
            }
        }
    )

    # ── Test 4: Modality Mismatch Guardrail Validation (Expected 400) ──
    run_test(
        "4. Modality Mismatch Guardrail Validation (Expected 400 Bad Request)",
        {
            "model": "auto",
            "task": "text_to_speech",
            "input_source": {
                "type": "url",
                "data": "C:\\Users\\Aryan\\Downloads\\Recording.m4a"
            }
        }
    )

    # ── Test 5: Explicit Audio Model Selection Override ──
    run_test(
        "5. Explicit Model Selection Override (xkeyC-whisper-large-v3-turbo-gguf)",
        {
            "model": "xkeyC-whisper-large-v3-turbo-gguf",
            "task": "speech_to_text",
            "input_source": {
                "type": "url",
                "data": "C:\\Users\\Aryan\\Downloads\\Recording.m4a"
            }
        }
    )

if __name__ == "__main__":
    main()
