"""Run 63 tables: per round the pp512 / pp2048 mean of each tool's own table (2 repetitions each), the
Cluaiz / llama-bench ratio per round, then median and range over the rounds; plus disk read and faults
from <tag>.delta. Usage: python summarize_run63.py results > tables.md
"""
import glob
import os
import re
import statistics
import sys

out = sys.argv[1]
rows = {}
for md in sorted(glob.glob(os.path.join(out, "r*_*.md"))):
    tag = os.path.basename(md)[:-3]
    rnd, cfg = tag.split("_", 1)
    speeds = {}
    for line in open(md, errors="replace"):
        m = re.search(r"\|\s*(pp\d+)\s*\|\s*([\d.]+) ± ([\d.]+)", line)
        if m:
            speeds[m.group(1)] = (float(m.group(2)), float(m.group(3)))
    delta = open(os.path.join(out, tag + ".delta")).read().strip() if os.path.exists(os.path.join(out, tag + ".delta")) else ""
    rows[(int(rnd[1:]), cfg)] = (speeds, delta)

print("| round | tool | pp512 | pp2048 | run totals |")
print("|---|---|---|---|---|")
for (r, cfg), (s, d) in sorted(rows.items()):
    cell = lambda k: f"{s[k][0]:.2f} ± {s[k][1]:.2f}" if k in s else "-"
    print(f"| {r} | {cfg} | {cell('pp512')} | {cell('pp2048')} | {d} |")

print()
print("| test | Cluaiz median (range) | llama-bench median (range) | ratio per round | ratio median (range) |")
print("|---|---|---|---|---|")
rounds = sorted({r for r, _ in rows})
for test in ("pp512", "pp2048"):
    c = [rows[(r, "C")][0][test][0] for r in rounds if (r, "C") in rows and test in rows[(r, "C")][0]]
    lf = [rows[(r, "LF")][0][test][0] for r in rounds if (r, "LF") in rows and test in rows[(r, "LF")][0]]
    ratios = [rows[(r, "C")][0][test][0] / rows[(r, "LF")][0][test][0] for r in rounds
              if (r, "C") in rows and (r, "LF") in rows and test in rows[(r, "C")][0] and test in rows[(r, "LF")][0]]
    if not c or not lf:
        continue
    print(f"| {test} | {statistics.median(c):.2f} ({min(c):.2f}-{max(c):.2f}) | {statistics.median(lf):.2f} ({min(lf):.2f}-{max(lf):.2f}) | "
          f"{', '.join(f'{x:.3f}' for x in ratios)} | {statistics.median(ratios):.3f} ({min(ratios):.3f}-{max(ratios):.3f}) |")
