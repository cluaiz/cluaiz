"""Engine regression check: every installed GGUF chat model x placement mode, through the real chat API.

For each run it checks that the request succeeds, the answer is not empty, no control-token text leaks into
the answer or the reasoning, and records llama.cpp's own timings (response "engine.timings"). With
--llama-bench it also runs llama.cpp's llama-bench with the same placement and settings and compares speeds.
With --report it writes a markdown report (hardware, commits, every setting, exact llama-bench commands).

Models come from the running engine (/v1/models, category "chat"); nothing is hardcoded. Modes that cannot
fit are skipped by measured sizes: any mode when the file is bigger than free RAM + free VRAM (unless
--allow-big), "cpu" when the file is bigger than free RAM.

Usage (engine running: cluaiz serve):
  python test/engine_regression/run.py
  python test/engine_regression/run.py --modes auto,gpu,split,cpu --runs 3 \\
      --llama-bench target/llama-ref/bin/Release/llama-bench.exe --report test/engine_regression/RESULTS.md
Exit code: 0 = all passed, 1 = a check failed.
"""
import argparse, datetime, json, platform, random, re, shutil, subprocess, sys, time, urllib.error, urllib.request
from pathlib import Path

# Text a model should never show the user: chat-template control tokens ("<|...|>", "<|tag>", "<tag|>")
CONTROL_TOKEN = re.compile(r"<\|[^<>\s]{0,40}>|<[^<>\s]{1,40}\|>")
FILLER = ("The river carried small stones from the mountains to the valley, and the farmers along its "
          "banks used the water for their fields in every season. ")
REPO = Path(__file__).resolve().parents[2]


def api(base, token, path, body=None, timeout=900):
    req = urllib.request.Request(base + path, data=None if body is None else json.dumps(body).encode(),
                                 headers={"Content-Type": "application/json", "Authorization": f"Bearer {token}"})
    with urllib.request.urlopen(req, timeout=timeout) as r:
        return json.loads(r.read())


def free_memory_gb():
    """(free RAM, free VRAM) in GB, best effort per OS; None when unknown."""
    ram = vram = None
    try:
        if sys.platform == "win32":
            import ctypes

            class MemoryStatus(ctypes.Structure):
                _fields_ = [("dwLength", ctypes.c_ulong), ("dwMemoryLoad", ctypes.c_ulong),
                            ("ullTotalPhys", ctypes.c_ulonglong), ("ullAvailPhys", ctypes.c_ulonglong),
                            ("ullTotalPageFile", ctypes.c_ulonglong), ("ullAvailPageFile", ctypes.c_ulonglong),
                            ("ullTotalVirtual", ctypes.c_ulonglong), ("ullAvailVirtual", ctypes.c_ulonglong),
                            ("ullAvailExtendedVirtual", ctypes.c_ulonglong)]
            status = MemoryStatus()
            status.dwLength = ctypes.sizeof(MemoryStatus)
            if ctypes.windll.kernel32.GlobalMemoryStatusEx(ctypes.byref(status)):
                ram = status.ullAvailPhys / 2**30
        elif Path("/proc/meminfo").exists():
            info = dict(line.split(":", 1) for line in Path("/proc/meminfo").read_text().splitlines() if ":" in line)
            ram = int(info["MemAvailable"].split()[0]) / 2**20
        elif shutil.which("vm_stat"):
            out = subprocess.run(["vm_stat"], capture_output=True, text=True, timeout=10).stdout
            page = int(re.search(r"page size of (\d+)", out).group(1))
            pages = sum(int(re.search(rf"{k}:\s+(\d+)", out).group(1)) for k in ("Pages free", "Pages inactive"))
            ram = pages * page / 2**30
    except Exception:
        ram = None
    if shutil.which("nvidia-smi"):
        try:
            out = subprocess.run(["nvidia-smi", "--query-gpu=memory.free", "--format=csv,noheader,nounits"],
                                 capture_output=True, text=True, timeout=10).stdout.split()
            vram = sum(float(x) for x in out) / 1024 if out else None
        except Exception:
            pass
    return ram, vram


