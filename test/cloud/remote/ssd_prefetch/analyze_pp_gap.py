"""Per-phase numbers of pp_gap.sh runs: for every run, the prompt t/s of each test, and between the phase
marks of its time-stamped log (llama-bench --progress lines, `cluaiz bench` "running ..." lines) the
seconds, MiB read from disk, the process's major faults, KiB per fault and the cgroup's mean anon / file
memory. Usage: python3 analyze_pp_gap.py <out_dir> > phases.md
"""
import csv
import glob
import os
import re
import sys

out = sys.argv[1]


def samples(path):
    rows = []
    with open(path) as f:
        for r in csv.DictReader(f):
            try:
                rows.append({k: float(v) for k, v in r.items()})
            except ValueError:
                pass
    return rows


def marks(path):
    found = []
    for line in open(path, errors="replace"):
        m = re.match(r"^(\d+\.\d+) (.*)$", line.rstrip("\n"))
        if not m:
            continue
        t, text = float(m.group(1)), m.group(2)
        if "llama-bench: benchmark" in text or text.startswith("running ") or "Model successfully loaded" in text:
            label = re.sub(r"^.*(llama-bench: |✅ \[EngineHost\] )", "", text)[:60]
            found.append((t, label))
    return found


def window(rows, t0, t1):
    inside = [r for r in rows if t0 <= r["t"] <= t1]
    if len(inside) < 2:
        return None
    a, b = inside[0], inside[-1]
    mib = (b["sectors_read"] - a["sectors_read"]) / 2048
    flt = b["proc_majflt"] - a["proc_majflt"]
    anon = sum(r["anon_mib"] for r in inside) / len(inside)
    fil = sum(r["file_mib"] for r in inside) / len(inside)
    return b["t"] - a["t"], mib, flt, (mib * 1024 / flt) if flt > 0 else 0, anon, fil


def tps(md):
    res = {}
    for line in open(md, errors="replace"):
        m = re.search(r"\|\s*(pp\d+|tg\d+)\s*\|\s*([\d.]+) ± ([\d.]+)", line)
        if m:
            res[m.group(1)] = float(m.group(2))
    return res


print("| run | t/s | phase | s | MiB read | proc major faults | KiB/fault | anon MiB | file MiB |")
print("|---|---|---|---|---|---|---|---|---|")
for log in sorted(glob.glob(os.path.join(out, "r*_*.log"))):
    tag = os.path.basename(log)[:-4]
    sp = os.path.join(out, tag + ".samples.csv")
    if not os.path.exists(sp):
        continue
    rows = samples(sp)
    if not rows:
        continue
    ms = marks(log)
    ms.append((rows[-1]["t"], "end"))
    speed = " ".join(f"{k} {v:.2f}" for k, v in tps(os.path.join(out, tag + ".md")).items())
    prev = (rows[0]["t"], "start")
    for t, label in ms:
        w = window(rows, prev[0], t)
        if w:
            print(f"| {tag} | {speed} | {prev[1]} | {w[0]:.0f} | {w[1]:.0f} | {w[2]:.0f} | {w[3]:.0f} | {w[4]:.0f} | {w[5]:.0f} |")
        prev = (t, label)
        speed = ""
