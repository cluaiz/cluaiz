"""Per-second system, Docker daemon, engine, cgroup OOM, and neighbour app sampler for vps26.

Usage:
    python3 sampler.py --out-dir <out> [--neighbour-url http://127.0.0.1:3100/health]
Stops cleanly when <out>/stop exists.
"""
import argparse
import json
import os
import re
import threading
import time
import urllib.error
import urllib.request
from pathlib import Path

TICK = os.sysconf("SC_CLK_TCK")
DISK_RE = re.compile(r"^(sd[a-z]+|vd[a-z]+|xvd[a-z]+|nvme\d+n\d+)$")
DOCKER_COMMS = {"dockerd", "containerd", "containerd-shim-runc-v2", "docker-proxy", "runc"}


def read_cpu_total():
    vals = list(map(int, open("/proc/stat").readline().split()[1:]))
    idle = vals[3] + (vals[4] if len(vals) > 4 else 0)
    return sum(vals), idle


def read_meminfo():
    m = {}
    for line in open("/proc/meminfo"):
        k, v = line.split(":", 1)
        m[k] = int(v.split()[0]) / 1024.0
    return m


def read_load1():
    return float(open("/proc/loadavg").read().split()[0])


def read_disk_read_kb():
    kb = 0.0
    for line in open("/proc/diskstats"):
        parts = line.split()
        if len(parts) >= 6 and DISK_RE.match(parts[2]):
            kb += int(parts[5]) * 0.5
    return kb


def read_pgmajfault():
    for line in open("/proc/vmstat"):
        if line.startswith("pgmajfault "):
            return int(line.split()[1])
    return 0


def read_cgroup_oom(cgroup_file):
    try:
        cg = cgroup_file.read_text().strip()
        if not cg:
            return 0
        ev = Path(cg) / "memory.events"
        if not ev.exists():
            return 0
        for line in ev.read_text().splitlines():
            if line.startswith("oom_kill "):
                return int(line.split()[1])
    except OSError:
        pass
    return 0


def read_cgroup_pids(cgroup_file):
    try:
        cg = cgroup_file.read_text().strip()
        if not cg:
            return set()
        procs_f = Path(cg) / "cgroup.procs"
        if not procs_f.exists():
            return set()
        return {line.strip() for line in procs_f.read_text().splitlines() if line.strip().isdigit()}
    except OSError:
        return set()


def scan_procs(target_engine, cg_pids):
    docker_pids = {}
    engine_pids = {}
    pg_rss_kb = 0
    for p in os.listdir("/proc"):
        if not p.isdigit():
            continue
        try:
            comm = open(f"/proc/{p}/comm").read().strip()
            cmd = open(f"/proc/{p}/cmdline", "rb").read().replace(b"\0", b" ").decode(errors="replace")
        except OSError:
            continue
        if not cmd and comm not in DOCKER_COMMS:
            continue

        is_docker_daemon = comm in DOCKER_COMMS
        is_pg = comm == "postgres"
        is_engine = False
        if not is_docker_daemon and "sampler.py" not in cmd and "run26.sh" not in cmd:
            if cg_pids:
                is_engine = p in cg_pids
            elif target_engine == "cluaiz":
                if (
                    "/.cluaiz/bin/cluaiz" in cmd
                    or "/cluaiz_home/bin/cluaiz" in cmd
                    or comm == "cluaiz"
                    or "cluaiz-llama" in cmd
                ):
                    is_engine = True
            elif target_engine == "ollama":
                if (
                    "/bin/ollama" in cmd
                    or comm == "ollama"
                    or "ollama_llama_server" in cmd
                    or ("runner" in cmd and "ollama" in cmd)
                ) and "docker run" not in cmd:
                    is_engine = True
            elif target_engine == "llama":
                if "llama-server" in cmd or comm == "llama-server":
                    if "docker run" not in cmd:
                        is_engine = True

        if not (is_docker_daemon or is_engine or is_pg):
            continue

        try:
            st = open(f"/proc/{p}/stat").read().rsplit(")", 1)[1].split()
            ticks = int(st[11]) + int(st[12])
            anon_kb = file_kb = rss_kb = threads = 0
            for line in open(f"/proc/{p}/status"):
                if line.startswith("RssAnon:"):
                    anon_kb = int(line.split()[1])
                elif line.startswith("RssFile:"):
                    file_kb = int(line.split()[1])
                elif line.startswith("VmRSS:"):
                    rss_kb = int(line.split()[1])
                elif line.startswith("Threads:"):
                    threads = int(line.split()[1])
        except OSError:
            continue

        info = (ticks, anon_kb / 1024.0, file_kb / 1024.0, threads)
        if is_docker_daemon:
            docker_pids[p] = info
        if is_engine:
            engine_pids[p] = info
        if is_pg:
            pg_rss_kb += rss_kb

    return docker_pids, engine_pids, pg_rss_kb / 1024.0


