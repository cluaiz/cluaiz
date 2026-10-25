"""B0: record the old Wave B routes against a frozen, isolated executable.

Uses the existing context-runtime Engine owner; never builds the shared tree.
Example (all paths explicit):
  python test/cep/wave_b/golden_capture.py --exe <old cluaiz.exe> \
    --seed-home <isolated test home> --home <fresh home> --port <test port>
Golden files preserve the actual HTTP status/body, including existing failures.
They are migration baselines, NOT assertions that the old implementation is correct.
"""

import argparse
import base64
import hashlib
import importlib.util
import json
import re
import shutil
import socket
import sys
import time
import urllib.error
import urllib.parse
import uuid
from datetime import datetime, timezone
from pathlib import Path


sys.dont_write_bytecode = True
REPO = Path(__file__).resolve().parents[3]
OWN_ROWS = {16, 17, 19, 23, 24, 26, 27, 28, 29, 30, 33, 34,
            *range(58, 68), *range(70, 83), *range(87, 90), 92, 93,
            101, 102, *range(109, 119)}
SECRET_KEYS = {"session_token", "api_token", "api_key", "password", "secret",
               "authorization", "bearer_token", "hostname"}
CONFIG_FILES = ("settings.json", "permission.json", "hardware_profile.json",
                "model_registry.json")
FIXTURE_TEXT = "CEP migration baseline: UTF-8 file, document extraction, and local tools.\n"
SKILL_ID = "cep-baseline"


def read_json(path):
    return json.loads(path.read_text(encoding="utf-8-sig"))


def digest(path):
    with path.open("rb") as source:
        return hashlib.file_digest(source, "sha256").hexdigest()


