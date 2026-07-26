"""
Cluaiz ONNX Audio API Comprehensive Integration Test Suite
Tests all audio task types defined in normalize_audio_task() rules.
Run: python test/integration/onnx_audio_comprehensive_test.py
"""

import json
import urllib.request
import urllib.error
import sys
import time

BASE_URL = "http://localhost:8000/v1/audio/execute"
API_KEY  = "sk-cluaiz-eaeae61cd0dd1bf368144c29bafc172d"
AUDIO_FILE = "C:\\Users\\Aryan\\Downloads\\Recording.m4a"

PASS = "[PASS]"
FAIL = "[FAIL]"
SKIP = "[SKIP]"

results = []

def run_test(test_id, test_name, payload, expect_status=200, expect_error=False):
    print(f"\n{'='*60}")
    print(f"TEST {test_id}: {test_name}")
    print(f"{'='*60}")
    print(f"Payload: {json.dumps(payload, indent=2)}")

    req = urllib.request.Request(
        BASE_URL,
        data=json.dumps(payload).encode("utf-8"),
        headers={
            "Content-Type": "application/json",
            "Authorization": f"Bearer {API_KEY}"
        },
        method="POST"
    )

    try:
        with urllib.request.urlopen(req, timeout=60) as resp:
            status = resp.getcode()
            body = json.loads(resp.read().decode("utf-8"))
            print(f"\nHTTP {status}")
            print(f"Response: {json.dumps(body, indent=2)}")

            if expect_error:
                # Expected to fail at API level but got 200 - check body for error
                if body.get("status") == "error":
                    print(f"{PASS} - Got expected error in 200 body")
                    results.append((test_id, test_name, True, "Expected error in body"))
                elif expect_status == 200:
                    text = body.get("output", {}).get("text", "")
                    ok = bool(text and not text.startswith("Error:"))
                    tag = PASS if ok else FAIL
                    print(f"{tag} - text='{text[:80]}'")
                    results.append((test_id, test_name, ok, text[:80]))
            else:
                text = body.get("output", {}).get("text", "")
                task_out = body.get("task", "")
                ok = status == expect_status and bool(text and not text.startswith("Error:"))
                tag = PASS if ok else FAIL
                print(f"{tag} - status={status} task={task_out} text='{text[:80]}'")
                results.append((test_id, test_name, ok, f"text='{text[:80]}'"))

    except urllib.error.HTTPError as e:
        status = e.code
        try:
            body = json.loads(e.read().decode("utf-8"))
        except Exception:
            body = {}
        print(f"\nHTTP {status}")
        print(f"Error body: {json.dumps(body, indent=2)}")
        error_msg = body.get("error", "")
        ok = (status == expect_status) if expect_status != 200 else False
        tag = PASS if ok else FAIL
        print(f"{tag} - Got HTTP {status} (expected {expect_status})")
        results.append((test_id, test_name, ok, f"HTTP {status}: {error_msg[:60]}"))

    except Exception as e:
        print(f"\nCONNECTION ERROR: {e}")
        results.append((test_id, test_name, False, f"Connection error: {e}"))


