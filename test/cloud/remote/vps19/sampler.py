"""Resource log for run 19: one CSV row per second for the whole run.

Columns: time, phase (the text in <out>/phase), whole-machine CPU % and used memory (MemTotal - MemAvailable),
page cache, and for every Cluaiz process together (command line under ~/.cluaiz/): CPU %, RSS split into
anonymous memory (what the process really holds) and file pages (the memory-mapped model, which the kernel
can drop). Every 10 s it also checks the website container on 127.0.0.1:3000.

    python3 sampler.py <out dir>      (stops when <out>/stop exists)
"""
import os, sys, time, urllib.request
from pathlib import Path

OUT = Path(sys.argv[1]); OUT.mkdir(parents=True, exist_ok=True)
TICK = os.sysconf("SC_CLK_TCK")


def cpu_total():
    f = open("/proc/stat").readline().split()[1:]
    vals = list(map(int, f)); idle = vals[3] + vals[4]
    return sum(vals), idle


def meminfo():
    m = {}
    for line in open("/proc/meminfo"):
        k, v = line.split(":"); m[k] = int(v.split()[0]) // 1024
    return m


def cluaiz_pids():
    pids = []
    for p in os.listdir("/proc"):
        if not p.isdigit():
            continue
        try:
            cmd = open(f"/proc/{p}/cmdline", "rb").read().replace(b"\0", b" ").decode(errors="replace")
        except OSError:
            continue
        if "/.cluaiz/" in cmd and "sampler.py" not in cmd:
            pids.append(p)
    return pids


def proc(p):
    try:
        st = open(f"/proc/{p}/stat").read().rsplit(")", 1)[1].split()
        ticks = int(st[11]) + int(st[12])
        anon = filep = 0
        for line in open(f"/proc/{p}/status"):
            if line.startswith("RssAnon:"):
                anon = int(line.split()[1]) // 1024
            elif line.startswith("RssFile:"):
                filep = int(line.split()[1]) // 1024
        return ticks, anon, filep
    except OSError:
        return None


def web_ok():
    try:
        with urllib.request.urlopen("http://127.0.0.1:3000/", timeout=5) as r:
            return r.status
    except Exception:
        return 0


out = open(OUT / "resources.csv", "a", buffering=1)
if out.tell() == 0:
    out.write("time,phase,cpu_pct,mem_used_mb,cache_mb,cluaiz_procs,cluaiz_cpu_pct,cluaiz_anon_mb,cluaiz_file_mb,web\n")
prev_total, prev_idle = cpu_total(); prev_ticks = {}; prev_t = time.time(); n = 0; web = web_ok()
while not (OUT / "stop").exists():
    time.sleep(1)
    now = time.time(); total, idle = cpu_total()
    cpu = 100.0 * (1 - (idle - prev_idle) / max(1, total - prev_total)); prev_total, prev_idle = total, idle
    m = meminfo(); used = m["MemTotal"] - m["MemAvailable"]
    ticks = anon = filep = 0; cur = {}
    for p in cluaiz_pids():
        r = proc(p)
        if r:
            cur[p] = r[0]; anon += r[1]; filep += r[2]
            ticks += r[0] - prev_ticks.get(p, r[0])
    ccpu = 100.0 * ticks / TICK / max(0.001, now - prev_t); prev_ticks = cur; prev_t = now
    n += 1
    if n % 10 == 0:
        web = web_ok()
    try:
        phase = (OUT / "phase").read_text().strip().replace(",", ";")
    except OSError:
        phase = ""
    out.write(f"{time.strftime('%H:%M:%S')},{phase},{cpu:.1f},{used},{m['Cached']},{len(cur)},{ccpu:.1f},{anon},{filep},{web}\n")
