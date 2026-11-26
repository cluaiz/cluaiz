"""Print the one plain result table of vps_small_llm.sh and write it to <out-dir>/table.txt.

Reads what the vps26 scripts wrote into the results folder: resources.csv (sampler.py, one row per
second), steps_native_<engine>.jsonl and requests_native_<engine>.jsonl (workloads.py), cpu_stat.log
(marks written by vps_small_llm.sh) and machine.json. Percentiles come from vps26/analyze.py (pct), so
the numbers use the same method as run 26.

Usage: python3 table.py --out-dir <results folder>
"""
import argparse
import csv
import json
import re
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent.parent / "vps26"))
from analyze import pct  # noqa: E402  (same percentile method as run 26)

ENGINE_NAMES = {"cluaiz": "Cluaiz", "llama": "llama.cpp", "ollama": "Ollama"}
PHASE_RE = re.compile(r"^native_([a-z]+)_W1_.*_c(\d+)$")


def load_jsonl(path):
    rows = []
    if path.exists():
        for line in path.read_text().splitlines():
            if line.strip():
                rows.append(json.loads(line))
    return rows


def fnum(v, nd=1):
    return "-" if v is None else f"{v:.{nd}f}"


def steal_pct(out_dir, engine):
    """CPU steal over one engine block, from the /proc/stat lines marked before and after it."""
    marks = {}
    p = out_dir / "cpu_stat.log"
    if p.exists():
        for line in p.read_text().splitlines():
            parts = line.split()
            if len(parts) > 9 and parts[1] == "cpu":
                marks[parts[0]] = [int(x) for x in parts[2:]]
    a, b = marks.get(f"{engine}_begin"), marks.get(f"{engine}_end")
    if not a or not b or len(a) < 8:
        return None
    total = sum(b) - sum(a)
    return None if total <= 0 else 100.0 * (b[7] - a[7]) / total


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--out-dir", required=True)
    out_dir = Path(ap.parse_args().out_dir)

    machine = json.loads((out_dir / "machine.json").read_text())
    params = machine["params"]
    ql = params["queue_level"]

    phases = {}
    with (out_dir / "resources.csv").open() as f:
        for row in csv.DictReader(f):
            phases.setdefault(row["phase"], []).append(row)

    def col(rows, key):
        return [float(r[key]) for r in rows if r.get(key) not in (None, "")]

    engines = [e for e in ENGINE_NAMES if (out_dir / f"steps_native_{e}.jsonl").exists()]
    cols = {}
    for eng in engines:
        steps = load_jsonl(out_dir / f"steps_native_{eng}.jsonl")
        reqs = load_jsonl(out_dir / f"requests_native_{eng}.jsonl")
        by_level = {s["level"]: s for s in steps}
        rows = []
        for name, prows in phases.items():
            m = PHASE_RE.match(name)
            if m and m.group(1) == eng:
                rows += prows
        ok_reqs = [r for r in reqs if r.get("ok")]
        with_usage = [r for r in ok_reqs if r.get("in_tokens") is not None]
        n_ms = col(rows, "neighbour_ms")
        n_http = [int(float(r["neighbour_http"])) for r in rows]
        seq, que = by_level.get(1), by_level.get(ql)
        cols[eng] = {
            "machine_peak": max(col(rows, "mem_used_mb"), default=None),
            "engine_anon": max(col(rows, "engine_anon_mb"), default=None),
            "engine_file": max(col(rows, "engine_file_mb"), default=None),
            "tps_seq": seq.get("agg_tps") if seq else None,
            "tps_que": que.get("agg_tps") if que else None,
            "ok_seq": f"{seq['ok']} / {seq['total']}" if seq else "not run",
            "ok_que": f"{que['ok']} / {que['total']}" if que else "not run",
            "usage": f"{len(with_usage)} of {len(ok_reqs)}",
            "web": f"{fnum(pct(n_ms, 50))} / {fnum(pct(n_ms, 95))} / {fnum(max(n_ms, default=None))}",
            "web_fail": f"{sum(1 for c in n_http if c != 200)} of {len(n_http)}",
            "oom": max([int(float(r["oom_kills"])) for r in rows] or [0]),
            "steal": steal_pct(out_dir, eng),
        }

    base = phases.get("base", [])
    base_ms = col(base, "neighbour_ms")
    mc = machine["machine"]
    lines = [
        "Cluaiz small-LLM reproduce run (short version of run 19 / run 26)",
        "",
        f"CPU: {mc['cpu_model']}",
        f"vCPU: {mc['vcpus']}   RAM: {mc['mem_total_mb']} MB   Kernel: {mc['kernel']}   OS: {mc['os']}",
        f"Model: {params['model']} (Q4_K_M)   threads {params['threads']}, context {params['ctx']}, "
        f"KV cache {params['kv']}, slots {params['slots']}, temperature 0",
        "Engines: " + "; ".join(f"{k} {v}" for k, v in machine["engines"].items() if k in engines),
        "",
    ]
    labels = [
        ("Machine memory used, peak (MB)", lambda c: fnum(c["machine_peak"], 0)),
        ("Engine memory held, peak (MB)", lambda c: fnum(c["engine_anon"], 0)),
        ("Engine mapped model file, peak (MB)", lambda c: fnum(c["engine_file"], 0)),
        (f"Tokens/s, 1 at a time ({params['seq_secs']} s)", lambda c: fnum(c["tps_seq"], 2)),
        (f"Tokens/s, queue of {ql} ({params['queue_secs']} s)", lambda c: fnum(c["tps_que"], 2)),
        ("Requests OK, 1 at a time", lambda c: c["ok_seq"]),
        (f"Requests OK, queue of {ql}", lambda c: c["ok_que"]),
        ("OK requests with a token count", lambda c: c["usage"]),
        ("Website median / p95 / max (ms)", lambda c: c["web"]),
        ("Website failed checks", lambda c: c["web_fail"]),
        ("OOM kills", lambda c: str(c["oom"])),
        ("CPU steal during the engine block (%)", lambda c: fnum(c["steal"], 1)),
    ]
    w0 = max(len(x[0]) for x in labels) + 2
    wc = 18
    lines.append("".ljust(w0) + "".join(ENGINE_NAMES[e].ljust(wc) for e in engines))
    lines.append("-" * (w0 + wc * len(engines)))
    for label, fn in labels:
        lines.append(label.ljust(w0) + "".join(fn(cols[e]).ljust(wc) for e in engines))
    lines += [
        "",
        f"Website alone, before any engine ({len(base)} s): median / p95 / max (ms) = "
        f"{fnum(pct(base_ms, 50))} / {fnum(pct(base_ms, 95))} / {fnum(max(base_ms, default=None))}; "
        f"machine memory used peak {fnum(max(col(base, 'mem_used_mb'), default=None), 0)} MB",
        "Tokens/s = generated tokens / wall seconds of the step (all requests). If 'OK requests with a "
        "token count' is lower than the OK count, that engine did not report usage and the rate counts "
        "streamed chunks, not tokens.",
        "A CPU steal above a few percent means the host took CPU back: do not compare engines then.",
    ]
    text = "\n".join(lines) + "\n"
    (out_dir / "table.txt").write_text(text)
    print(text)


if __name__ == "__main__":
    main()
