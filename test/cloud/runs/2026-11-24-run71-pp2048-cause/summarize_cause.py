"""pp2048 cause run tables: per round and config the pp512 / pp2048 mean of the tool's own table (its
repetitions), the run totals from <tag>.delta, then per config the median and range over the rounds and the
ratio to LF in the same round. MiB read in the pp2048 phase comes from phases.md (analyze_pp_gap.py).
Usage: python summarize_cause.py <results dir> > tables.md
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
    if tag == "phases" or "_" not in tag:
        continue
    rnd, cfg = tag.split("_", 1)
    if not rnd[1:].isdigit():
        continue
    speeds = {}
    for line in open(md, errors="replace"):
        m = re.search(r"\|\s*(pp\d+)\s*\|\s*([\d.]+) ± ([\d.]+)", line)
        if m:
            speeds[m.group(1)] = (float(m.group(2)), float(m.group(3)))
    dp = os.path.join(out, tag + ".delta")
    delta = open(dp).read().strip() if os.path.exists(dp) else ""
    rows[(int(rnd[1:]), cfg)] = (speeds, delta)

# pp2048 phase MiB read per run from phases.md: Cluaiz "running pp2048" row; llama-bench rows of benchmark 2/2
pp2048_mib = {}
pp = os.path.join(out, "phases.md")
if os.path.exists(pp):
    cur = None
    for line in open(pp, errors="replace"):
        cells = [c.strip() for c in line.strip().strip("|").split("|")]
        if len(cells) < 5 or not cells[0].startswith("r"):
            continue
        cur = cells[0]
        phase, mib = cells[2], cells[4]
        if not mib.isdigit():
            continue
        if "pp2048" in phase or "benchmark 2/2" in phase:
            pp2048_mib[cur] = pp2048_mib.get(cur, 0) + int(mib)

cfgs = sorted({c for _, c in rows}, key=lambda c: (c != "LF", c != "C", c))
rounds = sorted({r for r, _ in rows})

print("| round | config | pp512 | pp2048 | pp2048 phase MiB read | run totals |")
print("|---|---|---|---|---|---|")
for r in rounds:
    for c in cfgs:
        if (r, c) not in rows:
            continue
        s, d = rows[(r, c)]
        cell = lambda k: f"{s[k][0]:.2f} ± {s[k][1]:.2f}" if k in s else "-"
        print(f"| {r} | {c} | {cell('pp512')} | {cell('pp2048')} | {pp2048_mib.get(f'r{r}_{c}', '-')} | {d} |")

print()
print("| config | pp512 median (range) | pp2048 median (range) | pp512 / LF per round | pp2048 / LF per round | pp2048 / LF median | pp2048 MiB read median |")
print("|---|---|---|---|---|---|---|")
for c in cfgs:
    line = [c]
    for test in ("pp512", "pp2048"):
        v = [rows[(r, c)][0][test][0] for r in rounds if (r, c) in rows and test in rows[(r, c)][0]]
        line.append(f"{statistics.median(v):.2f} ({min(v):.2f}-{max(v):.2f})" if v else "-")
    ratios = {}
    for test in ("pp512", "pp2048"):
        ratios[test] = [rows[(r, c)][0][test][0] / rows[(r, "LF")][0][test][0] for r in rounds
                        if (r, c) in rows and (r, "LF") in rows and test in rows[(r, c)][0] and test in rows[(r, "LF")][0]]
        line.append(", ".join(f"{x:.3f}" for x in ratios[test]) or "-")
    line.append(f"{statistics.median(ratios['pp2048']):.3f}" if ratios["pp2048"] else "-")
    mib = [pp2048_mib[f"r{r}_{c}"] for r in rounds if f"r{r}_{c}" in pp2048_mib]
    line.append(f"{statistics.median(mib):.0f}" if mib else "-")
    print("| " + " | ".join(line) + " |")
