"""Per case and binary: requests, judge-valid, tool calls (by name), schema-valid JSON content, constrained log.

python -B count_calls.py <results.jsonl> [<results.jsonl> ...]
"""
import json
import sys
from collections import defaultdict


def schema_json(content):
    try:
        value = json.loads(content or "")
    except ValueError:
        return False
    return isinstance(value, dict) and isinstance(value.get("name"), str) and isinstance(value.get("age"), int)


for path in sys.argv[1:]:
    rows = defaultdict(list)
    for line in open(path, encoding="utf-8"):
        row = json.loads(line)
        rows[row["case"]].append(row)
    print(f"## {path}")
    print("| case | n | valid | calls (names) | schema JSON content | constrained log |")
    print("|---|---|---|---|---|---|")
    for case, items in rows.items():
        names = defaultdict(int)
        for r in items:
            for call in r.get("tool_calls") or []:
                names[(call.get("function") or {}).get("name")] += 1
        calls = sum(bool(r.get("tool_calls")) for r in items)
        print(f"| {case} | {len(items)} | {sum(bool(r.get('valid')) for r in items)} | {calls} {dict(names)} | "
              f"{sum(schema_json(r.get('content')) for r in items)} | {sum(bool(r.get('constrained_log')) for r in items)} |")
