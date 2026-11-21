"""Aggregate vps26 resources.csv, step/request logs, cold starts, and Docker baseline
into analysis.md and summary.json.
"""
import argparse
import csv
import json
import math
from collections import OrderedDict
from pathlib import Path


def pct(vals, p):
    if not vals:
        return None
    s = sorted(vals)
    idx = (len(s) - 1) * (p / 100.0)
    lo = int(math.floor(idx))
    hi = int(math.ceil(idx))
    if lo == hi:
        return round(s[lo], 3)
    return round(s[lo] + (s[hi] - s[lo]) * (idx - lo), 3)


def avg(vals):
    return round(sum(vals) / len(vals), 3) if vals else None


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--out-dir", required=True)
    args = ap.parse_args()
    out_dir = Path(args.out_dir)

    phases = OrderedDict()
    res_csv = out_dir / "resources.csv"
    if res_csv.exists():
        with res_csv.open() as f:
            for row in csv.DictReader(f):
                phases.setdefault(row["phase"] or "-", []).append(row)

    phase_stats = []
    for name, rows in phases.items():
        col = lambda k: [float(r[k]) for r in rows if r.get(k) not in (None, "")]
        n_http = [int(float(r["neighbour_http"])) for r in rows if r.get("neighbour_http") not in (None, "")]
        n_errs = sum(1 for c in n_http if c != 200)
        n_ms = col("neighbour_ms")
        n_db = col("neighbour_db_ms")
        ooms = [int(float(r["oom_kills"])) for r in rows if r.get("oom_kills") not in (None, "")]
        phase_stats.append({
            "phase": name,
            "seconds": len(rows),
            "cpu_avg_pct": avg(col("cpu_pct")),
            "cpu_peak_pct": max(col("cpu_pct")) if col("cpu_pct") else None,
            "mem_used_peak_mb": max(col("mem_used_mb")) if col("mem_used_mb") else None,
            "mem_avail_min_mb": min(col("mem_avail_mb")) if col("mem_avail_mb") else None,
            "swap_peak_mb": max(col("swap_used_mb")) if col("swap_used_mb") else None,
            "disk_read_avg_kb_s": avg(col("disk_read_kb_s")),
            "docker_cpu_avg_pct": avg(col("docker_cpu_pct")),
            "docker_anon_peak_mb": max(col("docker_anon_mb")) if col("docker_anon_mb") else None,
            "docker_file_peak_mb": max(col("docker_file_mb")) if col("docker_file_mb") else None,
            "engine_cpu_avg_pct": avg(col("engine_cpu_pct")),
            "engine_anon_peak_mb": max(col("engine_anon_mb")) if col("engine_anon_mb") else None,
            "engine_file_peak_mb": max(col("engine_file_mb")) if col("engine_file_mb") else None,
            "pg_rss_peak_mb": max(col("pg_rss_mb")) if col("pg_rss_mb") else None,
            "neighbour_checks": len(n_http),
            "neighbour_errors": n_errs,
            "neighbour_p50_ms": pct(n_ms, 50),
            "neighbour_p95_ms": pct(n_ms, 95),
            "neighbour_max_ms": max(n_ms) if n_ms else None,
            "neighbour_db_p95_ms": pct(n_db, 95),
            "oom_kills_max": max(ooms) if ooms else 0,
        })

    steps = []
    for p in sorted(out_dir.glob("steps_*.jsonl")):
        for line in p.read_text().splitlines():
            if line.strip():
                steps.append(json.loads(line))

    cold_starts = []
    cs_file = out_dir / "cold_starts.jsonl"
    if cs_file.exists():
        for line in cs_file.read_text().splitlines():
            if line.strip():
                cold_starts.append(json.loads(line))

    recoveries = []
    for p in sorted(out_dir.glob("recovery_*.jsonl")):
        for line in p.read_text().splitlines():
            if line.strip():
                recoveries.append(json.loads(line))

    # Speech summary per mode/engine from raw requests
    speech_rows = []
    for p in sorted(out_dir.glob("requests_*.jsonl")):
        by_key = OrderedDict()
        for line in p.read_text().splitlines():
            if not line.strip():
                continue
            r = json.loads(line)
            if r.get("workload") in ("W5", "W6") and r.get("phase_kind") in ("tts", "stt_moonshine-tiny", "stt_moonshine-base"):
                key = (r["mode"], r["engine"], r["workload"], r["phase_kind"])
                by_key.setdefault(key, []).append(r)
        for (mode, eng, wl, pk), rs in by_key.items():
            ok_rs = [x for x in rs if x.get("ok")]
            rtfs = [x["rtf"] for x in ok_rs if x.get("rtf") is not None]
            recalls = [x["word_recall"] for x in ok_rs if x.get("word_recall") is not None]
            speech_rows.append({
                "mode": mode, "engine": eng, "workload": wl, "task": pk,
                "total": len(rs), "ok": len(ok_rs),
                "rtf_p50": pct(rtfs, 50), "rtf_p95": pct(rtfs, 95),
                "word_recall_mean": avg(recalls),
            })

    docker_base = None
    for cand in (out_dir / "docker_baseline.json", out_dir.parent / "results-run26" / "docker_baseline.json"):
        if cand.exists():
            docker_base = json.loads(cand.read_text())
            break

    summary = {
        "docker_baseline": docker_base,
        "cold_starts": cold_starts,
        "steps": steps,
        "speech": speech_rows,
        "recoveries": recoveries,
        "phases": phase_stats,
    }
    (out_dir / "summary.json").write_text(json.dumps(summary, indent=2))

    md = ["# vps26 Analysis", ""]
    if docker_base:
        md += [
            "## Docker ON vs OFF Baseline",
            f"- Docker ON machine used avg: {docker_base['docker_on']['mem_used_mb_avg']} MB (Docker daemon RSS avg: {docker_base['docker_rss_mb_avg']} MB)",
            f"- Docker OFF machine used avg: {docker_base['docker_off']['mem_used_mb_avg']} MB",
            f"- Delta machine used memory: {docker_base['delta_mem_used_mb']} MB",
            "",
        ]

    md += [
        "## Workload Steps",
        "| Mode | Engine | Model | Workload | Kind | Level | OK / Total | p50 s | p95 s | TTFT p50 s | Agg t/s | Req t/s | Leaks | Pass / Break |",
        "|---|---|---|---|---|---:|---|---:|---:|---:|---:|---:|---:|---|",
    ]
    for s in steps:
        if s.get("phase_kind") == "index_and_search":
            md.append(
                f"| {s['mode']} | {s['engine']} | {s['model']} | {s['workload']} | index+search | 1 | "
                f"{s['chunks_ok']}/{s['chunks_total']} | {s['index_s']} | - | - | {s['chunks_per_s']} c/s | - | - | "
                f"top1={s['top1_hit_rate']} top5={s['top5_hit_rate']} |"
            )
            continue
        if s.get("phase_kind") == "no_route":
            md.append(f"| {s['mode']} | {s['engine']} | {s['model']} | {s['workload']} | no_route | - | - | - | - | - | - | - | - | no route |")
            continue
        extra = ""
        if s.get("valid_json_rate") is not None:
            extra = f" json={s['valid_json_rate']} acc={s['label_accuracy']}"
        elif s.get("chunks_per_s") is not None:
            extra = f" {s['chunks_per_s']} c/s"
        status = "PASS" + extra if s.get("passed") else f"BREAK: {s.get('break_reason')}"
        md.append(
            f"| {s['mode']} | {s['engine']} | {s['model']} | {s['workload']} | {s['phase_kind']} | {s['level']} | "
            f"{s.get('ok')}/{s.get('total')} | {s.get('p50_s')} | {s.get('p95_s')} | {s.get('ttft_p50_s')} | "
            f"{s.get('agg_tps')} | {s.get('req_tps_mean')} | {s.get('think_leaks')} | {status} |"
        )

    if speech_rows:
        md += ["", "## Speech Details", "| Mode | Engine | Workload | Task | OK / Total | RTF p50 | RTF p95 | Word Recall |", "|---|---|---|---|---|---:|---:|---:|"]
        for sp in speech_rows:
            md.append(
                f"| {sp['mode']} | {sp['engine']} | {sp['workload']} | {sp['task']} | {sp['ok']}/{sp['total']} | "
                f"{sp['rtf_p50']} | {sp['rtf_p95']} | {sp['word_recall_mean']} |"
            )

    md += [
        "",
        "## Per-Phase Resource & Neighbour App Table",
        "| Phase | Secs | CPU avg/peak % | Machine Peak MB | Docker CPU avg % | Docker Anon Peak MB | Engine CPU avg % | Engine Anon/File Peak MB | PG Peak MB | Neighbour Errs | Neighbour p50/p95/max ms | OOM |",
        "|---|---:|---|---:|---:|---:|---:|---|---:|---:|---|---:|",
    ]
    for p in phase_stats:
        md.append(
            f"| {p['phase']} | {p['seconds']} | {p['cpu_avg_pct']} / {p['cpu_peak_pct']} | {p['mem_used_peak_mb']} | "
            f"{p['docker_cpu_avg_pct']} | {p['docker_anon_peak_mb']} | {p['engine_cpu_avg_pct']} | "
            f"{p['engine_anon_peak_mb']} / {p['engine_file_peak_mb']} | {p['pg_rss_peak_mb']} | "
            f"{p['neighbour_errors']}/{p['neighbour_checks']} | {p['neighbour_p50_ms']} / {p['neighbour_p95_ms']} / {p['neighbour_max_ms']} | {p['oom_kills_max']} |"
        )

    (out_dir / "analysis.md").write_text("\n".join(md) + "\n")
    print(f"Wrote {out_dir / 'analysis.md'} and {out_dir / 'summary.json'}")


if __name__ == "__main__":
    main()
