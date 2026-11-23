"""Tables of run 48's raw files (run_case_g.sh serve and bench configurations): one row per run, then
the median (range) of the rounds. Usage: python3 summarize_fair.py <out_dir>   (prints Markdown)"""
import glob, json, os, re, statistics, sys

out = sys.argv[1]
ANSI = re.compile(r"\x1b\[[0-9;]*m")
NAMES = ["code", "chat", "list", "long", "pp512", "pp2048", "pp4096"]


def serve_speeds(tag):
    """{prompt: (prompt tokens, prompt t/s, generated tokens, generation t/s)} from one serve run"""
    res = {}
    if "C-" in tag:
        # cluaiz serve: the "llama.cpp timings" lines of its own -vv log, in request order
        log = ANSI.sub("", open(f"{out}/{tag}.server.log", errors="replace").read())
        rows = re.findall(r"llama\.cpp timings: prompt (\d+) tokens in [\d.]+ ms \(([\d.]+) t/s\) \| "
                          r"generation (\d+) tokens in [\d.]+ ms \(([\d.]+) t/s\)", log)
        names = [json.loads(l)["prompt"] for l in open(f"{out}/{tag}.md") if l.startswith("{")]
        for name, r in zip(names, rows):
            res[name] = (int(r[0]), float(r[1]), int(r[2]), float(r[3]))
        return res
    for line in open(f"{out}/{tag}.md", errors="replace"):
        if line.startswith("{"):
            j = json.loads(line)
            t = j["timings"]
            res[j["prompt"]] = (t.get("prompt_n"), t.get("prompt_per_second"), t.get("predicted_n"),
                                t.get("predicted_per_second"))
    return res


def bench_speeds(tag):
    speeds = {}
    for line in open(f"{out}/{tag}.md", errors="replace"):
        cells = [c.strip() for c in line.strip().strip("|").split("|")]
        if len(cells) >= 2 and re.fullmatch(r"(pp|tg)\d+", cells[-2]) and "±" in cells[-1]:
            speeds[cells[-2]] = float(cells[-1].split("±")[0])
    return speeds


def cli_speed(tag):
    text = ANSI.sub("", open(f"{out}/{tag}.md", errors="replace").read() + open(f"{out}/{tag}.log", errors="replace").read())
    m = re.search(r"generation:.*?([\d.]+) tok/s", text)
    return {"tg128": float(m.group(1))} if m else {}


def delta(tag):
    p = f"{out}/{tag}.delta"
    return open(p).read().strip() if os.path.exists(p) else ""


def med(vals):
    vals = [v for v in vals if v is not None]
    if not vals:
        return ""
    return f"{statistics.median(vals):.2f} ({min(vals):.2f}-{max(vals):.2f})"


# A run that wrote no answers (a server that never loaded the model) has no .md and is left out
tags = sorted({os.path.basename(p)[:-5] for p in glob.glob(f"{out}/r*_*.done") if os.path.exists(p[:-5] + ".md")})
cfgs = {}
for t in tags:
    cfgs.setdefault(t.split("_", 1)[1], []).append(t)

serve = {c: ts for c, ts in cfgs.items() if "serve" in c}
bench = {c: ts for c, ts in cfgs.items() if "serve" not in c}

if serve:
    print("## Serve runs (one load per run): generation t/s of the four prompts, prompt t/s of the long prompts\n")
    print("| Run | " + " | ".join(NAMES) + " | prompt tokens (pp rows) | disk / time |")
    print("|---|" + "---|" * (len(NAMES) + 2))
    data = {}
    for c, ts in serve.items():
        for t in ts:
            s = serve_speeds(t)
            data[t] = s
            cells = [f"{s[n][3]:.2f}" if n in s and n[:2] != "pp" else (f"{s[n][1]:.2f}" if n in s else "") for n in NAMES]
            toks = ", ".join(str(s[n][0]) for n in NAMES[4:] if n in s)
            print(f"| {t} | " + " | ".join(cells) + f" | {toks} | {delta(t)} |")
    print("\n### Median (range) of the rounds\n")
    print("| Config | " + " | ".join(NAMES) + " |")
    print("|---|" + "---|" * len(NAMES))
    for c, ts in serve.items():
        row = []
        for n in NAMES:
            k = 1 if n.startswith("pp") else 3
            row.append(med([data[t][n][k] for t in ts if n in data[t]]))
        print(f"| {c} | " + " | ".join(row) + " |")

if bench:
    keys = ["pp512", "pp2048", "pp4096", "tg128"]
    print("\n## Bench runs (one load per run): t/s\n")
    print("| Run | " + " | ".join(keys) + " | disk / time |")
    print("|---|" + "---|" * (len(keys) + 1))
    data = {}
    for c, ts in bench.items():
        for t in ts:
            s = cli_speed(t) if c == "B-cli" else bench_speeds(t)
            data[t] = s
            print(f"| {t} | " + " | ".join(f"{s[k]:.2f}" if k in s else "" for k in keys) + f" | {delta(t)} |")
    print("\n### Median (range) of the rounds\n")
    print("| Config | " + " | ".join(keys) + " |")
    print("|---|" + "---|" * len(keys))
    for c, ts in bench.items():
        print(f"| {c} | " + " | ".join(med([data[t].get(k) for t in ts]) for k in keys) + " |")
