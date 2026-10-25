"""B3 model controls against a frozen real executable and a fresh isolated home.

Reads existing external GGUF headers. Removal affects only a newly created small
test package. Optional --check-load loads one real installed model without inference.
"""
import argparse
import copy
import json
import os
import sys
import urllib.error
import uuid
from pathlib import Path

sys.dont_write_bytecode = True
from golden_capture import REPO, digest, prepare, save, validate_paths

sys.path.insert(0, str(REPO / "test/context-runtime"))
from harness import Engine


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ("exe", "seed-home", "home"):
        parser.add_argument("--" + name, required=True, type=Path)
    parser.add_argument("--port", required=True, type=int)
    parser.add_argument("--check-load", action="store_true")
    args = parser.parse_args()
    exe, seed, home = (p.resolve() for p in (args.exe, args.seed_home, args.home))
    validate_paths(exe, seed, home, args.port)
    source_hash = digest(exe)
    binary, workspace, seed_hashes = prepare(exe, seed, home)
    if digest(binary) != source_hash:
        raise RuntimeError("Executable changed while copying")
    config = home / "engine/config"
    registry_path = config / "model_registry.json"
    registry = json.loads(registry_path.read_text(encoding="utf-8"))
    installed = registry["installed_models"]
    gguf = [e for e in installed.values() if e["format_type"] == "gguf" and e["category"] == "chat"]
    if not gguf:
        raise RuntimeError("Seed needs an installed GGUF chat package")
    selected = min(gguf, key=lambda e: sum(f["size_bytes"] for f in e["files"]))
    load_entry = None
    source_weight = None
    if args.check_load:
        # A new hard link uses no second weight-file allocation. Removing this owned
        # directory removes only its link; the original installed weight remains.
        primary = next(f for f in selected["files"] if f.get("is_primary"))
        source_weight = Path(selected["local_dir"]) / primary["name"]
        load_entry = copy.deepcopy(selected)
        load_entry["id"] = "cep-native-" + uuid.uuid4().hex
        load_dir = home / "models/test" / load_entry["id"]
        load_dir.mkdir(parents=True)
        os.link(source_weight, load_dir / primary["name"])
        load_entry.update(local_dir=str(load_dir), files=[primary], extra_files=[])
        installed[load_entry["id"]] = load_entry
    fixture_id = "cep-disposable-" + uuid.uuid4().hex
    fixture = home / "models/test" / fixture_id
    fixture.mkdir(parents=True)
    (fixture / "removal-only.txt").write_text("Disposable removal fixture, not inference weights.", encoding="utf-8")
    installed[fixture_id] = {
        "id": fixture_id, "category": "test", "format_type": "test-fixture",
        "huggingface_repo": "", "local_dir": str(fixture),
        "files": [{"name": "removal-only.txt", "is_primary": True,
                   "size_bytes": (fixture / "removal-only.txt").stat().st_size}],
        "supported_tasks": [], "requires_gpu": False,
        "metadata": {"architecture": "test-only", "parameters": "unknown", "context_window": "unknown",
                     "quantization": None, "bit_depth": None, "chat_template": None}}
    save(registry_path, registry)
    if args.check_load:
        # Bounded CPU-only load in this disposable settings copy; user/seed settings stay untouched.
        settings_path = config / "settings.json"
        settings = json.loads(settings_path.read_text(encoding="utf-8"))
        settings["optimization"].update(n_gpu_layers=0, custom_context_window=512,
                                        batch_size=16, ubatch_size=16, n_threads=2, n_threads_batch=2)
        save(settings_path, settings)
    engine = Engine(url=f"http://127.0.0.1:{args.port}", exe=str(binary), home=str(home),
                    workspace=str(workspace), log=str(home / "engine.log"))
    report = {"executable_sha256": source_hash, "port": args.port, "checks": [],
              "real_model_load_requested": args.check_load, "real_model_load_completed": False}

    def check(name, condition):
        if not condition:
            raise AssertionError(name)
        report["checks"].append(name)

    def error(method, params, code):
        reply = engine.post("/v1/cep", {"jsonrpc": "2.0", "id": 1, "method": method, "params": params}, timeout=30)
        check(method + " error", reply.get("error", {}).get("code") == code and "result" not in reply)

    failure = None
    try:
        engine.start()
        listed = engine.cep("models.list")
        listed_ids = {e["id"] for e in listed}
        check("all installed IDs", listed_ids == set(installed))
        for entry in listed:
            original = installed[entry["id"]]
            check("registry domain fields", entry["category"] == original["category"]
                  and entry["format"] == original["format_type"]
                  and entry["size"] == sum(f["size_bytes"] for f in original["files"])
                  and "metadata" in entry and "status" not in entry)
        check("OpenAI IDs match registry", {e["id"] for e in engine.get("/v1/models")["data"]} == listed_ids)
        check("Ollama IDs match registry", {e["model"] for e in engine.get("/api/tags")["models"]} == listed_ids)
        filtered = engine.cep("models.list", category="chat")
        check("category filtering", {e["id"] for e in filtered} == {e["id"] for e in listed if e["category"] == "chat"})
        inspected = engine.cep("models.inspect", model=selected["id"])
        check("real GGUF header", bool(inspected["metadata"].get("general.architecture"))
              and len(inspected["tensors"]) > 0 and inspected["model"] == selected["id"])
        for method in ("models.load", "models.inspect", "models.remove", "models.download"):
            for model in ("../escape", "a/b", "a\\b", "a\0b"):
                error(method, {"model": model}, -32602)
        for method in ("models.load", "models.inspect", "models.remove"):
            error(method, {"model": "missing-" + uuid.uuid4().hex}, -32004)
        error("models.download", {"model": "registry-" + uuid.uuid4().hex}, -32007)
        error("models.inspect", {"model": selected["id"], "file": "../escape.gguf"}, -32602)
        available_gb = engine.cep("system.live")["memory"]["available_gb"]
        required_gb = sum(f["size_bytes"] for f in selected["files"]) / (1024 ** 3) + 2
        load_allowed = args.check_load and available_gb >= required_gb
        if args.check_load and not load_allowed:
            report["load_skipped_reason"] = f"Available RAM {available_gb:.2f} GiB below test budget {required_gb:.2f} GiB"
        if load_allowed:
            loaded = engine.cep("models.load", model=load_entry["id"])
            check("real model load", loaded["model"] == load_entry["id"] and loaded["loaded_ms"] >= 0)
            active = engine.cep("system.ps")["active_processes"]
            check("loaded model reported", any(e["model_id"] == load_entry["id"] for e in active))
            report["load_ms"] = loaded["loaded_ms"]
            report["real_model_load_completed"] = True
        result = engine.cep("models.remove", model=fixture_id)
        check("only disposable directory removed", result["model"] == fixture_id and not fixture.exists())
        check("registry removed", fixture_id not in json.loads(registry_path.read_text(encoding="utf-8"))["installed_models"])
        check("external model remains", Path(selected["local_dir"]).is_dir())
        if load_allowed:
            check("unrelated removal preserves loaded model", any(e["model_id"] == load_entry["id"]
                  for e in engine.cep("system.ps")["active_processes"]))
            removed = engine.cep("models.remove", model=load_entry["id"])
            check("loaded test package removed", removed["model"] == load_entry["id"]
                  and not Path(load_entry["local_dir"]).exists())
            check("loaded test package unloaded first", engine.cep("system.ps")["active_processes"] == [])
            check("original weight preserved after hard-link removal", source_weight.is_file()
                  and source_weight.stat().st_size == primary["size_bytes"])
        for path in ("/models/available", "/v1/models/installed", "/models/download", "/models/load",
                     "/v1/models/" + selected["id"] + "/inspect_raw_header", "/v1/models/" + selected["id"]):
            try:
                response = engine._request("GET", path, timeout=10)
            except urllib.error.HTTPError as exception:
                response = exception
            with response:
                check("retired model route", response.code == 404)
    except Exception as exception:
        failure = exception
        report["failure"] = {"type": type(exception).__name__, "message": str(exception)}
    finally:
        engine.stop()
        report["owned_process_stopped"] = engine.proc is None
        report["seed_config_unchanged"] = all(digest(seed / "engine/config" / name) == value
                                               for name, value in seed_hashes.items())
        report["passed"] = failure is None and report["seed_config_unchanged"]
        save(home / "models-http-report.json", report)
        print(json.dumps(report, ensure_ascii=False, indent=2))
    if failure is not None:
        raise failure
    if not report["passed"]:
        raise RuntimeError("Model journey isolation check failed")


if __name__ == "__main__":
    main()
