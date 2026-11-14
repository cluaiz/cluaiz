"""system.* and storage.* HTTP and real CLI journey in a fresh isolated Engine home.

Reuses the process harness and journey_home.py. Reads installed model
headers for planning; never loads weights, downloads models or touches user settings.
"""
import argparse
import json
import os
import subprocess
import sys
import urllib.error
import uuid
from pathlib import Path

sys.dont_write_bytecode = True
from journey_home import REPO, digest, prepare, save, validate_paths

sys.path.insert(0, str(REPO / "test/context-runtime"))
from harness import Engine


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ("exe", "seed-home", "home"):
        parser.add_argument("--" + name, required=True, type=Path)
    parser.add_argument("--port", required=True, type=int)
    args = parser.parse_args()
    exe, seed, home = (p.resolve() for p in (args.exe, args.seed_home, args.home))
    validate_paths(exe, seed, home, args.port)
    source_hash = digest(exe)
    binary, workspace, seed_hashes = prepare(exe, seed, home)
    if digest(binary) != source_hash:
        raise RuntimeError("Executable changed while copying")
    config = home / "engine/config"
    permission_path = config / "permission.json"
    permission = json.loads(permission_path.read_text(encoding="utf-8"))
    # Exercise the existing optional auth policy only in this freshly created home.
    permission.setdefault("api_auth", {})["required"] = True
    save(permission_path, permission)
    registry = json.loads((config / "model_registry.json").read_text(encoding="utf-8"))
    candidates = [entry for entry in registry["installed_models"].values()
                  if entry.get("format_type") == "gguf" and entry.get("category") == "chat"]
    if not candidates:
        raise RuntimeError("Seed needs an installed GGUF chat model for a real header plan")
    selected = min(candidates, key=lambda e: sum(f.get("size_bytes", 0) for f in e["files"]))
    engine = Engine(url=f"http://127.0.0.1:{args.port}", exe=str(binary), home=str(home),
                    workspace=str(workspace), log=str(home / "engine.log"))
    report = {"executable_sha256": source_hash, "port": args.port, "checks": [], "cli": []}

    def check(name, condition):
        if not condition:
            raise AssertionError(name)
        report["checks"].append(name)

    def cli(*command):
        env = dict(os.environ, CLUAIZ_HOME=str(home), CLUAIZ_PORT=str(args.port),
                   CLUAIZ_HOST="127.0.0.1", NO_COLOR="1")
        result = subprocess.run([str(binary), *command], env=env, capture_output=True,
                                text=True, encoding="utf-8", errors="replace", timeout=30)
        report["cli"].append({"command": list(command), "exit": result.returncode,
                              "stdout": result.stdout, "stderr": result.stderr})
        return result

    def rpc_error(method, params, code):
        reply = engine.post("/v1/cep", {"jsonrpc": "2.0", "id": 1,
                                      "method": method, "params": params}, timeout=30)
        check(method + " error envelope", reply.get("jsonrpc") == "2.0" and reply.get("id") == 1)
        check(method + " typed error", reply.get("error", {}).get("code") == code)
        return reply["error"]

    failure = None
    try:
        engine.start()
        live = engine.cep("system.live")
        check("live actual owned process", live["process_id"] == engine.pid())
        check("live telemetry fields", all(k in live for k in ("memory", "cpu", "gpus"))
              and "status" not in live)
        check("initial ps empty", engine.cep("system.ps")["active_processes"] == [])
        result = cli("ps")
        check("CLI ps uses CEP", result.returncode == 0 and "No model loaded" in result.stdout)
        settings_path = config / "settings.json"
        saved_hash = digest(settings_path)
        plan = engine.cep("system.plan", model=selected["id"], n_ctx=512, batch_tokens=16)
        check("real installed model plan", all(k in plan for k in
              ("model_info", "execution_plan", "hardware_info")) and "status" not in plan)
        check("plan does not load weights", engine.cep("system.ps")["active_processes"] == [])
        check("plan does not change settings", digest(settings_path) == saved_hash)
        rpc_error("system.plan", {"model": selected["id"], "batch_tokens": 0}, -32602)
        rpc_error("system.plan", {"model": selected["id"], "unexpected": True}, -32602)
        missing = rpc_error("system.plan", {"model": str(uuid.uuid4())}, -32004)
        check("missing model type", missing.get("data", {}).get("type") == "model_not_found")
        expected = sum(p.stat().st_size for p in (home / "temp_media").rglob("*") if p.is_file())
        usage = engine.cep("storage.status")
        check("storage counts actual fixture", usage["file_count"] == 1
              and usage["total_size_bytes"] == expected and expected > 0)
        result = cli("storage", "temp-status")
        check("CLI storage status", result.returncode == 0 and "Total Files: 1" in result.stdout)
        cleaned = engine.cep("storage.clean")
        check("cleanup actual byte count", cleaned == {"removed_files": 1, "freed_bytes": expected})
        check("cleanup actual filesystem", list((home / "temp_media").iterdir()) == [])
        (home / "temp_media/cli.txt").write_bytes(b"cli")
        result = cli("storage", "temp-clean")
        check("CLI cleanup counts actual bytes", result.returncode == 0
              and "1 files, 3 bytes" in result.stdout)
        check("storage is empty", engine.cep("storage.status")["total_size_bytes"] == 0)
        cache = home / "temp_media"
        cache.rmdir()
        cache.write_bytes(b"not a directory")
        try:
            result = cli("storage", "temp-status")
            check("CLI RPC error is failure", result.returncode != 0
                  and "storage.status" in result.stderr and "Total Files" not in result.stdout)
            check("invalid storage root preserved", cache.read_bytes() == b"not a directory")
        finally:
            cache.unlink()
            cache.mkdir()
        token_path = home / "session.token"
        original = token_path.read_text(encoding="utf-8")
        try:
            token_path.write_text(str(uuid.uuid4()), encoding="utf-8")
            result = cli("storage", "temp-status")
            check("CLI unauthorized is failure", result.returncode != 0
                  and "401" in result.stderr and "Total Files" not in result.stdout)
        finally:
            token_path.write_text(original, encoding="utf-8")
    except Exception as error:
        failure = error
        report["failure"] = {"type": type(error).__name__, "message": str(error)}
    finally:
        engine.stop()
        report["owned_process_stopped"] = engine.proc is None
        report["seed_config_unchanged"] = all(digest(seed / "engine/config" / name) == value
                                               for name, value in seed_hashes.items())
        report["passed"] = failure is None and report["seed_config_unchanged"]
        save(home / "system-http-report.json", report)
        print(json.dumps(report, ensure_ascii=False, indent=2))
    if failure is not None:
        raise failure
    if not report["passed"]:
        raise RuntimeError("System HTTP journey isolation check failed")


if __name__ == "__main__":
    main()
