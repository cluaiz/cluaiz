"""Probe the documented video_url chat input with a local red MP4 fixture."""
import argparse
from pathlib import Path
import sys


REPO = Path(__file__).resolve().parents[4]
sys.path.insert(0, str(REPO / "test" / "release_soak"))
from soak import Engine, Journal  # noqa: E402


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--exe", type=Path, required=True)
    parser.add_argument("--home", type=Path, required=True)
    parser.add_argument("--port", type=int, default=8013)
    args = parser.parse_args()
    args.exe, args.home = args.exe.resolve(), args.home.resolve()
    if args.port in (8000, 8011) or args.home == Path.home() / ".cluaiz":
        parser.error("use isolated home and port")
    clip = args.home / "workspace" / "red-clip.mp4"
    if not clip.is_file():
        parser.error("make a red clip in the isolated workspace first")
    journal = Journal(Path(__file__).resolve().parent / "raw", args.home)
    engine = Engine(args, journal)
    body = {"model": "qwen3.5-0.8b-gguf-Q4_K_M", "temperature": 0, "max_tokens": 64,
            "think_mode": "off", "messages": [{"role": "user", "content": [
                {"type": "text", "text": "What colour are the frames? Answer one word."},
                {"type": "video_url", "video_url": {"url": str(clip), "include_audio": False}},
            ]}]}
    try:
        engine.start()
        status, reply = engine.request("POST", "/v1/chat/completions", body)
        answer = ((reply.get("choices") or [{}])[0].get("message") or {}).get("content") if isinstance(reply, dict) else ""
        ok = status == 200 and "red" in (answer or "").lower()
        journal.write("video_check.jsonl", {"pass": ok, "status": status, "answer": answer,
                                             "fixture_bytes": clip.stat().st_size})
        print(f"video red {int(ok)}/1, HTTP {status}, answer {answer!r}")
        return int(not ok)
    finally:
        engine.stop()


if __name__ == "__main__":
    sys.exit(main())
