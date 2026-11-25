"""Recount Run77 from raw replies. Missing/incomplete passes remain unknown."""
import collections
import json
import statistics
from pathlib import Path

ROOT = Path(__file__).resolve().parent


def repeated_calls(row):
    calls = [line.split("[API] Tool call:", 1)[1].strip()
             for line in row["log_lines"] if "[API] Tool call:" in line]
    counts = collections.Counter(calls)
    return max(counts.values(), default=0), sum(n - 1 for n in counts.values() if n > 1)


def score(path):
    try:
        rows = [json.loads(line) for line in (path / "raw.jsonl").read_text(encoding="utf-8").splitlines()]
    except FileNotFoundError:
        return None
    if len(rows) != 40 or [r["i"] for r in rows] != list(range(40)):
        return None
    good = lambda r: r["verdict"] == "correct" and not r["wrong_no_tool"] and r["status"] == 200
    math = path / "math.json"
    math_row = json.loads(math.read_text(encoding="utf-8")) if math.exists() else None
    return {
        "correct": sum(map(good, rows)),
        "obvious": sum(good(r) for r in rows if r["kind"] == "call"),
        "indirect": sum(good(r) for r in rows if r["kind"] == "pick"),
        "no_tool": sum(good(r) for r in rows if r["kind"] == "none"),
        "wrong_no_tool": sum(r["wrong_no_tool"] for r in rows if r["kind"] == "none"),
        "median_s": statistics.median(r["seconds"] for r in rows),
        "steps_total": sum(r["steps"] for r in rows),
        "steps_max": max(r["steps"] for r in rows),
        "loop_prompts": sum(repeated_calls(r)[0] >= 2 for r in rows),
        "repeated_calls": sum(repeated_calls(r)[1] for r in rows),
        "repeat_limit_prompts": sum(r["repeat_limit"] for r in rows),
        "http_errors": sum(r["status"] != 200 for r in rows),
        "leaked": sum(r["leak"] for r in rows),
        "math": {"seconds": math_row["seconds"], "steps": math_row["steps"],
                 "engine_calls": math_row["engine_calls"],
                 "content": (math_row["reply"].get("choices") or [{}])[0].get("message", {}).get("content", "")}
                 if math_row else None,
    }


result = {p.name: score(p) for p in ROOT.iterdir() if p.is_dir() and (p / "raw.jsonl").exists()}
result = {k: v for k, v in sorted(result.items()) if v is not None and not k.startswith("pilot-")}
(ROOT / "metrics.json").write_text(json.dumps(result, indent=2) + "\n", encoding="utf-8")
for name, m in result.items():
    print(name, f"{m['correct']}/40", f"{m['obvious']}/24", f"{m['indirect']}/8",
          f"{m['no_tool']}/8", f"wrong-none={m['wrong_no_tool']}",
          f"loops={m['loop_prompts']}", f"steps={m['steps_total']}", f"median={m['median_s']}")
print("\n| Model/mode/setup | Correct /40 | Obvious /24 | Indirect /8 | No-tool /8 | Wrong no-tool | Loop prompts | Steps | Median s |")
print("|---|---:|---:|---:|---:|---:|---:|---:|---:|")
for name, m in result.items():
    print(f"| {name} | {m['correct']} | {m['obvious']} | {m['indirect']} | {m['no_tool']} | "
          f"{m['wrong_no_tool']} | {m['loop_prompts']} | {m['steps_total']} | {m['median_s']:.2f} |")