def main():
    print("\nCluaiz ONNX Audio API - Comprehensive Integration Test Suite")
    print("Tests all task types from normalize_audio_task() dynamic routing rules")
    print(f"Audio File: {AUDIO_FILE}")
    print(f"Server: {BASE_URL}\n")

    # ── GROUP 1: ONNX STT Core (speech_to_text) ─────────────────────────────
    run_test(
        "1.1", "STT - explicit model + speech_to_text task",
        {
            "model": "whisper-large-v3-turbo-INT8",
            "task": "speech_to_text",
            "instruction": "Transcribe this audio cleanly into Hindi.",
            "input_source": {"type": "url", "data": AUDIO_FILE},
            "parameters": {"language": "hi", "temperature": 0}
        }
    )

    run_test(
        "1.2", "STT - auto model + auto task (dynamic routing)",
        {
            "model": "auto",
            "task": "auto",
            "instruction": "Transcribe speech to text.",
            "input_source": {"type": "url", "data": AUDIO_FILE},
            "parameters": {"language": "hi"}
        }
    )

    run_test(
        "1.3", "STT - ASR alias task routing",
        {
            "model": "whisper-large-v3-turbo-INT8",
            "task": "automatic_speech_recognition",
            "input_source": {"type": "url", "data": AUDIO_FILE},
            "parameters": {"language": "hi"}
        }
    )

    run_test(
        "1.4", "STT - 'asr' shorthand task alias",
        {
            "model": "whisper-large-v3-turbo-INT8",
            "task": "asr",
            "input_source": {"type": "url", "data": AUDIO_FILE}
        }
    )

    run_test(
        "1.5", "STT - 'speech_recognition' alias",
        {
            "model": "whisper-large-v3-turbo-INT8",
            "task": "speech_recognition",
            "input_source": {"type": "url", "data": AUDIO_FILE}
        }
    )

    # ── GROUP 2: Speech Translation ──────────────────────────────────────────
    run_test(
        "2.1", "Speech Translation - speech_translation task",
        {
            "model": "whisper-large-v3-turbo-INT8",
            "task": "speech_translation",
            "instruction": "Translate spoken Hindi to English.",
            "input_source": {"type": "url", "data": AUDIO_FILE},
            "parameters": {"language": "hi", "translate_to": "en"}
        }
    )

    run_test(
        "2.2", "Speech Translation - 'translation' alias",
        {
            "model": "whisper-large-v3-turbo-INT8",
            "task": "translation",
            "input_source": {"type": "url", "data": AUDIO_FILE},
            "parameters": {"translate_to": "en"}
        }
    )

    # ── GROUP 3: Modality Mismatch Guardrails (Expected HTTP 400) ────────────
    run_test(
        "3.1", "Guardrail: TTS model with audio input (must 400)",
        {
            "model": "auto",
            "task": "text_to_speech",
            "input_source": {"type": "url", "data": AUDIO_FILE}
        },
        expect_status=400
    )

    run_test(
        "3.2", "Guardrail: STT model with text input (must 400)",
        {
            "model": "whisper-large-v3-turbo-INT8",
            "task": "speech_to_text",
            "input_source": {"type": "text", "data": "Hello world text."}
        },
        expect_status=400
    )

    # ── GROUP 4: Parameter Variants ─────────────────────────────────────────
    run_test(
        "4.1", "STT - with timestamps flag",
        {
            "model": "whisper-large-v3-turbo-INT8",
            "task": "speech_to_text",
            "input_source": {"type": "url", "data": AUDIO_FILE},
            "parameters": {"timestamps": True, "language": "hi"}
        }
    )

    run_test(
        "4.2", "STT - high temperature (0.8)",
        {
            "model": "whisper-large-v3-turbo-INT8",
            "task": "speech_to_text",
            "input_source": {"type": "url", "data": AUDIO_FILE},
            "parameters": {"temperature": 0.8, "language": "hi"}
        }
    )

    run_test(
        "4.3", "STT - language auto-detect",
        {
            "model": "whisper-large-v3-turbo-INT8",
            "task": "speech_to_text",
            "input_source": {"type": "url", "data": AUDIO_FILE},
            "parameters": {"language": "auto"}
        }
    )

    # ── GROUP 5: Task Alias Normalization (dynamic routing) ──────────────────
    run_test(
        "5.1", "Task alias: tts -> text_to_speech (text input)",
        {
            "model": "auto",
            "task": "tts",
            "input_source": {"type": "text", "data": "Hello from Cluaiz."}
        }
    )

    run_test(
        "5.2", "Task alias: voice_conversion -> voice_conversion",
        {
            "model": "auto",
            "task": "voice_conversion",
            "input_source": {"type": "url", "data": AUDIO_FILE}
        }
    )

    run_test(
        "5.3", "Task alias: audio_classification -> audio_classification",
        {
            "model": "auto",
            "task": "audio_classification",
            "input_source": {"type": "url", "data": AUDIO_FILE}
        }
    )

    run_test(
        "5.4", "Task alias: speaker_diarization -> speaker_diarization",
        {
            "model": "auto",
            "task": "speaker_diarization",
            "input_source": {"type": "url", "data": AUDIO_FILE}
        }
    )

    run_test(
        "5.5", "Task alias: diarization -> speaker_diarization",
        {
            "model": "auto",
            "task": "diarization",
            "input_source": {"type": "url", "data": AUDIO_FILE}
        }
    )

    run_test(
        "5.6", "Task alias: emotion_recognition",
        {
            "model": "auto",
            "task": "emotion_recognition",
            "input_source": {"type": "url", "data": AUDIO_FILE}
        }
    )

    run_test(
        "5.7", "Task alias: keyword_spotting -> keyword_spotting",
        {
            "model": "auto",
            "task": "keyword_spotting",
            "input_source": {"type": "url", "data": AUDIO_FILE}
        }
    )

    run_test(
        "5.8", "Task alias: noise_reduction",
        {
            "model": "auto",
            "task": "noise_reduction",
            "input_source": {"type": "url", "data": AUDIO_FILE}
        }
    )

    run_test(
        "5.9", "Task alias: audio_embedding -> audio_embedding",
        {
            "model": "auto",
            "task": "audio_embedding",
            "input_source": {"type": "url", "data": AUDIO_FILE}
        }
    )

    run_test(
        "5.10", "Task alias: language_identification -> language_identification",
        {
            "model": "auto",
            "task": "language_identification",
            "input_source": {"type": "url", "data": AUDIO_FILE}
        }
    )

    # ── GROUP 6: Error Handling ───────────────────────────────────────────────
    run_test(
        "6.1", "Error: non-existent audio file path",
        {
            "model": "whisper-large-v3-turbo-INT8",
            "task": "speech_to_text",
            "input_source": {"type": "url", "data": "C:\\fake\\path\\doesnotexist.m4a"}
        },
        expect_status=500
    )

    run_test(
        "6.2", "Error: model does not exist",
        {
            "model": "fake-model-xyz",
            "task": "speech_to_text",
            "input_source": {"type": "url", "data": AUDIO_FILE}
        },
        expect_status=404
    )

    run_test(
        "6.3", "Error: missing input_source",
        {
            "model": "whisper-large-v3-turbo-INT8",
            "task": "speech_to_text"
        },
        expect_status=422
    )

    # ── GROUP 7: Instruction Variants ────────────────────────────────────────
    run_test(
        "7.1", "STT - Hindi instruction override",
        {
            "model": "whisper-large-v3-turbo-INT8",
            "task": "speech_to_text",
            "instruction": "Audio ko Hindi mein transcribe karo, koi bhi word miss mat karo.",
            "input_source": {"type": "url", "data": AUDIO_FILE},
            "parameters": {"language": "hi"}
        }
    )

    run_test(
        "7.2", "STT - English instruction override",
        {
            "model": "whisper-large-v3-turbo-INT8",
            "task": "speech_to_text",
            "instruction": "Transcribe all speech to English text verbatim.",
            "input_source": {"type": "url", "data": AUDIO_FILE},
            "parameters": {"language": "en"}
        }
    )

    # ── FINAL RESULTS SUMMARY ────────────────────────────────────────────────
    print(f"\n{'='*60}")
    print("FINAL TEST RESULTS SUMMARY")
    print(f"{'='*60}")

    total = len(results)
    passed = sum(1 for _, _, ok, _ in results if ok)
    failed = total - passed

    for tid, name, ok, detail in results:
        tag = PASS if ok else FAIL
        print(f"  {tag} [{tid}] {name}")
        if not ok:
            print(f"         -> {detail}")

    print(f"\nTotal: {total} | Passed: {passed} | Failed: {failed}")
    print(f"Success Rate: {(passed/total*100):.1f}%" if total else "No tests run")

    if failed > 0:
        sys.exit(1)
    else:
        print("\nAll tests passed!")
        sys.exit(0)


if __name__ == "__main__":
    main()
