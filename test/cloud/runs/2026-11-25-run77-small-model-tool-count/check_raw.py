"""Validate sanitized raw JSONL and print compact scores."""
import json
import statistics
import sys
from pathlib import Path

for path in sorted(Path(sys.argv[1]).glob("*/raw.jsonl")):
    rows = []
    for i, line in enumerate(path.read_text(encoding="utf-8").splitlines(), 1):
        try:
            rows.append(json.loads(line))
        except json.JSONDecodeError as exc:
            print(path.parent.name, "invalid JSON line", i, exc)
            break
    else:
        print(path.parent.name, len(rows),
              sum(r["verdict"] == "correct" and not r["wrong_no_tool"] for r in rows),
              sum(r["wrong_no_tool"] for r in rows),
              statistics.median(r["seconds"] for r in rows) if rows else "?")
