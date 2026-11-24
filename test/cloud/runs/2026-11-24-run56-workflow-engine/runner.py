"""Start an isolated Engine and run the workflow logic/trigger journey."""
import argparse
import json
import os
from pathlib import Path
import subprocess
import sys
import time


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
        parser.error("use an isolated home and test port")
    args.home.mkdir(parents=True, exist_ok=True)
    journal = Journal(Path(__file__).resolve().parent / "raw", args.home)
    engine = Engine(args, journal)
    env = os.environ.copy()
    env.update(CLUAIZ_HOME=str(args.home), CLUAIZ_PORT=str(args.port), CLUAIZ_HOST="127.0.0.1")
    script = Path(__file__).resolve().with_name("engine-check.cjs")
    journal.write("setup.jsonl", {"exe": str(args.exe), "home": str(args.home), "port": args.port,
                                  "script": str(script), "time": time.time()})
    try:
        engine.start()
        child = subprocess.Popen(["node", str(script)], cwd=args.home / "workspace", env=env,
                                 stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True, errors="replace")
        for line in child.stdout:
            journal.write("console.log", line.rstrip())
        child.wait()
        results = sorted((Path(__file__).resolve().parent / "raw").glob("engine-result-*.json"))
        cases = json.loads(results[-1].read_text(encoding="utf-8")) if results else []
        passed = sum(case.get("status") == "PASS" for case in cases)
        journal.write("setup.jsonl", {"event": "finished", "exit": child.returncode,
                                      "passed": passed, "total": len(cases), "time": time.time()})
        print(f"workflow checks {passed}/{len(cases)}; exit {child.returncode}")
        return int(child.returncode != 0 or passed != len(cases))
    finally:
        engine.stop()


if __name__ == "__main__":
    sys.exit(main())
