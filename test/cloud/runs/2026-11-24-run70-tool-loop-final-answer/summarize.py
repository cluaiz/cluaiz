"""Tables for run 70 from raw/<label>/results.jsonl (python -B summarize.py > raw/tables.md)"""
import json
import statistics
from pathlib import Path

RAW = Path(__file__).resolve().parent / "raw"
LABELS = ["before_marker", "before2_marker", "after_marker", "before_maxsteps2", "after_maxsteps2",
          "before_stream", "after_stream", "before_client_tools", "after_client_tools"]
print("| Set | n | Real answer | Marker in answer | Ended at step/repeat limit | Final no-tools turn | Engine tool steps (sum / max) | Median s | Total s |")
print("|---|---|---|---|---|---|---|---|---|")
for label in LABELS:
    rows = [json.loads(l) for l in open(RAW / label / "results.jsonl", encoding="utf-8")]
    secs = [r["seconds"] for r in rows]
    print(f"| {label} | {len(rows)} | {sum(r['real_answer'] for r in rows)} | {sum(r['has_expected'] for r in rows)} | "
          f"{sum(r['step_limit'] for r in rows)} | {sum(r['final_no_tools_turn'] for r in rows)} | "
          f"{sum(r['steps'] for r in rows)} / {max(r['steps'] for r in rows)} | {statistics.median(secs):.1f} | {sum(secs):.1f} |")
print()
print("Per request, before2 vs after (same binary order, same prompts):")
print()
print("| i | before2 steps | before2 limit | before2 s | after steps | after limit | final turn | after s | after answer (first 60 chars) |")
print("|---|---|---|---|---|---|---|---|---|")
b = {r["i"]: r for r in map(json.loads, open(RAW / "before2_marker" / "results.jsonl", encoding="utf-8"))}
for r in map(json.loads, open(RAW / "after_marker" / "results.jsonl", encoding="utf-8")):
    x = b[r["i"]]
    text = r["content"][:60].replace("\n", " ").replace("|", "/")
    print(f"| {r['i']} | {x['steps']} | {x['step_limit']} | {x['seconds']} | {r['steps']} | {r['step_limit']} | {r['final_no_tools_turn']} | {r['seconds']} | {text} |")
