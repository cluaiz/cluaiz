"""One-off: the rendered prompt (return_prompt) of an audio and an image request with get_weather, to see which
tool form reached the model. python -B prompt_probe.py --exe <exe> --home <home> --port 8013 --out <dir>"""
import argparse, json, os, sys
from pathlib import Path
sys.path.insert(0, str(Path(__file__).resolve().parent))
import edgecheck as ec
p = argparse.ArgumentParser()
p.add_argument("--exe", type=Path, required=True); p.add_argument("--home", type=Path, required=True)
p.add_argument("--port", type=int, default=8013); p.add_argument("--out", type=Path, required=True)
a = p.parse_args()
journal = ec.probe.Journal(a.out, a.home); engine = ec.probe.Engine(a, journal)
reqs = {
  "audio_tool": ec.body(model=ec.AUDIO_MODEL, prompt_parts=[ec.audio_part, {"type": "text", "text": "Answer the spoken question. Use the get_weather tool."}], temperature=0, max_tokens=32, think="off", stream=False, tools=[ec.WEATHER], return_prompt=True),
  "image_tool": ec.body(model=ec.VISION, prompt_parts=[ec.IMAGE, ec.TOOL_TEXT], temperature=0, max_tokens=32, think="off", stream=False, tools=[ec.WEATHER], return_prompt=True),
}
try:
    engine.start()
    for name, req in reqs.items():
        status, reply = engine.request("POST", "/v1/chat/completions", req, timeout=900, record=False)
        eng = reply.get("engine") if isinstance(reply, dict) else None
        prompt = None
        if isinstance(eng, dict):
            prompt = (eng.get("context") or {}).get("prompt")
        journal.write("prompts.jsonl", {"case": name, "status": status, "engine_keys": sorted(eng) if isinstance(eng, dict) else None, "prompt": prompt,
                                        "message": reply["choices"][0]["message"] if isinstance(reply, dict) and reply.get("choices") else reply})
        print(name, status, json.dumps(prompt)[:3000] if prompt else sorted(eng or {}))
finally:
    engine.stop()
