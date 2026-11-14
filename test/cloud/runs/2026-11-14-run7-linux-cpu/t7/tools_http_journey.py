"""tools.* real executable journey with a local registry/ZIP and fresh isolated home.

No public package is downloaded, no native plugin/MCP process or editor is launched.
The real existing installer downloads, extracts, registers and removes test skill files.
"""
import argparse
import io
import json
import sys
import threading
import urllib.error
import uuid
import zipfile
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path

sys.dont_write_bytecode = True
from journey_home import REPO, digest, prepare, save, validate_paths

sys.path.insert(0, str(REPO / "test/context-runtime"))
from harness import Engine


class RegistryFixture:
    def __init__(self, name, text):
        archive = io.BytesIO()
        with zipfile.ZipFile(archive, "w", zipfile.ZIP_DEFLATED) as bundle:
            bundle.writestr("SKILL.md", text)
            bundle.writestr("package.json", json.dumps({"name": name, "version": "0.1.0",
                "functions": [{"name": "inspect_fixture", "parameters": {"type": "object"}}]}))
        self.bundle = archive.getvalue()
        self.requests = 0
        fixture = self

        class Handler(BaseHTTPRequestHandler):
            def log_message(self, *_):
                pass

            def do_GET(self):
                fixture.requests += 1
                data = {
                    "/registry.json": {"routing": {"skills": "skills/index.json"}},
                    "/skills/index.json": {"items": {name: "fixture/package.json", "failed-install": "failed/package.json"}},
                    "/skills/fixture/package.json": {"latest_version": "0.1.0", "versions": {"0.1.0":
                        {"files": {"file_directory": fixture.url + "/bundle.zip"}}}},
                    "/skills/failed/package.json": {"latest_version": "0.1.0", "versions": {"0.1.0":
                        {"files": {"file_directory": fixture.url + "/broken.zip"}}}},
                }
                if self.path == "/bundle.zip":
                    body, content_type = fixture.bundle, "application/zip"
                elif self.path in data:
                    body, content_type = json.dumps(data[self.path]).encode(), "application/json"
                else:
                    self.send_error(503 if self.path == "/broken.zip" else 404)
                    return
                self.send_response(200)
                self.send_header("Content-Type", content_type)
                self.send_header("Content-Length", str(len(body)))
                self.end_headers()
                self.wfile.write(body)

        self.server = ThreadingHTTPServer(("127.0.0.1", 0), Handler)
        self.url = f"http://127.0.0.1:{self.server.server_port}"
        self.thread = threading.Thread(target=self.server.serve_forever, daemon=True)

    def start(self):
        self.thread.start()

    def stop(self):
        if self.thread.is_alive():
            self.server.shutdown()
        self.server.server_close()
        if self.thread.ident is not None:
            self.thread.join(timeout=5)


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
    name = "cep-skill-" + uuid.uuid4().hex[:12]
    text = f"---\nname: {name}\ndescription: Inspect fixture metadata\n---\n\nLocal archive instructions.\n"
    fixture = RegistryFixture(name, text)
    engine = Engine(url=f"http://127.0.0.1:{args.port}", exe=str(binary), home=str(home),
                    workspace=str(workspace), log=str(home / "engine.log"))
    report = {"executable_sha256": source_hash, "port": args.port, "checks": []}

    def check(label, condition):
        if not condition:
            raise AssertionError(label)
        report["checks"].append(label)

    def error(method, params, code):
        reply = engine.post("/v1/cep", {"jsonrpc": "2.0", "id": 1, "method": method, "params": params}, timeout=30)
        check(method + " typed error", reply.get("error", {}).get("code") == code and "result" not in reply)

    failure = None
    try:
        fixture.start()
        engine.start()
        # CLI startup materializes its master package registry. Apply the existing
        # registry configuration afterwards, in this disposable home only.
        package_path = home / "engine/config/package.json"
        package = json.loads(package_path.read_text(encoding="utf-8"))
        package.setdefault("web", {}).setdefault("tools", {})["manifest_url"] = fixture.url + "/registry.json"
        save(package_path, package)
        task = engine.cep("tools.install", kind="skill", name=name)
        task_id = task["task"]["task_id"]
        installed = engine.cep("tasks.result", task_id=task_id, wait_ms=30000)
        check("actual install task result", installed["id"] == name)
        check("task completed", engine.cep("tasks.get", task_id=task_id)["status"] == "completed")
        directory = home / "tools/skills" / name
        check("actual archive extracted", (directory / "SKILL.md").read_text(encoding="utf-8") == text)
        check("installer fetched the local registry and bundle", fixture.requests >= 4)
        listed = engine.cep("tools.list", kind="skill")
        entry = next(e for e in listed if e["id"] == name)
        check("installed metadata and functions", entry["enabled"] is True
              and entry["functions"][0]["name"] == "inspect_fixture")
        check("thirteen tool methods", len(engine.cep("rpc.discover", group="tools")["methods"]) == 13)
        files = engine.cep("tools.files", kind="skill", name=name)
        check("package file tree", any(f["path"] == "SKILL.md" for f in files["files"]))
        check("actual file read", engine.cep("tools.read_file", kind="skill", name=name, path="SKILL.md")["text"] == text)
        updated = "Changed UTF-8 नमस्ते\n"
        written = engine.cep("tools.write_file", kind="skill", name=name, text=updated)
        check("file write actual bytes", written["bytes"] == len(updated.encode())
              and (directory / "SKILL.md").read_text(encoding="utf-8") == updated)
        check("existing skill executor", engine.cep("tools.call", kind="skill", name=name, input={}) == updated)
        engine.cep("tools.set_settings", kind="skill", name=name, patch={"enabled": False})
        check("settings persisted", engine.cep("tools.get_settings", kind="skill", name=name)["values"]["enabled"] is False)
        error("tools.call", {"kind": "skill", "name": name, "input": {}}, -32010)
        engine.cep("tools.set_settings", kind="skill", name=name, patch={"enabled": True})
        (directory / ".cache").mkdir()
        (directory / ".cache/temp_a.tmp").write_bytes(b"123")
        (directory / ".cache/keep.txt").write_bytes(b"4567")
        cache = engine.cep("tools.cache", kind="skill", name=name)
        check("cache actual counts", cache[0]["all_bytes"] == 7 and cache[0]["temp_bytes"] == 3)
        cleared = engine.cep("tools.clear_cache", kind="skill", name=name)
        check("cache cleared without uninstall", cleared == {"removed": 2, "freed_bytes": 7}
              and list((directory / ".cache").iterdir()) == [] and (directory / "SKILL.md").is_file())
        requests = fixture.requests
        for bad in ("../escape", "a/b", "a\\b", "UPPER", "a@1", "a\0b"):
            error("tools.install", {"kind": "skill", "name": bad}, -32602)
        check("invalid install sends no registry requests", fixture.requests == requests)
        error("tools.read_file", {"kind": "skill", "name": name, "path": "../outside"}, -32602)
        failed = engine.cep("tools.install", kind="skill", name="failed-install")
        error("tasks.result", {"task_id": failed["task"]["task_id"], "wait_ms": 30000}, -32010)
        check("failed install has no package", not (home / "tools/skills/failed-install").exists())
        suggested = engine.cep("tools.discover", query="use " + name, explain=True)
        check("existing matcher result", suggested["installed"] >= 1 and isinstance(suggested["candidates"], list))
        removed = engine.cep("tools.remove", kind="skill", name=name)
        check("actual removal", removed == {"kind": "skill", "name": name} and not directory.exists())
        check("registry removal visible", all(e["id"] != name for e in engine.cep("tools.list", kind="skill")))
    except Exception as exception:
        failure = exception
        report["failure"] = {"type": type(exception).__name__, "message": str(exception)}
    finally:
        engine.stop()
        fixture.stop()
        report["owned_process_stopped"] = engine.proc is None
        report["registry_stopped"] = not fixture.thread.is_alive()
        report["local_registry_requests"] = fixture.requests
        report["seed_config_unchanged"] = all(digest(seed / "engine/config" / name) == value
                                               for name, value in seed_hashes.items())
        report["passed"] = failure is None and report["seed_config_unchanged"] and report["registry_stopped"]
        save(home / "tools-http-report.json", report)
        print(json.dumps(report, ensure_ascii=False, indent=2))
    if failure is not None:
        raise failure
    if not report["passed"]:
        raise RuntimeError("Tool journey isolation check failed")


if __name__ == "__main__":
    main()
