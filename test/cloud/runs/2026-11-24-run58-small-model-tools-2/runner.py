"""Run the unchanged Run 39 tasks and grading against an isolated Engine server."""
import argparse
import ctypes
import json
import os
from pathlib import Path
import socket
import subprocess
import sys
import time
import urllib.request

HINT = ("For weather, rain, temperature or umbrella questions use get_weather; "
        "for money conversion use convert_currency; for reminders use create_reminder; "
        "for finding documents use search_files; for sending mail use send_email. "
        "For questions needing none of these actions, do not call a tool.")


def free_ram_gib():
    fields = [("dwLength", ctypes.c_ulong), ("dwMemoryLoad", ctypes.c_ulong)]
    fields += [(name, ctypes.c_ulonglong) for name in (
        "ullTotalPhys", "ullAvailPhys", "ullTotalPageFile", "ullAvailPageFile",
        "ullTotalVirtual", "ullAvailVirtual", "ullAvailExtendedVirtual")]
    memory = type("Memory", (ctypes.Structure,), {"_fields_": fields})()
    memory.dwLength = ctypes.sizeof(memory)
    ctypes.windll.kernel32.GlobalMemoryStatusEx(ctypes.byref(memory))
    return round(memory.ullAvailPhys / 2**30, 2)


def free_vram_mib():
    reply = subprocess.run(["nvidia-smi", "--query-gpu=memory.free,memory.total",
                            "--format=csv,noheader,nounits"], capture_output=True, text=True, timeout=10)
    return reply.stdout.strip() if reply.returncode == 0 else "?"


def run():
    parser = argparse.ArgumentParser()
    parser.add_argument("--exe", type=Path, required=True)
    parser.add_argument("--home", type=Path, required=True)
    parser.add_argument("--port", type=int, default=8014)
    parser.add_argument("--variant", choices=("baseline", "hint"), default="baseline")
    parser.add_argument("models", nargs="+")
    args = parser.parse_args()
    args.exe, args.home = args.exe.resolve(), args.home.resolve()
    if args.port not in (8013, 8014) or args.home == Path.home() / ".cluaiz":
        parser.error("use port 8013/8014 and an isolated home")
    with socket.socket() as sock:
        if sock.connect_ex(("127.0.0.1", args.port)) == 0:
            parser.error("test port is already occupied")
    folder = Path(__file__).resolve().parent
    result_dir = folder / args.variant
    result_dir.mkdir(exist_ok=True)
    raw_dir = result_dir / "raw"
    raw_dir.mkdir(parents=True, exist_ok=True)
    env = os.environ.copy()
    env.update(CLUAIZ_HOME=str(args.home), CLUAIZ_PORT=str(args.port),
               CLUAIZ_HOST="127.0.0.1", RUN39_RAW_DIR=str(raw_dir))
    if args.variant == "hint":
        env["RUN39_TOOL_HINT"] = HINT
    setup = {"variant": args.variant, "models": args.models, "port": args.port,
             "start_free_ram_gib": free_ram_gib(), "start_free_vram_mib": free_vram_mib(),
             "hint": HINT if args.variant == "hint" else None}
    (result_dir / "setup.json").write_text(json.dumps(setup, indent=2), encoding="utf-8")
    print(json.dumps(setup), flush=True)
    with (raw_dir / "engine.log").open("a", encoding="utf-8") as log:
        engine = subprocess.Popen([str(args.exe), "serve"], env=env,
                                  stdout=log, stderr=subprocess.STDOUT)
        try:
            deadline = time.monotonic() + 120
            while time.monotonic() < deadline:
                if engine.poll() is not None:
                    raise RuntimeError(f"Engine exited {engine.returncode}; inspect engine.log")
                try:
                    urllib.request.urlopen(f"http://127.0.0.1:{args.port}/health", timeout=2)
                    break
                except Exception:
                    time.sleep(1)
            else:
                raise RuntimeError("health check timed out")
            token = args.home / "session.token"
            if not token.exists():
                raise RuntimeError("isolated session token was not created")
            command = [sys.executable, str(folder / "probe.py"),
                       f"http://127.0.0.1:{args.port}", str(token), *args.models]
            with (result_dir / "results.txt").open("a", encoding="utf-8") as results:
                child = subprocess.run(command, cwd=result_dir, env=env,
                                       stdout=results, stderr=subprocess.STDOUT)
            return child.returncode
        finally:
            engine.terminate()
            try:
                engine.wait(timeout=15)
            except subprocess.TimeoutExpired:
                engine.kill()
                engine.wait()
            setup.update(end_free_ram_gib=free_ram_gib(), end_free_vram_mib=free_vram_mib())
            (result_dir / "setup.json").write_text(json.dumps(setup, indent=2), encoding="utf-8")
            print(json.dumps({"end_free_ram_gib": setup["end_free_ram_gib"],
                              "end_free_vram_mib": setup["end_free_vram_mib"]}), flush=True)


if __name__ == "__main__":
    sys.exit(run())
