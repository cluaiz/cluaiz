"""Run release CLI checks in an isolated home and save each result immediately."""
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
    parser.add_argument("--only", default="")
    parser.add_argument("--with-server", action="store_true")
    args = parser.parse_args()
    args.exe, args.home = args.exe.resolve(), args.home.resolve()
    if args.port in (8000, 8011) or args.home == Path.home() / ".cluaiz" or not args.exe.is_file():
        parser.error("use an existing executable and isolated home")
    folder = Path(__file__).resolve().parent / "raw"
    journal = Journal(folder, args.home)
    env = os.environ.copy()
    env.update(CLUAIZ_HOME=str(args.home), CLUAIZ_PORT=str(args.port), CLUAIZ_HOST="127.0.0.1",
               CLUAIZ_NONINTERACTIVE="1", NO_COLOR="1")
    chat = "qwen3.5-0.8b-gguf-Q4_K_M"
    vector = "embeddinggemma-300m-gguf-Q8_0"
    flow = REPO / "test/cloud/runs/2026-11-23-run49-release-e2e/run49-check.flow.yaml"
    sample = args.home / "ingest-sample.md"
    sample.write_text("# Release test\n\nThe check number is 4917.\n", encoding="utf-8")
    (args.home / "workspace").mkdir(exist_ok=True)
    (args.home / "workspace" / "notes.txt").write_text("The secret number is 4917.\n", encoding="utf-8")
    cases = [
        ("version", ["--version"], lambda s: "0.1.0" in s),
        ("list", ["list"], lambda s: chat in s and vector in s),
        ("show", ["show", chat], lambda s: chat in s),
        ("status", ["status"], lambda s: "GPU" in s and "RAM" in s),
        ("set_chat", ["model", "set-chat", chat], lambda s: chat in s),
        ("set_vector", ["model", "set-vector", vector], lambda s: vector in s),
        ("run", ["run", chat, "What is 2+2? Answer with the number only.", "--think", "off"],
            lambda s: "4" in s),
        ("bench", ["bench", chat, "-p", "64", "-n", "16", "-r", "1"],
            lambda s: "t/s" in s),
        ("plugin_install", ["plugin", "install", "math", "--yes"], lambda s: "math" in s),
        ("plugin_list", ["plugin", "list"], lambda s: "math" in s),
        ("mcp_install", ["mcp", "install", "filesystem", "--yes"], lambda s: "filesystem" in s),
        ("mcp_list", ["mcp", "list"], lambda s: "filesystem" in s),
        ("flow_validate", ["flow", "validate", str(flow)], lambda s: "valid" in s.lower()),
        ("flow_save", ["flow", "save", str(flow)], lambda s: "saved run49-check" in s),
        ("flow_list", ["flow", "list"], lambda s: "run49-check" in s),
        ("flow_run", ["flow", "run", "run49-check"], lambda s: "succeeded" in s.lower()),
        ("flow_logs", ["flow", "logs", "run49-check"], lambda s: "succeeded" in s.lower()),
        ("ingest", ["ingest", str(sample)], lambda s: "chunk" in s.lower()),
        ("agent_headless", ["agent", "-p", "Read notes.txt and say the secret number.",
                            "--output-format", "json"], lambda s: "4917" in s),
        ("storage", ["storage", "temp-status"], lambda s: "storage" in s.lower() or "temp" in s.lower()),
        ("config", ["config", "list"], lambda s: "chat_memory.enabled" in s),
    ]
    results = []
    only = {name for name in args.only.split(",") if name}
    engine = Engine(args, journal) if args.with_server else None
    file_name = "cli_checks_retry.jsonl" if only else "cli_checks.jsonl"
    try:
        if engine:
            engine.start()
        for name, arguments, check in cases:
            if only and name not in only:
                continue
            started = time.perf_counter()
            journal.write(file_name, {"event": "request", "name": name, "args": arguments, "time": time.time()})
            try:
                run = subprocess.run([str(args.exe), *arguments], cwd=args.home / "workspace", env=env,
                                     input="y\n" if name == "flow_save" else None,
                                     capture_output=True, text=True, errors="replace", timeout=900)
                output = run.stdout + run.stderr
                passed = run.returncode == 0 and check(output)
                detail = {"exit": run.returncode, "pass": passed, "elapsed_s": round(time.perf_counter() - started, 2),
                          "output": output}
            except subprocess.TimeoutExpired as error:
                detail = {"exit": None, "pass": False, "elapsed_s": round(time.perf_counter() - started, 2),
                          "error": repr(error)}
            journal.write(file_name, {"event": "response", "name": name, **detail, "time": time.time()})
            results.append(detail["pass"])
            print(f"{'PASS' if detail['pass'] else 'FAIL'} {name}", flush=True)
    finally:
        if engine:
            engine.stop()
    print(f"{sum(results)}/{len(results)} CLI checks passed")
    return int(not all(results))


if __name__ == "__main__":
    sys.exit(main())
