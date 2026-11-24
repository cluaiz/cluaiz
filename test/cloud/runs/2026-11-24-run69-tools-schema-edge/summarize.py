"""Summary table from edgecheck results: python -B summarize.py <out-dir> [<out-dir> ...]

Recomputes `constrained` from each request's kept log lines with edgecheck.constrained_in, so every folder is
counted with the same rule; `reasoning` = requests with a non-empty reasoning_content; `finish` = finish_reason counts."""
import json
import sys
from collections import Counter, OrderedDict
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from edgecheck import constrained_in  # noqa: E402

for folder in sys.argv[1:]:
    rows = OrderedDict()
    for line in (Path(folder) / "results.jsonl").open(encoding="utf-8"):
        r = json.loads(line)
        row = rows.setdefault(r["case"], {"n": 0, "valid": 0, "constrained": 0, "reasoning": 0, "status": set(),
                                          "finish": Counter(), "prompt": set()})
        row["n"] += 1
        row["valid"] += bool(r["valid"])
        row["constrained"] += constrained_in(r.get("log") or [])
        row["reasoning"] += (r.get("reasoning_len") or 0) > 0
        row["status"].add(r.get("status"))
        row["finish"][str(r.get("finish_reason"))] += 1
        row["prompt"].update(int(x) for x in r.get("prompt_tokens_log") or [] if int(x) > 4)
    print(f"\n{folder}\n| Case | Valid | Constrained (log) | Reasoning non-empty | finish_reason | HTTP | prompt tokens (log, >4) |\n|---|---:|---:|---:|---|---|---|")
    for case, row in rows.items():
        finish = ", ".join(f"{k} {v}" for k, v in sorted(row["finish"].items()))
        print(f"| {case} | {row['valid']} / {row['n']} | {row['constrained']} / {row['n']} | {row['reasoning']} / {row['n']} | {finish} | "
              f"{sorted(row['status'])} | {sorted(row['prompt'])} |")
