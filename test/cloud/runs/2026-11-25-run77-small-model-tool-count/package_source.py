"""Package the current committed Engine build source without private files."""

import argparse
from pathlib import Path
import subprocess
import tarfile


BUILD_PATHS = (
    "Cargo.toml", "Cargo.lock", ".cargo", "agents", "api", "assets",
    "automation", "cep", "cmd", "context", "engine", "ffi", "security",
    "terminal", "tools", "test/plugin_testing",
)


def private_path(name):
    path = Path(name)
    return any(part in (".env", ".ssh", ".aws") for part in path.parts) or path.suffix.lower() in (
        ".pem", ".key", ".p12", ".pfx") or "credentials" in path.name.lower()


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--repo", type=Path, required=True)
    parser.add_argument("--out", type=Path, required=True)
    args = parser.parse_args()
    commit = subprocess.check_output(["git", "-C", str(args.repo), "rev-parse", "HEAD"], text=True).strip()
    args.out.parent.mkdir(parents=True, exist_ok=True)
    proc = subprocess.Popen(["git", "-C", str(args.repo), "archive", "--format=tar", commit,
                             *BUILD_PATHS], stdout=subprocess.PIPE)
    skipped = 0
    with tarfile.open(fileobj=proc.stdout, mode="r|") as source, tarfile.open(args.out, "w:gz") as output:
        for member in source:
            if private_path(member.name):
                skipped += 1
                continue
            output.addfile(member, source.extractfile(member) if member.isfile() else None)
    if proc.wait() != 0:
        raise RuntimeError("git archive failed")
    print(f"commit={commit} archive_bytes={args.out.stat().st_size} private_paths_skipped={skipped}")


if __name__ == "__main__":
    main()