class NeighbourProber:
    def __init__(self, url):
        self.url = url
        self.lock = threading.Lock()
        self.latest = (0, 0.0, 0.0)  # http_status, total_ms, db_ms

    def start(self, stop_path):
        def run():
            while not stop_path.exists():
                t0 = time.perf_counter()
                status = 0
                db_ms = 0.0
                try:
                    with urllib.request.urlopen(self.url, timeout=5.0) as resp:
                        status = resp.status
                        body = json.loads(resp.read())
                        db_ms = float(body.get("db_ms") or 0.0)
                except urllib.error.HTTPError as e:
                    status = e.code
                except Exception:
                    status = 0
                ms = round((time.perf_counter() - t0) * 1000.0, 2)
                with self.lock:
                    self.latest = (status, ms, db_ms)
                elapsed = time.perf_counter() - t0
                if elapsed < 1.0:
                    time.sleep(1.0 - elapsed)

        th = threading.Thread(target=run, daemon=True)
        th.start()

    def get(self):
        with self.lock:
            return self.latest


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--out-dir", required=True)
    ap.add_argument("--neighbour-url", default="http://127.0.0.1:3100/health")
    args = ap.parse_args()

    out_dir = Path(args.out_dir)
    out_dir.mkdir(parents=True, exist_ok=True)
    stop_path = out_dir / "stop"
    phase_path = out_dir / "phase"
    target_path = out_dir / "target"
    cg_path = out_dir / "cgroup_path"
    csv_path = out_dir / "resources.csv"

    prober = NeighbourProber(args.neighbour_url)
    prober.start(stop_path)

    header = (
        "ts,phase,cpu_pct,mem_used_mb,mem_avail_mb,swap_used_mb,cache_mb,load1,"
        "disk_read_kb_s,pgmajfault_s,docker_cpu_pct,docker_anon_mb,docker_file_mb,"
        "engine_procs,engine_threads,engine_cpu_pct,engine_anon_mb,engine_file_mb,"
        "pg_rss_mb,neighbour_http,neighbour_ms,neighbour_db_ms,oom_kills\n"
    )
    new_file = not csv_path.exists() or csv_path.stat().st_size == 0
    out = open(csv_path, "a", buffering=1)
    if new_file:
        out.write(header)

    prev_total, prev_idle = read_cpu_total()
    prev_disk = read_disk_read_kb()
    prev_maj = read_pgmajfault()
    prev_docker_ticks = {}
    prev_engine_ticks = {}
    prev_t = time.monotonic()

    while not stop_path.exists():
        time.sleep(1.0)
        now_m = time.monotonic()
        dt = max(0.05, now_m - prev_t)
        prev_t = now_m

        total, idle = read_cpu_total()
        cpu_pct = 100.0 * (1.0 - (idle - prev_idle) / max(1, total - prev_total))
        prev_total, prev_idle = total, idle

        mi = read_meminfo()
        mem_used = mi["MemTotal"] - mi["MemAvailable"]
        mem_avail = mi["MemAvailable"]
        swap_used = mi.get("SwapTotal", 0.0) - mi.get("SwapFree", 0.0)
        cache_mb = mi.get("Cached", 0.0)
        load1 = read_load1()

        cur_disk = read_disk_read_kb()
        disk_kb_s = max(0.0, (cur_disk - prev_disk) / dt)
        prev_disk = cur_disk

        cur_maj = read_pgmajfault()
        maj_s = max(0.0, (cur_maj - prev_maj) / dt)
        prev_maj = cur_maj

        try:
            phase = phase_path.read_text().strip().replace(",", ";")
        except OSError:
            phase = ""
        try:
            target_engine = target_path.read_text().strip()
        except OSError:
            target_engine = "none"

        cg_pids = read_cgroup_pids(cg_path) if target_engine != "none" else set()
        d_pids, e_pids, pg_rss = scan_procs(target_engine, cg_pids)

        d_ticks_delta = sum(max(0, v[0] - prev_docker_ticks.get(p, v[0])) for p, v in d_pids.items())
        prev_docker_ticks = {p: v[0] for p, v in d_pids.items()}
        d_cpu = 100.0 * d_ticks_delta / TICK / dt
        d_anon = sum(v[1] for v in d_pids.values())
        d_file = sum(v[2] for v in d_pids.values())

        e_ticks_delta = sum(max(0, v[0] - prev_engine_ticks.get(p, v[0])) for p, v in e_pids.items())
        prev_engine_ticks = {p: v[0] for p, v in e_pids.items()}
        e_cpu = 100.0 * e_ticks_delta / TICK / dt
        e_anon = sum(v[1] for v in e_pids.values())
        e_file = sum(v[2] for v in e_pids.values())
        e_threads = sum(v[3] for v in e_pids.values())

        n_http, n_ms, n_db_ms = prober.get()
        oom_kills = read_cgroup_oom(cg_path)

        out.write(
            f"{time.strftime('%Y-%m-%dT%H:%M:%S')},{phase},{cpu_pct:.1f},{mem_used:.1f},{mem_avail:.1f},"
            f"{swap_used:.1f},{cache_mb:.1f},{load1:.2f},{disk_kb_s:.1f},{maj_s:.1f},"
            f"{d_cpu:.1f},{d_anon:.1f},{d_file:.1f},"
            f"{len(e_pids)},{e_threads},{e_cpu:.1f},{e_anon:.1f},{e_file:.1f},"
            f"{pg_rss:.1f},{n_http},{n_ms:.2f},{n_db_ms:.2f},{oom_kills}\n"
        )


if __name__ == "__main__":
    main()
