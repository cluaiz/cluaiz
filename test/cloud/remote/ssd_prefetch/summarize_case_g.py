"""Tables of run_case_g.sh's raw files: one row per run, then the median (range) of the rounds.
Usage: python3 summarize_case_g.py <out_dir>   (prints Markdown)"""
import csv, glob, os, re, statistics, sys

out = sys.argv[1]
ANSI = re.compile(r"\x1b\[[0-9;]*m")


def bench_speeds(path):
    """pp/tg t/s from a llama-bench or `cluaiz bench` Markdown table"""
    speeds = {}
    for line in open(path, errors="replace"):
        cells = [c.strip() for c in line.strip().strip("|").split("|")]
        if len(cells) >= 2 and re.fullmatch(r"(pp|tg)\d+", cells[-2]) and "±" in cells[-1]:
            speeds[cells[-2]] = float(cells[-1].split("±")[0])
    return speeds


def text_speeds(tag):
    """per prompt (prompt t/s, generation t/s, generated tokens)"""
    res = {}
    log = open(f"{out}/{tag}.log", errors="replace").read()
    if "L-text" in tag:
        for m in re.finditer(r"\| (\w+) \| prompt (\d+) tok ([\d.]+) t/s \| gen (\d+) tok ([\d.]+) t/s \|", log):
            res[m.group(1)] = (float(m.group(3)), float(m.group(5)), int(m.group(4)))
        return res
    log = ANSI.sub("", log)
    for block in re.finditer(r"### prompt (\w+)(.*?)### end", log, re.S):
        p = re.search(r"prompt eval\s+(\d+) tokens\s+[\d.]+ s\s+([\d.]+) tokens/s", block.group(2))
        g = re.search(r"\neval\s+(\d+) tokens\s+[\d.]+ s\s+([\d.]+) tokens/s", block.group(2))
        if p and g:
            res[block.group(1)] = (float(p.group(2)), float(g.group(2)), int(g.group(1)))
    return res


def resources(tag):
    r = {}
    tf = f"{out}/{tag}.time"
    if os.path.exists(tf):
        t = open(tf).read()
        m = re.search(r"Maximum resident set size \(kbytes\): (\d+)", t)
        r["rss"] = int(m.group(1)) // 1024 if m else ""
        m = re.search(r"Major \(requiring I/O\) page faults: (\d+)", t)
        r["majflt"] = int(m.group(1)) if m else ""
        m = re.search(r"Exit status: (\d+)", t)
        r["exit"] = m.group(1) if m else "?"
    df = f"{out}/{tag}.delta"
    if os.path.exists(df):
        m = re.search(r"read (-?\d+) MiB from disk, (\d+) s", open(df).read())
        if m:
            r["disk"], r["secs"] = int(m.group(1)), int(m.group(2))
    sf = f"{out}/{tag}.samples.csv"
    if os.path.exists(sf):
        s = [x for x in csv.DictReader(open(sf)) if x["vram_used_mib"] and x["cgroup_mib"]]
        if s:
            r["vram"] = max(int(x["vram_used_mib"]) for x in s)
            r["cgroup"] = max(int(x["cgroup_mib"]) for x in s)
            r["avail"] = min(int(x["mem_avail_mib"]) for x in s)
    return r


def med(vals):
    vals = [v for v in vals if v != ""]
    return f"{statistics.median(vals):.2f} ({min(vals):.2f}-{max(vals):.2f})" if vals else ""


tags = sorted(os.path.basename(p)[:-5] for p in glob.glob(f"{out}/r*_*.done"))
bench = {}
text = {}
for tag in tags:
    rnd, cfg = tag.split("_", 1)
    res = resources(tag)
    if cfg.endswith("-text"):
        text.setdefault(cfg, []).append((rnd, text_speeds(tag), res))
    else:
        md = f"{out}/{tag}.md"
        bench.setdefault(cfg, []).append((rnd, bench_speeds(md) if os.path.exists(md) else {}, res))

tests = ["pp512", "tg128", "tg400"]
cols = "| peak RSS MiB | peak cgroup MiB | peak VRAM MiB (hog included) | disk MiB read | major faults | s | exit |"
print("## bench runs (-t 4 -p 512 -n 128,400 -r 1)\n")
print("| Config | Round | " + " | ".join(tests) + " " + cols)
print("|---|---|" + "---|" * (len(tests) + 7))
for cfg, runs in bench.items():
    for rnd, sp, r in runs:
        print(f"| {cfg} | {rnd} | " + " | ".join(str(sp.get(t, "")) for t in tests)
              + f" | {r.get('rss', '')} | {r.get('cgroup', '')} | {r.get('vram', '')} | {r.get('disk', '')} | {r.get('majflt', '')} | {r.get('secs', '')} | {r.get('exit', '')} |")
print("\nMedian (range) of the rounds:\n")
print("| Config | " + " | ".join(tests) + " | disk MiB read | major faults |\n|---|" + "---|" * (len(tests) + 2))
for cfg, runs in bench.items():
    print(f"| {cfg} | " + " | ".join(med([sp[t] for _, sp, _ in runs if t in sp]) for t in tests)
          + f" | {med([r.get('disk', '') for *_, r in runs])} | {med([r.get('majflt', '') for *_, r in runs])} |")

prompts = ["code", "chat", "list", "long"]
print("\n## case G prompts (greedy, thinking off, MTP off): generation t/s, prompt t/s in brackets\n")
print("| Config | Round | " + " | ".join(prompts) + " " + cols)
print("|---|---|" + "---|" * (len(prompts) + 7))
for cfg, runs in text.items():
    for rnd, sp, r in runs:
        cells = [f"{sp[p][1]:.2f} ({sp[p][0]:.1f}; {sp[p][2]} tok)" if p in sp else "" for p in prompts]
        print(f"| {cfg} | {rnd} | " + " | ".join(cells)
              + f" | {r.get('rss', '')} | {r.get('cgroup', '')} | {r.get('vram', '')} | {r.get('disk', '')} | {r.get('majflt', '')} | {r.get('secs', '')} | {r.get('exit', '')} |")
print("\nMedian (range) of the rounds, generation t/s:\n")
print("| Config | " + " | ".join(prompts) + " | prompt t/s (code) | disk MiB read |\n|---|" + "---|" * (len(prompts) + 2))
for cfg, runs in text.items():
    print(f"| {cfg} | " + " | ".join(med([sp[p][1] for _, sp, _ in runs if p in sp]) for p in prompts)
          + f" | {med([sp['code'][0] for _, sp, _ in runs if 'code' in sp])} | {med([r.get('disk', '') for *_, r in runs])} |")
