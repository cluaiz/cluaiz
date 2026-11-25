"""Per request of a media_tools_probe.py set: the calls the model produced in order (from `[NativeTools] Parsed
N call(s): [...]` lines, one per generation), the Engine tool calls actually run (`[API] Tool call:` lines),
tool steps, step / repeat limit, final no-tools turn.

python -B count_media_calls.py <set dir> [<set dir> ...]
"""
import json
import re
import sys
from pathlib import Path

CALL = re.compile(r"\[API\] Tool call: \w+ '([^']+)' \(sub_func: ([^)]*)\)")
PARSED = re.compile(r"\[NativeTools\] Parsed \d+ call\(s\): (\[.*\])")

for d in sys.argv[1:]:
    d = Path(d)
    print(f"## {d.name}")
    print("| i | model calls per generation | calls run | load_tool run | math run | steps | limit hit | final no-tools turn | answer |")
    print("|---|---|---|---|---|---|---|---|---|")
    results = [json.loads(l) for l in open(d / "results.jsonl", encoding="utf-8")]
    logs = [json.loads(l) for l in open(d / "request_logs.jsonl", encoding="utf-8")]
    for r, lg in zip(results, logs):
        calls = []
        for line in lg["lines"]:
            m = CALL.search(line)
            if m:
                name, sub = m.group(1), m.group(2)
                calls.append(name if sub == "None" else f"{name}:{sub.removeprefix('Some(').strip(chr(34)+')')}")
        gens = []
        for line in lg["lines"]:
            m = PARSED.search(line)
            if m:
                gens.append("+".join(json.loads(m.group(1))) or "none")
        loads = sum(c == "load_tool" for c in calls)
        maths = sum(c.startswith("math") for c in calls)
        ans = (r.get("content") or "").replace("\n", " ").replace("|", "/")[:60]
        print(f"| {r['i']} | {' / '.join(gens) or '-'} | {', '.join(calls) or '-'} | {loads} | {maths} | {r['steps']} | "
              f"{'yes' if r['step_limit'] else 'no'} | {'yes' if r['final_no_tools_turn'] else 'no'} | {ans} |")
    print()
