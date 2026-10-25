"""Writes docs/api/cep/openrpc.json and docs/api/cep/methods.md from a running Engine's `rpc.discover`.

    python test/cep/export_openrpc.py --exe target/release/cluaiz.exe --home <isolated CLUAIZ_HOME> [--port 8011] [--check]

The method reference is never written by hand: the registry is the one source. `--check` only
compares, and fails when the committed files are out of date.
"""
import argparse
import json
import os
import subprocess
import sys
import time
import urllib.request
from pathlib import Path

DOCS = Path(__file__).resolve().parents[2] / "docs" / "api" / "cep"


def schema_type(schema, components):
    if not isinstance(schema, dict):
        return "any"
    if "$ref" in schema:
        return schema["$ref"].rsplit("/", 1)[-1]
    if "anyOf" in schema or "oneOf" in schema:
        options = [schema_type(s, components) for s in schema.get("anyOf", schema.get("oneOf", []))]
        return " | ".join(o for o in options if o != "null") + (" | null" if "null" in options else "")
    kind = schema.get("type", "any")
    if isinstance(kind, list):
        kind = " | ".join(kind)
    if kind == "array":
        return f"{schema_type(schema.get('items', {}), components)}[]"
    if "enum" in schema:
        return " | ".join(json.dumps(v) for v in schema["enum"])
    return kind


def cell(text):
    return (text or "").replace("\n", " ").replace("|", "\\|")


def markdown(doc):
    components = doc.get("components", {}).get("schemas", {})
    groups = {}
    for method in doc["methods"]:
        groups.setdefault(method["name"].split(".")[0], []).append(method)
    lines = [
        "# CEP method reference",
        "",
        f"Generated from `rpc.discover` of Cluaiz Engine {doc['info']['version']} (CEP {doc['info'].get('x-cep-version')}) by "
        "`test/cep/export_openrpc.py`. Do not edit by hand. The machine-readable form is "
        "[`openrpc.json`](openrpc.json); concepts are in [README.md](README.md).",
        "",
        "| Group | Methods |",
        "|---|---|",
    ]
    for group, methods in sorted(groups.items()):
        lines.append(f"| `{group}` | " + ", ".join(f"[`{m['name']}`](#{m['name'].replace('.', '')})" for m in methods) + " |")
    for group, methods in sorted(groups.items()):
        lines += ["", f"## {group}"]
        for method in methods:
            cep = method.get("x-cep", {})
            flags = [f"scope `{cep.get('scope')}`", f"`{cep.get('execution')}`"]
            flags += [name for name in ("destructive", "owner_only") if cep.get(name)]
            flags.append("MCP tool" if cep.get("mcp") else "not on MCP")
            lines += ["", f"### {method['name']}", "", f"{method.get('summary', '')}", "", " · ".join(flags), ""]
            params = method.get("params", [])
            if params:
                lines += ["| Param | Type | Required | Description |", "|---|---|---|---|"]
                for p in params:
                    schema = p.get("schema") if isinstance(p.get("schema"), dict) else {}
                    lines.append(f"| `{p['name']}` | {cell(schema_type(schema, components))} | {'yes' if p.get('required') else 'no'} | "
                                 f"{cell(schema.get('description'))} |")
            else:
                lines.append("No params.")
            for example in method.get("examples", []):
                params = {p["name"]: p["value"] for p in example.get("params", [])}
                request = {"jsonrpc": "2.0", "id": 1, "method": method["name"], "params": params}
                lines += ["", "```json", json.dumps(request, ensure_ascii=False),
                          json.dumps({"jsonrpc": "2.0", "id": 1, "result": example.get("result", {}).get("value")}, ensure_ascii=False), "```"]
    return "\n".join(lines) + "\n"


def main():
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--exe", required=True)
    parser.add_argument("--home", required=True)
    parser.add_argument("--port", default="8011")
    parser.add_argument("--check", action="store_true")
    args = parser.parse_args()
    env = dict(os.environ, CLUAIZ_HOME=args.home, CLUAIZ_PORT=args.port, CLUAIZ_HOST="127.0.0.1")
    server = subprocess.Popen([args.exe, "serve"], env=env, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    try:
        for _ in range(90):
            try:
                urllib.request.urlopen(f"http://127.0.0.1:{args.port}/health", timeout=2)
                break
            except Exception:
                time.sleep(1)
        token = open(os.path.join(args.home, "session.token"), encoding="utf-8").read().strip()
        body = json.dumps({"jsonrpc": "2.0", "id": 1, "method": "rpc.discover"}).encode()
        request = urllib.request.Request(f"http://127.0.0.1:{args.port}/v1/cep", data=body,
                                         headers={"Content-Type": "application/json", "Authorization": f"Bearer {token}"})
        doc = json.load(urllib.request.urlopen(request, timeout=60))["result"]
    finally:
        server.kill()
    files = {DOCS / "openrpc.json": json.dumps(doc, indent=2, ensure_ascii=False) + "\n", DOCS / "methods.md": markdown(doc)}
    if args.check:
        stale = [p.name for p, text in files.items() if not p.exists() or p.read_text(encoding="utf-8") != text]
        print("stale: " + ", ".join(stale) if stale else f"up to date ({len(doc['methods'])} methods)")
        return 1 if stale else 0
    DOCS.mkdir(parents=True, exist_ok=True)
    for path, text in files.items():
        path.write_text(text, encoding="utf-8")
    print(f"wrote {len(doc['methods'])} methods to {DOCS}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