def git_rev(path):
    try:
        out = subprocess.run(["git", "-C", str(path), "log", "-1", "--format=%h %cd %s", "--date=short"],
                             capture_output=True, text=True, timeout=10).stdout.strip()
        dirty = subprocess.run(["git", "-C", str(path), "status", "--porcelain", "--untracked-files=no"],
                               capture_output=True, text=True, timeout=10).stdout.strip()
        return out + (" (+ uncommitted changes)" if dirty else "")
    except Exception:
        return "unknown"


def model_file(models_dir, model_id):
    folder = Path(models_dir) / "chat" / model_id
    files = [f for f in folder.glob("*.gguf") if not f.name.lower().startswith(("mmproj", "mtp-"))]
    return max(files, key=lambda f: f.stat().st_size) if files else None


def chat(base, token, model_id, llama, max_tokens, stream, think_mode, unload):
    body = {"model": model_id, "max_tokens": max_tokens, "stream": stream, "temporary_chat": "strict",
            "think_mode": think_mode,
            "messages": [{"role": "user", "content": f"Note {random.randint(0, 10**9)}. {FILLER * 18}\n"
                                                     "In two sentences, what is this text about?"}]}
    if llama:
        body["llama"] = llama
    if unload:
        body["keep_alive"] = 0
    if not stream:
        resp = api(base, token, "/v1/chat/completions", body)
        if "error" in resp:
            raise RuntimeError(resp["error"])
        msg = resp["choices"][0]["message"]
        return msg.get("content") or "", msg.get("reasoning_content") or "", resp.get("engine") or {}
    req = urllib.request.Request(base + "/v1/chat/completions", data=json.dumps(body).encode(),
                                 headers={"Content-Type": "application/json", "Authorization": f"Bearer {token}"})
    content = reasoning = ""
    engine = {}
    with urllib.request.urlopen(req, timeout=900) as r:
        for raw in r:
            line = raw.decode("utf-8", "replace").strip()
            if not line.startswith("data:") or line.endswith("[DONE]"):
                continue
            chunk = json.loads(line[5:])
            engine = chunk.get("engine") or engine
            for ch in chunk.get("choices", []):
                d = ch.get("delta", {})
                content += d.get("content") or ""
                reasoning += d.get("reasoning_content") or ""
    if content.startswith("Error:"):
        raise RuntimeError(content)
    return content, reasoning, engine


