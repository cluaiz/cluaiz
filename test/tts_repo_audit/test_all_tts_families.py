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
    print("\n" + "=" * 80)
    print("  CLUAIZ 8-FAMILY TTS INTEGRATION TEST SUITE")
    print("=" * 80)

    results = {}

    # 1. Kokoro Auto-Route
    print("\n[TEST 1] Kokoro Auto-Route (Testing upgraded G2P pronunciation with Magic 'E' and Soft 'C')")
    p1 = {
        "model": "auto",
        "task": "auto",
        "input_source": {
            "type": "text",
            "data": "In the silent depths of space, silence reigns supreme"
        },
        "stream": False,
        "parameters": {
            "voice_id": "af_heart"
        }
    }
    t0 = time.time()
    res1 = post(p1)
    dt1 = time.time() - t0
    print(f"  Execution Time: {dt1:.2f}s")
    if res1 and "data" in res1 and res1["data"].startswith("data:audio/wav;base64,"):
        print(f"  Result: Success! Generated WAV audio payload (length: {len(res1['data'])} chars)")
    else:
        print(f"  Result: {json.dumps(res1, indent=2)}")
    results["test_1_kokoro_auto"] = res1

    #2. Kokoro Explicit
    print("\n[TEST 2] Kokoro Explicit Model ID")
    p2 = {
        "model": "Kokoro-82M-v1.0-ONNX-UINT8",
        "task": "text_to_speech",
        "input_source": {
            "type": "text",
            "data": "This is a direct explicit model test"
        },
        "stream": False,
        "parameters": {
            "voice_id": "af_bella"
        }
    }
    t0 = time.time()
    res2 = post(p2)
    dt2 = time.time() - t0
    print(f"  Execution Time: {dt2:.2f}s")
    if res2 and "data" in res2 and res2["data"].startswith("data:audio/wav;base64,"):
        print(f"  Result: Success! Generated WAV audio payload (length: {len(res2['data'])} chars)")
    else:
        print(f"  Result: {json.dumps(res2, indent=2)}")
    results["test_2_kokoro_explicit"] = res2

    #3. CosyVoice2
    print("\n[TEST 3] CosyVoice2 Pipeline Verification")
    p3 = {
        "model": "CosyVoice2-0.5B",
        "task": "text_to_speech",
        "input_source": {
            "type": "text",
            "data": "Verify cosyvoice pipeline response"
        },
        "stream": False
    }
    res3 = post(p3)
    print(f"  Result: {json.dumps(res3, indent=2)}")
    results["test_3_cosyvoice2"] = res3

    # 4. Supertonic
    print("\n[TEST 4] Supertonic Pipeline Verification")
    p4 = {
        "model": "supertonic-3",
        "task": "text_to_speech",
        "input_source": {
            "type": "text",
            "data": "Verify supertonic pipeline response"
        },
        "stream": False
    }
    res4 = post(p4)
    print(f"  Result: {json.dumps(res4, indent=2)}")
    results["test_4_supertonic"] = res4

    # 5. Audio8
    print("\n[TEST 5] Audio8 Pipeline Verification")
    p5 = {
        "model": "Audio8-TTS-Preview-0.6B-ONNX-INT4-INT4",
        "task": "text_to_speech",
        "input_source": {
            "type": "text",
            "data": "Verify audio8 pipeline response"
        },
        "stream": False
    }
    res5 = post(p5)
    print(f"  Result: {json.dumps(res5, indent=2)}")
    results["test_5_audio8"] = res5

    # 6. OmniVoice
    print("\n[TEST 6] OmniVoice GenAI Pipeline Verification")
    p6 = {
        "model": "OmniVoice-Onnx-CUDA",
        "task": "text_to_speech",
        "input_source": {
            "type": "text",
            "data": "Verify omnivoice pipeline response"
        },
        "stream": False
    }
    res6 = post(p6)
    print(f"  Result: {json.dumps(res6, indent=2)}")
    results["test_6_omnivoice"] = res6

    # 7. Matcha
    print("\n[TEST 7] Matcha Flow-Matching Pipeline Verification")
    p7 = {
        "model": "LuxTTS-INT8",
        "task": "text_to_speech",
        "input_source": {
            "type": "text",
            "data": "Verify matcha pipeline response"
        },
        "stream": False
    }
    res7 = post(p7)
    print(f"  Result: {json.dumps(res7, indent=2)}")
    results["test_7_matcha"] = res7

    # 8. Chatterbox
    print("\n[TEST 8] Chatterbox Semantic Codec Pipeline Verification")
    p8 = {
        "model": "chatterbox-dummy",
        "task": "text_to_speech",
        "input_source": {
            "type": "text",
            "data": "Verify chatterbox pipeline response"
        },
        "stream": False
    }
    res8 = post(p8)
    print(f"  Result: {json.dumps(res8, indent=2)}")
    results["test_8_chatterbox"] = res8

    # 9. VitsPiper
    print("\n[TEST 9] VitsPiper End-to-End Pipeline Verification")
    p9 = {
        "model": "piper-vits-dummy",
        "task": "text_to_speech",
        "input_source": {
            "type": "text",
            "data": "Verify piper vits pipeline response"
        },
        "stream": False
    }
    res9 = post(p9)
    print(f"  Result: {json.dumps(res9, indent=2)}")
    results["test_9_vitspiper"] = res9

    output_json_path = "test/tts_repo_audit/test_all_tts_families.json"
    with open(output_json_path, "w", encoding="utf-8") as f:
        json.dump(results, f, indent=2)
    print(f"\n[OK] All test output responses saved to: {output_json_path}")

if __name__ == "__main__":
    run_tests()
