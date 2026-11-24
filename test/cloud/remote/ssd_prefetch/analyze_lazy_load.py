"""Per-run table for run_prefetch_laptop.ps1 -Set lazy: load seconds and disk bytes read during the load.

Load window: the Engine's "MEMORY TRACE: 1. BEFORE MODEL LOAD" and "2. AFTER MODEL LOAD" debug lines (-vv,
UTC) in <tag>.log. Disk bytes: the typeperf samples (one per second, local time, offset from the CSV header)
inside that window, summed (bytes/s x 1 s). llama-bench prints no load time, so L rows have only the totals.

Usage: python analyze_lazy_load.py <results folder>   (writes lazy_load.md there and prints it)
"""
import csv
import datetime as dt
import re
import statistics
import sys
from pathlib import Path

out = Path(sys.argv[1])
TS = re.compile(r"^(\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}\.\d+)Z")


def utc(line):
    m = TS.match(line)
    return dt.datetime.fromisoformat(m.group(1)[:26]) if m else None


def perf(tag):
    p = out / f"{tag}.perf.csv"
    if not p.exists() or p.stat().st_size == 0:
        return []
    rows = list(csv.reader(p.open(encoding="utf-8", errors="replace")))
    off = int(re.search(r"\((-?\d+)\)\"?$", rows[0][0]).group(1)) if re.search(r"\((-?\d+)\)", rows[0][0]) else 0
    head = rows[0]
    i_mem = next(i for i, h in enumerate(head) if h.endswith("Available MBytes"))
    i_rd = next(i for i, h in enumerate(head) if h.endswith("Disk Read Bytes/sec"))
    res = []
    for r in rows[1:]:
        try:
            t = dt.datetime.strptime(r[0], "%m/%d/%Y %H:%M:%S.%f") + dt.timedelta(minutes=off)
        except ValueError:
            continue
        num = lambda x: float(x) if x.strip() else None
        res.append((t, num(r[i_mem]), num(r[i_rd])))
    return res


def run(tag):
    row = {"tag": tag}
    md = out / f"{tag}.md"
    if md.exists():
        for line in md.read_text(encoding="utf-8", errors="replace").splitlines():
            c = [x.strip() for x in line.strip().strip("|").split("|")]
            if len(c) >= 2 and re.match(r"^(pp|tg)\d+$", c[-2]):
                m = re.match(r"^([\d.]+)", c[-1])
                if m:
                    row[c[-2][:2]] = float(m.group(1))
    info = out / f"{tag}.info.txt"
    if info.exists():
        t = info.read_text(encoding="utf-8", errors="replace")
        for k, pat in (("free_before", r"free RAM MiB before: (\d+)"), ("secs", r"seconds: ([\d.,]+)"), ("exit", r"exit code: (\S+)")):
            m = re.search(pat, t)
            if m:
                row[k] = m.group(1).replace(",", "")
    before = after = None
    log = out / f"{tag}.log"
    text = log.read_text(encoding="utf-8", errors="replace") if log.exists() else ""
    for line in text.splitlines():
        if "1. BEFORE MODEL LOAD" in line and before is None:
            before = utc(line)
        elif "2. AFTER MODEL LOAD" in line and after is None:
            after = utc(line)
    m = re.search(r"n_gpu_layers = (-?\d+) \| n_cpu_moe = (\d+)", text)
    row["placement"] = f"ngl {m.group(1)} ncmoe {m.group(2)}" if m else ""
    m = re.search(r"MoE cache size = +([\d.]+) MiB", text)
    row["cache"] = m.group(1) if m else ""
    m = re.search(r"(\d+) MiB of CPU-mapped weights left out", text)
    row["lazy_mib"] = m.group(1) if m else ""
    m = re.findall(r"moe_prefetch: off: (\d+) calls, \d+ ranges, (\d+) MiB prefetched(?:, (\d+) MiB skipped)?", text)
    row["pf"] = f"{m[-1][0]} calls, {m[-1][1]} MiB asked, {m[-1][2] or '?'} MiB skipped" if m else ""
    samples = perf(tag)
    if samples:
        rd = [s[2] for s in samples if s[2] is not None]
        mem = [s[1] for s in samples if s[1] is not None]
        row["read_gb"] = sum(rd) / 1e9
        row["min_free"] = min(mem) if mem else None
        if before and after:
            row["load_s"] = (after - before).total_seconds()
            lo, hi = before - dt.timedelta(seconds=1), after + dt.timedelta(seconds=1)
            row["load_read_gb"] = sum(s[2] for s in samples if s[2] is not None and lo <= s[0] <= hi) / 1e9
    return row


tags = sorted({p.name[: -len(".info.txt")] for p in out.glob("r*_*.info.txt")}, key=lambda t: (int(t[1:t.index("_")]), t))
rows = [run(t) for t in tags]
f = lambda v, d=2: "" if v is None or v == "" else (f"{v:.{d}f}" if isinstance(v, float) else str(v))
lines = ["| run | free RAM before MiB | min free MiB | wall s | load s | disk GB in load | disk GB total | pp512 | tg128 | placement | cache MiB | lazy MiB | prefetch |",
         "|---|---|---|---|---|---|---|---|---|---|---|---|---|"]
for r in rows:
    lines.append(f"| {r['tag']} | {r.get('free_before','')} | {f(r.get('min_free'),0)} | {r.get('secs','')} | {f(r.get('load_s'),1)} | {f(r.get('load_read_gb'))} | {f(r.get('read_gb'))} | {f(r.get('pp'))} | {f(r.get('tg'))} | {r['placement']} | {r['cache']} | {r['lazy_mib']} | {r['pf']} |")
lines += ["", "Median per config (range):", "", "| config | n | load s | disk GB in load | disk GB total | pp512 | tg128 |", "|---|---|---|---|---|---|---|"]
cfgs = []
for r in rows:
    c = r["tag"].split("_", 1)[1]
    if c not in cfgs:
        cfgs.append(c)
for c in cfgs:
    rs = [r for r in rows if r["tag"].split("_", 1)[1] == c]
    def cell(k, d=2):
        v = [r[k] for r in rs if isinstance(r.get(k), float)]
        return f"{statistics.median(v):.{d}f} ({min(v):.{d}f}-{max(v):.{d}f})" if v else ""
    lines.append(f"| {c} | {len(rs)} | {cell('load_s',1)} | {cell('load_read_gb')} | {cell('read_gb')} | {cell('pp')} | {cell('tg')} |")
text = "\n".join(lines) + "\n"
(out / "lazy_load.md").write_text(text, encoding="utf-8")
print(text)
