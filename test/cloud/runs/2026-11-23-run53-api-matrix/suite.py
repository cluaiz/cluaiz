"""Run the existing release API matrix against an Engine started by this suite.

python -B suite.py --exe <cluaiz.exe> --home <isolated-home> --port 8013
"""
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
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--exe", type=Path, required=True)
    parser.add_argument("--home", type=Path, required=True)
    parser.add_argument("--port", type=int, default=8013)
    parser.add_argument("--only", default="")
    parser.add_argument("--label", default="matrix")
    parser.add_argument("--driver", choices=("api", "tools"), default="api")
    args = parser.parse_args()
    args.exe, args.home = args.exe.resolve(), args.home.resolve()
    if args.port in (8000, 8011) or args.home == Path.home() / ".cluaiz":
        parser.error("use an isolated home and port")
    folder = Path(__file__).resolve().parent / "raw"
    journal = Journal(folder, args.home)
    engine = Engine(args, journal)
    script = REPO / "test/cloud/remote/probes" / (
        "release_e2e.py" if args.driver == "api" else "tools_checks.py")
    result_file = folder / f"{args.driver}_checks_{args.label}.jsonl"
    journal.write("setup.jsonl", {"exe": str(args.exe), "home": str(args.home),
                                  "port": args.port, "driver": str(script), "only": args.only, "label": args.label,
                                  "engine_cwd": str(args.home / "workspace"), "time": time.time()})
    try:
        (args.home / "workspace").mkdir(exist_ok=True)
        os.chdir(args.home / "workspace")
        engine.start()
        command = [sys.executable, str(script), "--home", str(args.home), "--port", str(args.port),
                   "--out", str(result_file)]
        if args.only and args.driver == "api":
            command += ["--only", args.only]
        process = subprocess.Popen(command, stdout=subprocess.PIPE, stderr=subprocess.STDOUT,
                                   text=True, errors="replace", cwd=REPO)
        for line in process.stdout:
            journal.write("console.log", line.rstrip())
        process.wait()
        results = [json.loads(line) for line in result_file.read_text(encoding="utf-8").splitlines()]
        journal.write("setup.jsonl", {"event": "suite_finished", "exit": process.returncode,
                      "passed": sum(bool(item.get("pass")) for item in results), "total": len(results),
                      "time": time.time()})
        print(f"api checks {sum(bool(item.get('pass')) for item in results)}/{len(results)}; driver exit {process.returncode}")
        return int(process.returncode != 0 or not all(item.get("pass") for item in results))
    finally:
        engine.stop()


if __name__ == "__main__":
    sys.exit(main())
