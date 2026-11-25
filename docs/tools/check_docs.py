#!/usr/bin/env python3
"""Checks the Engine docs the way the website and GitHub read them (no network, no dependencies).

1. `docs/registry.json` (the website navigation): every page's `file_path` exists, slugs are unique.
2. Every relative link in `docs/**/*.md`, the root README and every folder README points to a file that exists, and a
   `#heading` anchor to a heading that exists in that file (GitHub's slug rules).
3. No page links to a local Windows path (`C:\\...`, `file://`), which works on nobody else's machine.
4. `docs/index.json` and `docs/index.md` match what `docs/tools/build_index.py` writes now. Every docs page and folder
   README has its own name and description at the top (`<!-- name: ... description: ... -->`).

External links (http/https) are not fetched here; that is a separate, scheduled check.

Run from the repository root:  python docs/tools/check_docs.py
Exit code 0 when clean, 1 with one line per problem.
"""

from __future__ import annotations

import json
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
DOCS = ROOT / "docs"

LINK = re.compile(r"(?<!!)\[[^\]]*\]\(([^)\s]+)(?:\s+\"[^\"]*\")?\)")
IMAGE = re.compile(r"!\[[^\]]*\]\(([^)\s]+)\)")
FENCE = re.compile(r"^\s*(```|~~~)")
HEADING = re.compile(r"^(#{1,6})\s+(.*?)\s*#*\s*$")
LOCAL_PATH = re.compile(r"\]\((?:file://|[A-Za-z]:[\\/])")


def slug(heading: str) -> str:
    """GitHub's anchor for a heading: lower case, markup and punctuation dropped, spaces to dashes."""
    text = re.sub(r"`([^`]*)`", r"\1", heading)
    text = re.sub(r"\[([^\]]*)\]\([^)]*\)", r"\1", text)
    text = text.strip().lower()
    text = re.sub(r"[^\w\- ]", "", text)
    return text.replace(" ", "-")


def outside_code(lines: list[str]):
    """Yields (line number, line) outside fenced code blocks."""
    fenced = False
    for number, line in enumerate(lines, 1):
        if FENCE.match(line):
            fenced = not fenced
            continue
        if not fenced:
            yield number, line


_anchor_cache: dict[Path, set[str]] = {}


def anchors(path: Path) -> set[str]:
    if path not in _anchor_cache:
        found: set[str] = set()
        seen: dict[str, int] = {}
        for _, line in outside_code(path.read_text(encoding="utf-8", errors="replace").splitlines()):
            match = HEADING.match(line)
            if match:
                base = slug(match.group(2))
                count = seen.get(base, 0)
                found.add(base if count == 0 else f"{base}-{count}")
                seen[base] = count + 1
            for explicit in re.findall(r'<a\s+(?:name|id)="([^"]+)"', line):
                found.add(explicit)
        _anchor_cache[path] = found
    return _anchor_cache[path]


def check_registry(problems: list[str]) -> None:
    registry = json.loads((DOCS / "registry.json").read_text(encoding="utf-8"))
    slugs: dict[str, str] = {}
    for section in registry:
        for group in section.get("groups", []):
            for page in group.get("pages", []):
                where = f"docs/registry.json [{section.get('id')} / {group.get('group')}]"
                file_path = page.get("file_path", "")
                if not (ROOT / file_path).is_file():
                    problems.append(f"{where}: page '{page.get('slug')}' points to a missing file {file_path}")
                key = f"{section.get('id')}/{page.get('slug')}"
                if key in slugs:
                    problems.append(f"{where}: slug '{page.get('slug')}' is used twice in section {section.get('id')}")
                slugs[key] = file_path


def submodule_roots() -> list[Path]:
    """Submodule folders from .gitmodules. CI checks out the repository without submodules, so a link into
    one (for example engine/llama/llama.cpp/CLUAIZ_PATCHES.md) cannot be checked there and is skipped."""
    gitmodules = ROOT / ".gitmodules"
    if not gitmodules.is_file():
        return []
    found = re.findall(r"^\s*path\s*=\s*(.+?)\s*$", gitmodules.read_text(encoding="utf-8"), re.M)
    return [(ROOT / rel).resolve() for rel in found]


SUBMODULES = submodule_roots()


def check_links(path: Path, problems: list[str]) -> None:
    lines = path.read_text(encoding="utf-8", errors="replace").splitlines()
    rel = path.relative_to(ROOT).as_posix()
    for number, line in outside_code(lines):
        if LOCAL_PATH.search(line):
            problems.append(f"{rel}:{number}: links to a local machine path")
        for target in LINK.findall(line) + IMAGE.findall(line):
            if re.match(r"^[a-z][a-z0-9+.-]*:", target, re.I):  # http:, https:, mailto:, ...
                continue
            file_part, _, anchor = target.partition("#")
            destination = path if not file_part else (path.parent / file_part).resolve()
            if file_part and not destination.exists():
                if any(destination.is_relative_to(sub) for sub in SUBMODULES):
                    continue
                problems.append(f"{rel}:{number}: link to a missing file {target}")
                continue
            if anchor and destination.suffix == ".md" and destination.is_file():
                if anchor not in anchors(destination):
                    problems.append(f"{rel}:{number}: link to a missing heading #{anchor} in {destination.relative_to(ROOT).as_posix()}")


SKIP_DIRS = {"target", "node_modules", ".git", ".agent", "llama.cpp"}


def code_readmes() -> list[Path]:
    """Folder READMEs outside docs/ (they link docs/assets images and source files too)."""
    found = []
    for path in ROOT.rglob("*"):
        if path.name.lower() == "readme.md" and path.is_file() and path != ROOT / "README.md":
            parts = set(path.relative_to(ROOT).parts)
            if not parts & SKIP_DIRS and path.relative_to(ROOT).parts[0] != "docs":
                found.append(path)
    return sorted(found)


def check_index(problems: list[str]) -> None:
    sys.path.insert(0, str(Path(__file__).resolve().parent))
    import build_index

    as_json, as_md = build_index.build()
    for path, want in ((build_index.INDEX_JSON, as_json), (build_index.INDEX_MD, as_md)):
        if not path.is_file() or path.read_text(encoding="utf-8") != want:
            problems.append(f"{path.relative_to(ROOT).as_posix()}: out of date; run python docs/tools/build_index.py")
    for row in json.loads(as_json):
        if not row["metadata"]:
            problems.append(f"{row['path']}: no name and description at the top (see .agents/skills/readme-docs, or docs/tools/README.md)")


def main() -> int:
    problems: list[str] = []
    check_registry(problems)
    pages = sorted(DOCS.rglob("*.md")) + [ROOT / "README.md"] + code_readmes()
    for page in pages:
        check_links(page, problems)
    check_index(problems)
    for problem in problems:
        print(problem)
    print(f"{len(pages)} pages checked, {len(problems)} problem(s)", file=sys.stderr)
    return 1 if problems else 0


if __name__ == "__main__":
    sys.exit(main())
