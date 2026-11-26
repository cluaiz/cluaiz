"""Page-cache snapshot of the GGUF at the start of the pp2048 phase, for every pp_gap.sh run.

    python3 mincore_watch.py <pp_gap out dir> <gguf> [--stop-file F]

Runs beside pp_gap.sh (which is unchanged). It follows the newest r*_*.log of the out dir (lines stamped with the
epoch time by pp_gap.sh) and, at the first pp2048 phase mark of that run, takes one mincore() snapshot of the whole
GGUF: `running pp2048` for `cluaiz bench`; for llama-bench `benchmark N/N: starting` where N is the pp2048 test
(the last test; with -p 2048 alone that is 1/1). mincore only reads the kernel's residency bits of a mapping that
is never touched, so it does not load pages. The GGUF tensor table is read (pure Python) to split the resident
bytes into expert tensors (`*_exps*`) and the rest, and per block for the experts.
Output: <out>/<tag>.mincore.json (one per run).
"""
import ctypes
import ctypes.util
import glob
import json
import mmap
import os
import re
import struct
import sys
import time

OUT, GGUF = sys.argv[1], sys.argv[2]
STOP = sys.argv[4] if len(sys.argv) > 4 and sys.argv[3] == "--stop-file" else os.path.join(OUT, "mincore.stop")
PAGE = mmap.PAGESIZE
libc = ctypes.CDLL(ctypes.util.find_library("c"), use_errno=True)
libc.mmap.restype = ctypes.c_void_p
libc.mmap.argtypes = [ctypes.c_void_p, ctypes.c_size_t, ctypes.c_int, ctypes.c_int, ctypes.c_int, ctypes.c_long]
libc.munmap.argtypes = [ctypes.c_void_p, ctypes.c_size_t]
libc.mincore.argtypes = [ctypes.c_void_p, ctypes.c_size_t, ctypes.c_void_p]


def tensors(path):
    """(name, offset, size) of each tensor, offsets absolute in the file; size = gap to the next tensor."""
    sizes = {0: 1, 1: 1, 2: 2, 3: 2, 4: 4, 5: 4, 6: 4, 7: 1, 10: 8, 11: 8, 12: 8}
    with open(path, "rb") as f:
        def u(fmt, n):
            return struct.unpack("<" + fmt, f.read(n))[0]

        def string():
            return f.read(u("Q", 8))

        def skip(kind):
            if kind == 8:
                string()
            elif kind == 9:
                inner, count = u("I", 4), u("Q", 8)
                if inner in sizes:
                    f.seek(sizes[inner] * count, 1)
                else:
                    for _ in range(count):
                        skip(inner)
            else:
                f.seek(sizes[kind], 1)

        assert f.read(4) == b"GGUF"
        u("I", 4)
        n_tensors, n_kv = u("Q", 8), u("Q", 8)
        align = 32
        for _ in range(n_kv):
            key = string().decode("utf-8", "replace")
            kind = u("I", 4)
            if key == "general.alignment" and kind == 4:
                align = u("I", 4)
            else:
                skip(kind)
        info = []
        for _ in range(n_tensors):
            name = string().decode("utf-8", "replace")
            dims = u("I", 4)
            f.seek(8 * dims, 1)
            u("I", 4)
            info.append((name, u("Q", 8)))
        data = (f.tell() + align - 1) // align * align
    end = os.path.getsize(path)
    info.sort(key=lambda x: x[1])
    return [(n, data + o, (info[i + 1][1] if i + 1 < len(info) else end - data) - o) for i, (n, o) in enumerate(info)]


TENSORS = tensors(GGUF)


def snapshot():
    size = os.path.getsize(GGUF)
    fd = os.open(GGUF, os.O_RDONLY)
    addr = libc.mmap(None, size, mmap.PROT_READ, mmap.MAP_SHARED, fd, 0)
    os.close(fd)
    pages = (size + PAGE - 1) // PAGE
    vec = (ctypes.c_ubyte * pages)()
    t0 = time.time()
    rc = libc.mincore(ctypes.c_void_p(addr), size, vec)
    took = time.time() - t0
    libc.munmap(ctypes.c_void_p(addr), size)
    if rc != 0:
        return {"error": f"mincore errno {ctypes.get_errno()}"}
    bits = bytes(vec)
    resident = sum(b & 1 for b in bits)

    def res(off, n):
        a, b = off // PAGE, (off + n + PAGE - 1) // PAGE
        return sum(x & 1 for x in bits[a:b]) * PAGE

    exp_total = exp_res = oth_total = oth_res = 0
    blocks = {}
    for name, off, n in TENSORS:
        r = res(off, n)
        if "_exps" in name:
            exp_total += n
            exp_res += r
            m = re.match(r"blk\.(\d+)\.", name)
            if m:
                blk = blocks.setdefault(int(m.group(1)), [0, 0])
                blk[0] += n
                blk[1] += r
        else:
            oth_total += n
            oth_res += r
    mib = 1048576
    return {"file_mib": round(size / mib, 1), "resident_mib": round(resident * PAGE / mib, 1),
            "experts_mib": round(exp_total / mib, 1), "experts_resident_mib": round(exp_res / mib, 1),
            "other_mib": round(oth_total / mib, 1), "other_resident_mib": round(oth_res / mib, 1),
            "per_block_experts_resident_pct": {k: round(100 * v[1] / v[0], 1) for k, v in sorted(blocks.items()) if v[0]},
            "mincore_seconds": round(took, 3)}


def phase_mark(line):
    """True on the first line of the pp2048 phase."""
    if "running pp2048" in line:
        return True
    m = re.search(r"llama-bench: benchmark (\d+)/(\d+): starting", line)
    return bool(m and m.group(1) == m.group(2))


done = set()
while not os.path.exists(STOP):
    logs = sorted(glob.glob(os.path.join(OUT, "r*_*.log")), key=os.path.getmtime)
    for log in logs[-1:]:
        tag = os.path.basename(log)[:-4]
        if tag in done or os.path.exists(os.path.join(OUT, tag + ".mincore.json")):
            done.add(tag)
            continue
        with open(log, errors="replace") as f:
            for line in f:
                m = re.match(r"^(\d+\.\d+) (.*)$", line.rstrip("\n"))
                if m and phase_mark(m.group(2)):
                    snap = snapshot()
                    snap.update({"tag": tag, "mark": m.group(2)[:100], "mark_time": float(m.group(1)),
                                 "snapshot_time": time.time()})
                    snap["delay_s"] = round(snap["snapshot_time"] - snap["mark_time"], 2)
                    with open(os.path.join(OUT, tag + ".mincore.json"), "w") as o:
                        json.dump(snap, o, indent=1)
                    done.add(tag)
                    break
    time.sleep(0.2)
