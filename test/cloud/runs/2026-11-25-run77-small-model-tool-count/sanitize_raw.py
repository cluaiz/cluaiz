"""Redact VM identifiers from raw JSONL/log files before copying them into git.

Run on the VM: python3 sanitize_raw.py <raw-directory>. It edits only files under
that directory. The operator should scan the output again before committing.
"""
import re
import sys
from pathlib import Path

root = Path(sys.argv[1]).resolve(strict=True)
assert root.name == "raw" and root.parent.name == "run77", root
patterns = [
    (re.compile(r"/home/[^/\\\s\"']+"), "/home/<vps>"),
    (re.compile(r"(?<![\w.])(?:\d{1,3}\.){3}\d{1,3}(?![\w.])"), "<vps>"),
    (re.compile(r"\b(?:[\w-]+\.)?internal\b", re.I), "<vps>"),
]
for path in root.rglob("*"):
    if not path.is_file() or path.suffix not in (".json", ".jsonl", ".log"):
        continue
    before = path.read_text(encoding="utf-8")
    after = before
    for pattern, replacement in patterns:
        after = pattern.sub(replacement, after)
    if after != before:
        path.write_text(after, encoding="utf-8")
    print(path.relative_to(root), len(before), before != after)
