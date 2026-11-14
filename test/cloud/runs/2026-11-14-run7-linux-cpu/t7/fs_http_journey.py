"""fs.* and workspace.* live owner journey.

Starts only a frozen executable copy in a fresh isolated home on an explicit test port.
Uses the Engine harness and journey_home.py. Never builds or opens a dialog.
"""
import argparse
import json
import sys
import urllib.error
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
    exe, seed, home = (path.resolve() for path in (args.exe, args.seed_home, args.home))
    validate_paths(exe, seed, home, args.port)
    source_hash = digest(exe)
    binary, workspace, seed_hashes = prepare(exe, seed, home)
    if digest(binary) != source_hash:
        raise RuntimeError("Executable changed while copying")
    engine = Engine(url=f"http://127.0.0.1:{args.port}", exe=str(binary),
                    home=str(home), log=str(home / "engine.log"))
    report = {"executable_sha256": source_hash, "port": args.port, "checks": []}

    def check(name, condition):
        if not condition:
            raise AssertionError(name)
        report["checks"].append(name)

    def call(method, params, error=None):
        if engine.proc.poll() is not None:
            raise RuntimeError("Owned Engine exited")
        reply = engine.post("/v1/cep", {"jsonrpc": "2.0", "id": 1,
                                      "method": method, "params": params}, timeout=30)
        check(method + " envelope", reply.get("jsonrpc") == "2.0" and reply.get("id") == 1)
        if error is not None:
            check(method + " typed error", reply.get("error", {}).get("code") == error)
            return reply["error"]
        check(method + " result", "result" in reply and "error" not in reply)
        return reply["result"]

    failure = None
    try:
        engine.start()
        check("owned session", (home / "session.token").is_file() and engine.proc.poll() is None)
        call("workspace.set", {"path": str(workspace)})
        current = call("workspace.get", {})
        check("workspace round trip", Path(current["path"]).resolve() == workspace.resolve())
        text = "CEP HTTP journey: UTF-8 नमस्ते\n"
        written = call("fs.write", {"path": "journey/notes.md", "text": text})
        check("UTF-8 byte count", written["bytes"] == len(text.encode()))
        read = call("fs.read", {"path": "journey/notes.md"})
        check("UTF-8 contents", read.get("text") == text and read.get("binary") is False)
        call("fs.write", {"path": "journey/bytes.dat", "data_base64": "AP8BgA=="})
        binary_result = call("fs.read", {"path": "journey/bytes.dat"})
        check("binary contents", binary_result.get("binary") is True
              and binary_result.get("data_base64") == "AP8BgA==")
        call("fs.mkdir", {"path": "journey/sub"})
        check("mkdir effect", (workspace / "journey/sub").is_dir())
        call("fs.copy", {"from": "journey/notes.md", "to": "journey/copy.md"})
        call("fs.rename", {"from": "journey/copy.md", "to": "journey/renamed.md"})
        check("copy and rename effects", not (workspace / "journey/copy.md").exists()
              and (workspace / "journey/renamed.md").read_text(encoding="utf-8") == text)
        listing = call("fs.list", {"path": "journey", "recursive": True})
        check("renamed list fields", any(item.get("path") == "journey/renamed.md"
              and item.get("is_folder") is False and item.get("size") == len(text.encode())
              for item in listing.get("items", [])))
        call("fs.delete", {"path": "journey/renamed.md"})
        check("delete effect", not (workspace / "journey/renamed.md").exists())
        call("fs.write", {"path": "invalid/new.md", "text": "a", "data_base64": "Yg=="}, -32602)
        check("invalid payload creates nothing", not (workspace / "invalid").exists())
        missing = call("fs.read", {"path": "missing.txt"}, -32004)
        check("missing file type", missing.get("data", {}).get("type") == "file_not_found")
        discovery = call("rpc.discover", {"group": "fs"})
        check("all eight fs methods", len(discovery.get("methods", [])) == 8)
    except Exception as error:
        failure = error
        report["failure"] = {"type": type(error).__name__, "message": str(error)}
    finally:
        engine.stop()
        report["owned_process_stopped"] = engine.proc is None
        report["seed_config_unchanged"] = all(digest(seed / "engine/config" / name) == value
                                               for name, value in seed_hashes.items())
        report["passed"] = failure is None and report["seed_config_unchanged"]
        save(home / "fs-http-report.json", report)
        print(json.dumps(report, ensure_ascii=False, indent=2))
    if failure is not None:
        raise failure
    if not report["passed"]:
        raise RuntimeError("HTTP journey isolation check failed")


if __name__ == "__main__":
    main()
