"""Run remaining Run77 models sequentially on the named VM's isolated homes.

No Engine source edits. The operator starts this from ~/run77 on the VM. The
script never touches a non-run77 model vault and releases each generated GGUF
after recording its file name, byte length and SHA-256.
"""
import hashlib
import json
import os
import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent
assert ROOT.name == "run77" and ROOT.parent == Path.home()
EXE = Path.home() / "cluaiz/target/debug/cluaiz"
HOMES = {"A": ROOT / "homeA", "B": ROOT / "home", "C": ROOT / "homeA"}
MODELS = [
    ("qwen17", "unsloth/Qwen3-1.7B-GGUF:Q4_K_M", True),
    ("qwen4", "unsloth/Qwen3-4B-GGUF:Q4_K_M", True),
    ("qwen3508", "unsloth/Qwen3.5-0.8B-GGUF:Q4_K_M", True),
    ("qwen352", "unsloth/Qwen3.5-2B-GGUF:Q4_K_M", True),
    ("lfm17", "LiquidAI/LFM3-1.7B-GGUF:Q4_K_M", False),
    ("llama1", "bartowski/Llama-3.2-1B-Instruct-GGUF:Q4_K_M", False),
    ("llama3", "bartowski/Llama-3.2-3B-Instruct-GGUF:Q4_K_M", False),
    ("gemma1", "ggml-org/gemma-3-1b-it-GGUF:Q4_K_M", False),
    ("smol3", "ggml-org/SmolLM3-3B-GGUF:Q4_K_M", True),
]


def command(parts, home, timeout=1800):
    env = dict(os.environ, CLUAIZ_HOME=str(home), CLUAIZ_NONINTERACTIVE="1")
    p = subprocess.run(parts, env=env, text=True, capture_output=True, timeout=timeout)
    if p.returncode:
        raise RuntimeError(f"exit {p.returncode}: {(p.stdout + p.stderr)[-300:]}")
    return p.stdout + p.stderr


def record(label, event, **details):
    details = {k: re.sub(r"/home/[^/\\\s\"']+", "/home/<vps>", str(v)) for k, v in details.items()}
    with (ROOT / "progress.jsonl").open("a", encoding="utf-8") as f:
        f.write(json.dumps({"model": label, "event": event, **details}) + "\n")
    print(label, event, details, flush=True)


def model_file(home, model_id):
    files = [p for p in (home / "models/chat" / model_id).glob("*.gguf")
             if "mmproj" not in p.name.lower()]
    if len(files) != 1:
        raise RuntimeError(f"expected one GGUF, found {len(files)}")
    return files[0]


def digest(path):
    h = hashlib.sha256()
    with path.open("rb") as f:
        for block in iter(lambda: f.read(1024 * 1024), b""):
            h.update(block)
    return h.hexdigest()


def run_one(label, repo, thinking):
    record(label, "start", repository=repo)
    ids = []
    try:
        for setup in ("A", "B"):
            out = command([str(EXE), "pull", repo], HOMES[setup])
            m = re.search(r"Registered ID:\s+(\S+)", out)
            if not m:
                raise RuntimeError("pull gave no registered ID")
            ids.append((setup, m.group(1)))
            path = model_file(HOMES[setup], m.group(1))
            record(label, "model", setup=setup, model_id=m.group(1), file=path.name,
                   bytes=path.stat().st_size, sha256=digest(path))
        if ids[0][1] != ids[1][1]:
            raise RuntimeError("A and B model IDs differ")
        model_id = ids[0][1]
        for mode in (("off", "on") if thinking else ("off",)):
            for setup in "ABC":
                dest = ROOT / "raw" / f"{label}-{mode}-{setup}"
                out = command([sys.executable, str(ROOT / "matrix_probe.py"),
                               "--exe", str(EXE), "--home", str(HOMES[setup]),
                               "--out", str(dest), "--model", model_id,
                               "--setup", setup, "--think", mode], HOMES[setup], timeout=1800)
                rows = (dest / "raw.jsonl").read_text(encoding="utf-8").splitlines()
                if len(rows) != 40:
                    raise RuntimeError(f"only {len(rows)} rows in {setup} {mode}")
                record(label, "pass", setup=setup, think=mode, rows=len(rows))
        record(label, "complete")
    finally:
        for setup, model_id in ids:
            try:
                command([str(EXE), "rm", model_id], HOMES[setup])
                record(label, "removed_test_model", setup=setup)
            except Exception as exc:
                record(label, "cleanup_error", setup=setup, error=str(exc))


def main():
    start = sys.argv[1] if len(sys.argv) > 1 else MODELS[0][0]
    stop = sys.argv[2] if len(sys.argv) > 2 else None
    selected = False
    for label, repo, thinking in MODELS:
        if label == start:
            selected = True
        if not selected:
            continue
        try:
            run_one(label, repo, thinking)
        except Exception as exc:
            record(label, "error", error=str(exc))
        if label == stop:
            break
    if not selected:
        raise SystemExit(f"unknown start label: {start}")


if __name__ == "__main__":
    main()