def mode_params(mode, total_layers):
    if mode == "auto":
        return None
    if mode == "gpu":
        return {"n_gpu_layers": -1}
    if mode == "cpu":
        return {"n_gpu_layers": 0}
    if mode == "split":  # half the blocks on the GPU (+1 = output head, llama.cpp counting)
        return {"n_gpu_layers": max(total_layers // 2, 1) + 1} if total_layers else None
    raise ValueError(mode)


def llama_bench(exe, gguf, load, prompt_n, gen_n):
    """llama-bench with the placement and settings the engine reported; returns (pp, tg, command)."""
    s = load.get("settings") or {}
    kv = {"Kv16": "f16", "Kv8": "q8_0", "Kv4": "q4_0"}.get(load.get("cache_type"), "f16")
    fa = {"On": "1", "Off": "0"}.get(s.get("flash_attn"), "auto")
    ngl = load.get("n_gpu_layers", 0)
    base = [exe, "-m", str(gguf), "-ngl", str(99 if ngl < 0 else ngl), "-ncmoe", str(load.get("n_cpu_moe", 0)),
            "-ctk", kv, "-ctv", kv, "-fa", fa, "-nopo", "0" if load.get("op_offload", True) else "1",
            "-r", "2", "-o", "json"]
    if load.get("placement") == "CpuOnly":
        # The engine gives a CPU-only model no device at all; with -ngl 0 alone llama.cpp still sends the
        # prompt's big matrix work to the GPU (op offload), so its prompt speed would not be a CPU number
        base[-2:-2] = ["-dev", "none"]
    if s:
        base += ["-t", str(s["n_threads"]), "-b", str(s["n_batch"]), "-ub", str(s["n_ubatch"]),
                 "-lm", "mmap" if s.get("use_mmap", True) else "none"]

    def run(extra):
        out = subprocess.run(base + extra, capture_output=True, text=True, timeout=1800)
        rows = json.loads(out.stdout or "[]")
        if not rows:
            raise RuntimeError((out.stderr or "no output").strip().splitlines()[-1][:200])
        return rows[0]["avg_ts"]

    # Same shape as the engine run: the prompt from an empty context, generation after that prompt
    pp = run(["-p", str(prompt_n), "-n", "0"])
    tg = run(["-p", "0", "-n", str(gen_n), "-d", str(prompt_n)])
    shown = ["llama-bench", "-m", gguf.name] + base[3:-2]
    return pp, tg, " ".join(shown) + f" -p {prompt_n} -n 0  |  -p 0 -n {gen_n} -d {prompt_n}"


def write_report(path, rows, args, hardware, started):
    lines = [
        "# Cluaiz engine vs llama.cpp: regression report",
        "",
        f"Generated by `test/engine_regression/run.py` on {started:%Y-%m-%d %H:%M}. Every number below comes from",
        "a real run: the engine through its chat API (`engine.timings` = llama.cpp's own counters), llama.cpp",
        "through `llama-bench` built from the same llama.cpp source with the same placement and settings.",
        "",
        "## Hardware and software",
        "",
        "| | |",
        "|---|---|",
    ]
    for k, v in hardware:
        lines.append(f"| {k} | {v} |")
    lines += [
        "",
        "## Method",
        "",
        f"- Prompt: about {rows[0]['prompt_n'] if rows and rows[0].get('prompt_n') else 'n/a'} tokens of plain text, "
        "a new random number in front each run (no KV prefix reuse).",
        f"- Generation: `max_tokens` {args.max_tokens}. Runs per model and mode: {args.runs}.",
        "- Run 1 is the cold run (first request after load, thinking on `auto`, also checks for leaked control "
        "tokens). Later runs use thinking `off`, so exactly `max_tokens` are generated; speeds are the average "
        "of those warm runs.",
        "- `temporary_chat: strict` (no memory / RAG). Modes: `auto` = planner decides; `gpu` = "
        "`n_gpu_layers -1`; `split` = half the blocks on the GPU; `cpu` = `n_gpu_layers 0`.",
        "- llama-bench gets the placement the engine reported (`-ngl`, `-ncmoe`, KV type, flash attention, "
        "op offload, threads, batch, ubatch, mmap). Prompt speed is measured from an empty context, "
        "generation after the same number of prompt tokens (`-d`), like the engine run.",
        f"- A gap is flagged when engine generation < {args.min_ratio} x llama.cpp.",
        "",
        "## Results (tokens per second)",
        "",
        "| Model | Size | Mode | Result | Placement | n_ctx | KV | FA | op_offload | threads | batch/ubatch "
        "| Engine prompt cold / warm | Engine gen | llama.cpp prompt | llama.cpp gen | gen ratio |",
        "|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|",
    ]
    for r in rows:
        load = r.get("load") or {}
        s = load.get("settings") or {}
        placement = (f"{load.get('placement', '-')} ngl {load.get('n_gpu_layers', '-')}"
                     + (f" ncmoe {load['n_cpu_moe']}" if load.get("n_cpu_moe") else "")) if load else "-"
        ratio = f"{r['tg'] / r['btg']:.2f}" if r.get("btg") and r.get("tg") else "-"
        lines.append(
            f"| {r['model']} | {r['size']} | {r['mode']} | {r['status']} | {placement} | {load.get('n_ctx', '-')} "
            f"| {load.get('cache_type', '-')} | {s.get('flash_attn', '-')} | {load.get('op_offload', '-')} "
            f"| {s.get('n_threads', '-')} | {s.get('n_batch', '-')}/{s.get('n_ubatch', '-')} "
            f"| {r.get('pp_cold', 0):.0f} / {r.get('pp', 0):.0f} | {r.get('tg', 0):.1f} "
            f"| {r.get('bpp', 0) or 0:.0f} | {r.get('btg', 0) or 0:.1f} | {ratio} |")
    notes = [r for r in rows if r.get("notes")]
    if notes:
        lines += ["", "## Notes per run", ""]
        lines += [f"- **{r['model']} / {r['mode']}**: {r['notes']}" for r in notes]
    lines += ["", "## Exact llama-bench commands", ""]
    lines += [f"- **{r['model']} / {r['mode']}**: `{r['bench_cmd']}`" for r in rows if r.get("bench_cmd")]
    lines += ["", "## Planner decisions (auto mode)", ""]
    lines += [f"- **{r['model']}**: {(r.get('load') or {}).get('planner', '-')}" for r in rows if r["mode"] == "auto"]
    lines += ["", "## Reproduce", "", "```",
              "cargo build --release -p cmd && target/release/cluaiz.exe serve",
              "python test/engine_regression/run.py " + " ".join(sys.argv[1:]),
              "```", ""]
    Path(path).write_text("\n".join(lines), encoding="utf-8")


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--api", default="http://127.0.0.1:8000")
    ap.add_argument("--token-file", default=str(Path.home() / ".cluaiz" / "session.token"))
    ap.add_argument("--models-dir", default=str(Path.home() / ".cluaiz" / "models"))
    ap.add_argument("--modes", default="auto,gpu,split", help="comma list of auto,gpu,split,cpu")
    ap.add_argument("--only", default="", help="run models whose id contains this text")
    ap.add_argument("--skip", default="", help="comma list: skip models whose id contains any of these")
    ap.add_argument("--runs", type=int, default=3, help="runs per model and mode (the first one is the cold run)")
    ap.add_argument("--max-tokens", type=int, default=64)
    ap.add_argument("--allow-big", action="store_true", help="also run models bigger than free RAM + VRAM")
    ap.add_argument("--llama-bench", default="", help="path to llama-bench to compare speeds")
    ap.add_argument("--min-ratio", type=float, default=0.85, help="flag when engine gen < ratio x llama-bench")
    ap.add_argument("--report", default="", help="write a markdown report to this path")
    args = ap.parse_args()
    if args.runs < 2:
        sys.exit("--runs must be at least 2 (run 1 is the cold run)")
    if args.llama_bench:
        # absolute path: Windows does not resolve a relative executable path against the working directory
        args.llama_bench = str(Path(args.llama_bench).resolve())
        if not Path(args.llama_bench).is_file():
            sys.exit(f"llama-bench not found: {args.llama_bench}")

    started = datetime.datetime.now()
    token = Path(args.token_file).read_text().strip()
    models = [m["id"] for m in api(args.api, token, "/v1/models")["data"] if m.get("category") == "chat"]
    skips = [s for s in args.skip.split(",") if s]
    ram, vram = free_memory_gb()
    rows = []

    for model_id in sorted(models):
        if args.only and args.only.lower() not in model_id.lower():
            continue
        if any(s.lower() in model_id.lower() for s in skips):
            continue
        gguf = model_file(args.models_dir, model_id)
        if gguf is None:
            continue
        size_gb = gguf.stat().st_size / 2**30
        row_base = {"model": model_id, "size": f"{size_gb:.2f} GB"}
        if not args.allow_big and ram is not None and size_gb > ram + (vram or 0):
            rows.append({**row_base, "mode": "-", "status": "SKIP",
                         "notes": f"file {size_gb:.1f} GB > free RAM + VRAM (use --allow-big)"})
            continue
        total_layers, auto_placement = None, None
        for mode in [m for m in args.modes.split(",") if m]:
            row = {**row_base, "mode": mode}
            rows.append(row)
            ram, vram = free_memory_gb()
            if mode == "cpu" and ram is not None and size_gb > ram:
                row.update(status="SKIP", notes=f"cpu: file {size_gb:.1f} GB > free RAM {ram:.1f} GB")
                continue
            llama = mode_params(mode, total_layers)
            if mode == "split" and llama is None:
                row.update(status="SKIP", notes="layer count unknown (run auto first)")
                continue
            speeds, problems, load = [], [], {}
            for run in range(args.runs):
                try:
                    # Run 0 (cold): thinking on auto, checks leaks. Later runs: thinking off, so speed is
                    # measured on exactly max_tokens (thinking tokens are not limited by max_tokens)
                    content, reasoning, engine = chat(args.api, token, model_id, llama, args.max_tokens,
                                                      stream=(run % 2 == 0),
                                                      think_mode="auto" if run == 0 else "off",
                                                      unload=(run == args.runs - 1))
                except (urllib.error.URLError, RuntimeError, KeyError, json.JSONDecodeError) as e:
                    problems.append(f"request failed: {e}")
                    break
                load = engine.get("load") or load
                total_layers = load.get("total_layers") or total_layers
                if not content.strip():
                    problems.append("empty answer")
                for label, text in (("answer", content), ("reasoning", reasoning)):
                    leak = CONTROL_TOKEN.search(text)
                    if leak:
                        problems.append(f"control token in {label}: {leak.group(0)!r}")
                t = engine.get("timings") or {}
                speeds.append((t.get("prompt_n"), t.get("prompt_per_second", 0), t.get("predicted_per_second", 0)))
            if mode == "auto":
                auto_placement = load.get("placement")
            warm = speeds[1:]
            row.update(load=load, prompt_n=speeds[0][0] if speeds else None,
                       pp_cold=speeds[0][1] if speeds else 0,
                       pp=sum(s[1] for s in warm) / len(warm) if warm else 0,
                       tg=sum(s[2] for s in warm) / len(warm) if warm else 0)
            if problems and mode == "gpu" and auto_placement not in (None, "GpuOnly") and not speeds:
                # Forcing every layer onto the GPU for a model the planner could not fit there: a load error
                # is the correct outcome, not an engine regression
                row.update(status="NOT-FIT", notes="forced full GPU does not fit: " + "; ".join(problems))
                continue
            if problems:
                row.update(status="FAIL", notes="; ".join(problems))
                continue
            row["status"] = "PASS"
            if args.llama_bench and load:
                try:
                    bpp, btg, cmd = llama_bench(args.llama_bench, gguf, load, row["prompt_n"] or 512, args.max_tokens)
                    row.update(bpp=bpp, btg=btg, bench_cmd=cmd)
                    if row["tg"] < args.min_ratio * btg:
                        row["notes"] = f"engine generation {row['tg'] / btg:.2f}x llama.cpp"
                except Exception as e:  # llama-bench missing / crashed: report, do not fail the engine check
                    row["notes"] = f"llama-bench failed: {e}"

    width = max((len(r["model"]) for r in rows), default=10)
    print(f"\n{'model':<{width}}  {'mode':<5}  {'result':<7}  {'engine pp/tg':<16}  {'llama.cpp pp/tg':<16}  notes")
    for r in rows:
        eng = f"{r.get('pp', 0):.0f} / {r.get('tg', 0):.1f}" if r.get("load") else ""
        ref = f"{r['bpp']:.0f} / {r['btg']:.1f}" if r.get("btg") else ""
        print(f"{r['model']:<{width}}  {r['mode']:<5}  {r['status']:<7}  {eng:<16}  {ref:<16}  {r.get('notes', '')}")
    failed = sum(r["status"] == "FAIL" for r in rows)
    print(f"\n{sum(r['status'] == 'PASS' for r in rows)} passed, {failed} failed, "
          f"{sum(r['status'] in ('SKIP', 'NOT-FIT') for r in rows)} skipped / not fitting")

    if args.report:
        call = {"jsonrpc": "2.0", "id": 1, "method": "system.info", "params": {}}
        info = (api(args.api, token, "/v1/cep", call).get("result") or {}).get("hardware") or {}
        cpu, mem, gpus = info.get("cpu") or {}, info.get("memory") or {}, info.get("gpus") or []
        driver = ""
        if shutil.which("nvidia-smi"):
            driver = subprocess.run(["nvidia-smi", "--query-gpu=driver_version", "--format=csv,noheader"],
                                    capture_output=True, text=True, timeout=10).stdout.strip()
        hardware = [
            ("CPU", f"{cpu.get('brand', '?').strip()} | {cpu.get('physical_cores', '?')} cores / "
                    f"{cpu.get('logical_threads', '?')} threads | {', '.join(cpu.get('isa_features') or [])}"),
            ("RAM", f"{mem.get('total_gb', 0):.1f} GB total | {ram:.1f} GB free at the end of the run" if ram
                    else f"{mem.get('total_gb', 0):.1f} GB total"),
        ] + [("GPU", f"{g.get('name')} | {g.get('vram_total_gb', 0):.1f} GB VRAM | compute {g.get('compute_capability')}"
                     + (f" | driver {driver}" if driver else "")) for g in gpus] + [
            ("OS", f"{info.get('os_name', platform.system())} {info.get('os_version', platform.release())}"),
            ("Cluaiz commit", git_rev(REPO)),
            ("llama.cpp (fork) commit", git_rev(REPO / "engine" / "llama" / "llama.cpp")),
            ("llama-bench", args.llama_bench or "not run"),
        ]
        write_report(args.report, rows, args, hardware, started)
        print(f"report written: {args.report}")
    sys.exit(1 if failed else 0)


if __name__ == "__main__":
    main()
