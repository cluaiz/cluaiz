"""Per-phase resource table from run 19's resources.csv: duration, machine CPU and memory, and Cluaiz's own
CPU, anonymous memory and memory-mapped file pages (average and peak), plus website checks that failed.

    python3 analyze.py <out dir>     (writes phases.md)
"""
import csv, sys
from collections import OrderedDict
from pathlib import Path

OUT = Path(sys.argv[1])
phases = OrderedDict()
for row in csv.DictReader(open(OUT / "resources.csv")):
    phases.setdefault(row["phase"] or "-", []).append(row)

lines = ["| Phase | Seconds | Machine CPU avg / peak % | Machine memory peak MB | Cluaiz CPU avg / peak % "
         "| Cluaiz anon avg / peak MB | Cluaiz mapped file peak MB | Website down |",
         "|---|---|---|---|---|---|---|---|"]
for name, rows in phases.items():
    f = lambda k: [float(r[k]) for r in rows]
    avg = lambda v: sum(v) / len(v)
    down = sum(1 for r in rows if r["web"] not in ("200", "")) // 10
    lines.append(f"| {name} | {len(rows)} | {avg(f('cpu_pct')):.0f} / {max(f('cpu_pct')):.0f} | {max(f('mem_used_mb')):.0f} "
                 f"| {avg(f('cluaiz_cpu_pct')):.0f} / {max(f('cluaiz_cpu_pct')):.0f} | {avg(f('cluaiz_anon_mb')):.0f} / "
                 f"{max(f('cluaiz_anon_mb')):.0f} | {max(f('cluaiz_file_mb')):.0f} | {down} |")
(OUT / "phases.md").write_text("\n".join(lines) + "\n")
print("\n".join(lines))
