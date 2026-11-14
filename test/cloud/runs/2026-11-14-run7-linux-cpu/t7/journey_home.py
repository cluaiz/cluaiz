"""Disposable Engine homes for the live CEP journeys in this folder (tests only).

A journey copies the config of an isolated seed home (never weights, `session.token` or live
tool state) into a fresh home, plus a small workspace fixture, and runs the given executable on
a separate port. The owner's own home (`~/.cluaiz`) and ports 8000 / 8011 are refused.
"""

import hashlib
import json
import shutil
import socket
import sys
from pathlib import Path

sys.dont_write_bytecode = True
REPO = Path(__file__).resolve().parents[3]
CONFIG_FILES = ("settings.json", "permission.json", "hardware_profile.json",
                "model_registry.json")
FIXTURE_TEXT = "CEP journey fixture: UTF-8 file, document extraction, and local tools.\n"
SKILL_ID = "cep-journey"


def digest(path):
    with path.open("rb") as source:
        return hashlib.file_digest(source, "sha256").hexdigest()


def save(path, value):
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(value, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")


def validate_paths(exe, seed, home, port):
    if port in (8000, 8011) or not 1024 <= port <= 65535:
        raise ValueError("Choose a separate unprivileged journey port; never 8000 or 8011")
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
    (temp / "journey.txt").write_text(FIXTURE_TEXT, encoding="utf-8")
    skill = home / "tools/skills" / SKILL_ID
    skill.mkdir(parents=True)
    (skill / "SKILL.md").write_text(
        "---\nname: cep-journey\ndescription: Local CEP journey fixture.\n---\n\n"
        + FIXTURE_TEXT, encoding="utf-8")
    return binary, workspace, hashes