def save(path, value):
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(value, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")


def catalog(path):
    """Read the approved mapping, rather than maintaining a second route inventory."""
    rows = []
    for line in path.read_text(encoding="utf-8").splitlines():
        cells = [cell.strip() for cell in line.split("|")]
        if len(cells) < 5 or not cells[1].isdigit() or int(cells[1]) not in OWN_ROWS:
            continue
        verb, route = cells[2].split(" ", 1)
        route = route.strip("`")
        match = re.search(r"`([a-z]+\.[a-z_]+)", cells[3])
        aliases = {30: "models.calibrate_removed", 88: "ingest.run"}
        method = match.group(1) if match else aliases.get(int(cells[1]))
        if method is None:
            raise ValueError(f"No method mapping for catalog row {cells[1]}")
        group = "models" if int(cells[1]) == 30 else method.split(".")[0]
        rows.append({"row": int(cells[1]), "verb": verb, "route": route,
                     "method": method, "group": group})
    found = {row["row"] for row in rows}
    if found != OWN_ROWS:
        raise ValueError(f"Catalog coverage changed; missing={sorted(OWN_ROWS - found)}")
    return rows


def validate_paths(exe, seed, home, port):
    if port in (8000, 8011) or not 1024 <= port <= 65535:
        raise ValueError("Choose a separate unprivileged Wave B port; never 8000 or 8011")
    personal = Path.home() / ".cluaiz"
    if home.is_relative_to(personal.resolve()) or home.exists():
        raise ValueError("The destination home must be fresh; never use the personal home")
    if seed.is_relative_to(personal.resolve()) or not (seed / "engine/config").is_dir():
        raise ValueError("Seed must be an existing isolated test home, not ~/.cluaiz")
    if seed.is_relative_to(home) or home.is_relative_to(seed):
        raise ValueError("Seed and destination homes must be separate trees")
    if not exe.is_file() or (exe.parent / "portable.flag").exists():
        raise ValueError("Executable missing, or portable.flag would override CLUAIZ_HOME")
    with socket.socket() as probe:
        if sys.platform == "win32":
            probe.setsockopt(socket.SOL_SOCKET, socket.SO_EXCLUSIVEADDRUSE, 1)
        probe.bind(("127.0.0.1", port))


def prepare(exe, seed, home):
    """Copy selected config, never weights, session.token, or live tool state."""
    config = home / "engine/config"
    config.mkdir(parents=True)
    hashes = {}
    for name in CONFIG_FILES:
        source = seed / "engine/config" / name
        if source.is_file():
            shutil.copy2(source, config / name)
            hashes[name] = digest(source)
    binary = home / "bin" / exe.name
    binary.parent.mkdir()
    shutil.copy2(exe, binary)
    # Native optional dependencies are copied only when real, never zero-byte sentinels.
    for source in exe.parent.glob("*.dll"):
        if source.stat().st_size:
            shutil.copy2(source, binary.parent / source.name)
    workspace = home / "workspace"
    workspace.mkdir()
    (workspace / "notes.txt").write_text(FIXTURE_TEXT, encoding="utf-8")
    (workspace / "binary.dat").write_bytes(bytes([0, 255, 1, 128]))
    (workspace / "remove.txt").write_text("test-owned removal\n", encoding="utf-8")
    (workspace / "rename.txt").write_text("test-owned rename\n", encoding="utf-8")
    temp = home / "temp_media"
    temp.mkdir()
    (temp / "baseline.txt").write_text(FIXTURE_TEXT, encoding="utf-8")
    skill = home / "tools/skills" / SKILL_ID
    skill.mkdir(parents=True)
    (skill / "SKILL.md").write_text(
        "---\nname: cep-baseline\ndescription: Local CEP baseline fixture.\n---\n\n"
        + FIXTURE_TEXT, encoding="utf-8")
    return binary, workspace, hashes


class Capture:
    def __init__(self, engine, home, workspace, entries, timeout):
        self.engine, self.home, self.workspace = engine, home, workspace
        self.entries, self.timeout = entries, timeout
        self.groups, self.skips, self.failures, self.observations = {}, [], [], []
        self.replacements = [(str(workspace), "<workspace>"), (str(home), "<test_home>")]
        for entry in entries.values():
            self.replacements.append((entry["local_dir"], f"<model:{entry['id']}>"))
        self.replacements.sort(key=lambda pair: len(pair[0]), reverse=True)

    def sanitize(self, value):
        if isinstance(value, dict):
            return {key: "<redacted>" if key.lower() in SECRET_KEYS else self.sanitize(item)
                    for key, item in value.items()}
        if isinstance(value, list):
            return [self.sanitize(item) for item in value]
        if isinstance(value, str):
            for source, replacement in self.replacements:
                for variant in (source.replace("\\", "\\\\"), source,
                                source.replace("\\", "/")):
                    value = value.replace(variant, replacement)
            return value
        return value

    def skip(self, row, reason):
        self.skips.append({"route": row["route"], "verb": row["verb"],
                           "method": row["method"], "reason": reason})

    def request(self, row, body=None, query=None, path=None, expected=None, case="default"):
        path = path or row["route"]
        if query:
            path += "?" + urllib.parse.urlencode(query)
        if self.engine.proc.poll() is not None:
            raise RuntimeError("Owned baseline Engine exited; refusing to contact another process")
        started = time.monotonic()
        try:
            try:
                response = self.engine._request(row["verb"], path, body, self.timeout)
            except urllib.error.HTTPError as error:
                response = error
            with response:
                status = response.code
                raw = response.read().decode("utf-8", "replace")
                try:
                    result = json.loads(raw)
                except ValueError:
                    result = {"non_json_body": raw}
            record = {"route": row["route"], "verb": row["verb"], "method": row["method"],
                      "case": case, "request": {"path": path, "body": body},
                      "status": status, "body": result,
                      "seconds": round(time.monotonic() - started, 3)}
            if expected is not None and status not in expected:
                self.failures.append({"route": row["route"], "case": case,
                                      "expected": sorted(expected), "actual": status})
        except (OSError, TimeoutError) as error:
            self.failures.append({"route": row["route"], "case": case,
                                  "transport_error": str(error)})
            record = {"route": row["route"], "verb": row["verb"], "case": case,
                      "transport_error": str(error)}
        self.groups.setdefault(row["group"], []).append(self.sanitize(record))
        print(f"{row['verb']} {row['route']} [{case}]: {record.get('status', 'transport failure')}", flush=True)
        return record.get("body")

    def run(self, rows):
        present = [entry for entry in self.entries.values() if Path(entry["local_dir"]).is_dir()]
        gguf = next((entry for entry in present if entry.get("format_type") == "gguf"), None)
        absent = "cep-baseline-missing-" + uuid.uuid4().hex
        self.replacements.append((absent, "<missing_id>"))
        query = {"component_type": "skill", "component_id": SKILL_ID}
        # Mutations have a deliberate order; the catalog itself is not a workflow.
        order = {117: 0, 118: 1, 110: 2, 109: 3, 111: 4, 115: 5,
                 114: 6, 113: 7, 112: 8, 61: 80, 102: 90,
                 60: 98, 65: 98, 72: 98}
        for row in sorted(rows, key=lambda item: order.get(item["row"], 40)):
            n = row["row"]
            if n == 116:
                self.skip(row, "Interactive OS folder picker; would require founder input")
            elif n in (109, 110, 111, 112, 113, 114, 115):
                bodies = {
                    109: {"path": "notes.txt"},
                    110: {"path": "written.txt", "content": FIXTURE_TEXT},
                    111: {"dir_path": ".", "recursive": True},
                    112: {"path": "remove.txt"},
                    113: {"old_path": "rename.txt", "new_path": "renamed.txt"},
                    114: {"src_path": "notes.txt", "dest_path": "copied.txt"},
                    115: {"path": "created"},
                }
                self.request(row, dict(bodies[n], caller="user"), expected={200})
                if n == 109:
                    self.request(row, {"path": "binary.dat", "is_binary": True, "caller": "user"},
                                 expected={200}, case="binary")
                    self.request(row, {"path": "missing.txt", "caller": "user"},
                                 expected={404}, case="missing")
            elif n in (117, 118):
                self.request(row, {"workspace": str(self.workspace)} if n == 118 else None,
                             expected={200})
            elif n in (16, 17, 19):
                task = "tts" if n != 17 else "stt"
                available = next((entry for entry in present if entry.get("category") == task), None)
                if n == 19:
                    if not available:
                        self.request(row, {"input_source": {"type": "text", "data": FIXTURE_TEXT},
                                           "stream": False}, expected={404}, case="no_model")
                    self.skip(row, "No speech model in this isolated registry; no weights downloaded")
                else:
                    self.request(row, query={"model": available["id"]} if available else None,
                                 expected={200} if available else {404}, case="catalog" if available else "no_model")
                if not available and n != 19:
                    self.skip(row, f"Successful {task} catalog needs an installed speech model")
            elif n in (23, 24, 26, 30, 33, 58, 61, 63, 66, 70, 73, 75, 89, 92, 101, 102):
                self.request(row, {} if row["verb"] == "POST" else None, expected={200})
            elif n == 27:
                self.request(row, {"model_id": absent}, expected={404}, case="missing")
                self.skip(row, "Weight-loading success not captured by the lightweight B0 run")
            elif n == 28:
                entry = gguf
                path = row["route"].replace("{id}", urllib.parse.quote(entry["id"] if entry else absent, safe=""))
                result = self.request(row, path=path, expected={200})
                if isinstance(result, dict) and result.get("status") == "error":
                    self.observations.append({"route": row["route"], "status": 200,
                                              "issue": "Old inspect returns an error body with HTTP 200; registry package not resolved"})
            elif n == 29:
                self.request(row, path=row["route"].replace("{id}", absent), expected={404}, case="missing")
                self.skip(row, "Never remove an installed model with external read-only weights")
            elif n == 34:
                self.request(row, {"query": "Read local notes", "explain": True}, expected={200})
            elif n in (59, 64, 71):
                kind = {59: "skill", 64: "plugin", 71: "mcp"}[n]
                # The old wrapper maps installer validation errors to 500 (components.rs).
                # Preserve the defect as evidence; this is not a target assertion for CEP.
                self.request(row, {kind + "_name": "../outside"}, expected={500}, case="invalid_id")
                self.observations.append({"route": row["route"], "status": 500,
                                          "issue": "Invalid id rejected, but old wrapper reports an internal error"})
                self.skip(row, "Successful install reaches the external tool hub; no network install in B0")
            elif n in (60, 65, 72):
                kind = {60: "skill", 65: "plugin", 72: "mcp"}[n]
                self.request(row, {kind + "_name": SKILL_ID if kind == "skill" else absent},
                             expected={200} if kind == "skill" else None)
            elif n in (62, 67, 74):
                kind = {62: "skill", 67: "plugin", 74: "mcp"}[n]
                self.request(row, None if kind == "skill" else {kind + "_name": absent},
                             expected={200} if kind == "skill" else {404})
            elif n in (76, 80):
                self.request(row, query=query, expected={200})
            elif n == 77:
                self.request(row, dict(query, settings={"enabled": True}), expected=None)
            elif n == 78:
                self.request(row, query=dict(query, file_path="SKILL.md"), expected={200})
            elif n == 79:
                self.request(row, dict(query, content=FIXTURE_TEXT), expected={200})
            elif n == 81:
                self.request(row, dict(query, all=True), expected={200})
            elif n == 82:
                path = row["route"].replace("{component}", absent).replace("{function}", "missing")
                self.request(row, {"params": {}}, path=path, expected={500}, case="missing")
            elif n in (87, 88):
                inline = "data:text/plain;base64," + base64.b64encode(FIXTURE_TEXT.encode()).decode()
                self.request(row, {"source": inline, "output_controls": {"return_text": True}}, expected={200})
            elif n == 93:
                if gguf:
                    self.request(row, {"model": gguf["id"], "n_ctx": 512, "batch_tokens": 128}, expected={200})
                else:
                    self.skip(row, "No installed GGUF model in isolated registry for planning")
            else:
                raise ValueError(f"No explicit safe fixture for catalog row {n}")


def arguments():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--exe", required=True, type=Path)
    parser.add_argument("--seed-home", required=True, type=Path)
    parser.add_argument("--home", required=True, type=Path)
    parser.add_argument("--port", required=True, type=int)
    parser.add_argument("--timeout", type=float, default=60)
    parser.add_argument("--output", type=Path, default=Path(__file__).parent / "golden")
    parser.add_argument("--catalog", type=Path, default=REPO / ".agent/plans/CEP/03-method-catalog.md")
    return parser.parse_args()


def main():
    args = arguments()
    exe, seed, home = (path.resolve() for path in (args.exe, args.seed_home, args.home))
    validate_paths(exe, seed, home, args.port)
    if args.output.exists() and any(args.output.glob("*.json")):
        raise ValueError("Output already contains a baseline; use a fresh --output directory")
    rows = catalog(args.catalog)
    original_hash = digest(exe)
    binary, workspace, config_hashes = prepare(exe, seed, home)
    if digest(binary) != original_hash:
        raise RuntimeError("Executable changed during copy; capture requires a stable baseline")
    module_path = REPO / "test/context-runtime/harness.py"
    spec = importlib.util.spec_from_file_location("context_runtime_harness", module_path)
    harness = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(harness)
    engine = harness.Engine(url=f"http://127.0.0.1:{args.port}", exe=str(binary),
                            home=str(home), log=str(home / "engine.log"))
    capture = None
    try:
        engine.start()
        # Engine.start is shared; confirm the token really belongs to this fresh home.
        if not (home / "session.token").is_file() or engine.proc.poll() is not None:
            raise RuntimeError("Isolated Engine did not start with its own session token")
        registry_path = home / "engine/config/model_registry.json"
        entries = read_json(registry_path).get("installed_models", {}) if registry_path.exists() else {}
        capture = Capture(engine, home, workspace, entries, args.timeout)
        capture.run(rows)
    finally:
        engine.stop()
        if capture:
            for group, records in capture.groups.items():
                save(args.output / f"{group}.json", records)
            seed_unchanged = all(digest(seed / "engine/config" / name) == value
                                 for name, value in config_hashes.items())
            summary = {"captured_at": datetime.now(timezone.utc).isoformat(),
                       "baseline_executable_sha256": original_hash,
                       "baseline_source_revision": "unverified: existing binary, not rebuilt",
                       "catalog_sha256": digest(args.catalog), "port": args.port,
                       "groups": {key: len(value) for key, value in capture.groups.items()},
                       "catalog_routes": len(rows), "skipped": capture.skips,
                       "failures": capture.failures, "observations": capture.observations,
                       "seed_config_unchanged": seed_unchanged,
                       "owned_process_stopped": engine.proc is None}
            save(args.output / "summary.json", summary)
            print(json.dumps(summary, ensure_ascii=False, indent=2), flush=True)
            if not seed_unchanged or capture.failures:
                raise RuntimeError("Baseline has unexpected outcomes; inspect summary.json")


if __name__ == "__main__":
    main()
