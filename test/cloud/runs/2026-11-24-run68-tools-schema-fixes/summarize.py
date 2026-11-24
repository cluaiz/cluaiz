"""Summary table from fixcheck results: python -B summarize.py <out-dir> [<out-dir> ...]

Recomputes `constrained` from each request's kept log lines with fixcheck.constrained_in, so every
run is counted with the same rule."""
import json
import sys
from collections import OrderedDict
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from fixcheck import constrained_in  # noqa: E402

for folder in sys.argv[1:]:
    rows = OrderedDict()
    for line in (Path(folder) / "results.jsonl").open(encoding="utf-8"):
        r = json.loads(line)
        row = rows.setdefault(r["case"], {"n": 0, "valid": 0, "constrained": 0, "status": set(), "prompt_n": set()})
        row["n"] += 1
        row["valid"] += bool(r["valid"])
        row["constrained"] += constrained_in(r.get("log") or [])
        row["status"].add(r.get("status"))
        if r.get("prompt_n") not in (None, 1, 4):
            row["prompt_n"].add(r.get("prompt_n"))
    print(f"\n{folder}\n| Case | Valid | Constrained (log) | HTTP | prompt_n |\n|---|---:|---:|---|---|")
    for case, row in rows.items():
        print(f"| {case} | {row['valid']} / {row['n']} | {row['constrained']} / {row['n']} | {sorted(row['status'])} | {sorted(row['prompt_n'])} |")
