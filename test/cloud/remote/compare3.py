"""Cluaiz, Ollama and LM Studio on the same VM, the same GGUF files and the same prompt, each with its
own defaults; one tool at a time (the others stopped, so the whole GPU is free).

Each model: 1 cold request (after dropping the page cache, so every tool reads the file from disk) and
RUNS warm requests. Prompt = random number + ~540 tokens, 64 new tokens, thinking off. Ollama gets the
file through `ollama create`, LM Studio through `lms import --symbolic-link` (the same bytes).
Every reply goes to OUT/raw.jsonl as it arrives; OUT/results.md at the end.

    python3 compare3.py <out dir> <ctx> <model id> [<model id> ...]
"""
import json, os, random, shutil, subprocess, sys, time, urllib.request
from pathlib import Path

OUT = Path(sys.argv[1]); OUT.mkdir(parents=True, exist_ok=True)
CTX = int(sys.argv[2])
MODELS = sys.argv[3:]
HOME = Path(os.environ.get("CLUAIZ_HOME", Path.home() / "ch-reg"))
B = os.environ.get("CLUAIZ_BIN", str(Path.home() / ".cluaiz/bin/cluaiz"))
LMS = str(Path.home() / ".lmstudio/bin/lms")
RUNS = 3
FILLER = ("The river carried small stones from the mountains to the valley, and the farmers along its "
          "banks used the water for their fields in every season. ")
ENV = dict(os.environ, CLUAIZ_HOME=str(HOME), CLUAIZ_PORT="8012", CLUAIZ_HOST="127.0.0.1")
RAW = open(OUT / "raw.jsonl", "a")
TOOLS = os.environ.get("TOOLS", "cluaiz,ollama,lmstudio").split(",")


def prompt():
    return f"Note {random.randint(0, 10**9)}. {FILLER * 18}\nIn two sentences, what is this text about?"


def post(url, body, headers=None, timeout=1800):
    req = urllib.request.Request(url, json.dumps(body).encode(), {"Content-Type": "application/json", **(headers or {})})
    with urllib.request.urlopen(req, timeout=timeout) as r:
        return json.loads(r.read())


def wait(url, secs=180):
    for _ in range(secs):
        try:
            urllib.request.urlopen(url, timeout=2); return True
        except Exception:
            time.sleep(1)
    return False


def gpu_mib():
    out = subprocess.run(["nvidia-smi", "--query-gpu=memory.used", "--format=csv,noheader,nounits"],
                         capture_output=True, text=True).stdout.strip()
    return int(out) if out.isdigit() else None


def log(row):
    RAW.write(json.dumps(row) + "\n"); RAW.flush(); print(json.dumps(row), flush=True)


def sh(cmd, timeout=3600):
    return subprocess.run(cmd, shell=True, capture_output=True, text=True, timeout=timeout)


def drop_caches():
    sh("sync; echo 3 | sudo tee /proc/sys/vm/drop_caches >/dev/null")


def gguf_of(model_id):
    files = sorted((HOME / "models/chat" / model_id).glob("*.gguf"))
    return [f for f in files if "mmproj" not in f.name.lower()][0]


def tps(n, secs):
    return round(n / secs, 1) if n and secs else None


def ollama_round(model_id):
    name = "cz-" + model_id.lower().replace("_", "-")[:60]
    sh("sudo systemctl start ollama"); wait("http://127.0.0.1:11434/api/version")
    if name not in sh("ollama list").stdout:
        (OUT / "Modelfile").write_text(f"FROM {gguf_of(model_id)}\n")
        start = time.time(); r = sh(f"ollama create {name} -f {OUT / 'Modelfile'}")
        log({"tool": "ollama", "model": model_id, "step": "create", "ok": r.returncode == 0,
             "secs": round(time.time() - start, 1), "out": (r.stdout + r.stderr)[-300:]})
    sh("sudo systemctl restart ollama"); wait("http://127.0.0.1:11434/api/version")
    for run in range(RUNS + 1):
        if run == 0:
            drop_caches()
        body = {"model": name, "stream": False, "think": False, "keep_alive": "10m",
                "messages": [{"role": "user", "content": prompt()}], "options": {"num_predict": 64, "num_ctx": CTX}}
        start = time.time()
        try:
            try:
                d = post("http://127.0.0.1:11434/api/chat", body)
            except urllib.error.HTTPError:
                body.pop("think"); d = post("http://127.0.0.1:11434/api/chat", body)  # no thinking support
        except Exception as e:
            log({"tool": "ollama", "model": model_id, "run": run, "ok": False, "error": repr(e)[:300]}); continue
        ns = 1e9
        log({"tool": "ollama", "model": model_id, "run": run, "ok": True, "secs": round(time.time() - start, 2),
             "load_s": round(d.get("load_duration", 0) / ns, 2), "prompt_tokens": d.get("prompt_eval_count"),
             "prompt_tps": tps(d.get("prompt_eval_count"), d.get("prompt_eval_duration", 0) / ns),
             "gen_tokens": d.get("eval_count"), "gen_tps": tps(d.get("eval_count"), d.get("eval_duration", 0) / ns),
             "gpu_mib": gpu_mib(), "reply": (d.get("message") or {}).get("content", "")[:120]})
    log({"tool": "ollama", "model": model_id, "step": "ps", "out": sh("ollama ps").stdout})
    try:
        post("http://127.0.0.1:11434/api/chat", {"model": name, "messages": [], "keep_alive": 0})
    except Exception:
        pass
    sh("sudo systemctl stop ollama"); time.sleep(3)


def lms_round(model_id):
    if not Path(LMS).exists():
        log({"tool": "lmstudio", "model": model_id, "ok": False, "error": "lms not installed"}); return
    sh(f"{LMS} daemon up"); sh(f"{LMS} server start --port 1234")
    if not wait("http://127.0.0.1:1234/v1/models", 60):
        log({"tool": "lmstudio", "model": model_id, "ok": False, "error": "server did not start"}); return
    gguf = gguf_of(model_id)
    key = gguf.stem.lower()
    r = sh(f"{LMS} import -y --symbolic-link {gguf}")
    log({"tool": "lmstudio", "model": model_id, "step": "import", "ok": r.returncode == 0, "out": (r.stdout + r.stderr)[-300:]})
    listed = sh(f"{LMS} ls --json").stdout
    try:
        # the import names the model after its Hugging Face card, so find it by the linked file name
        key = next(m["modelKey"] for m in json.loads(listed) if (m.get("path") or "").endswith(gguf.name))
    except Exception:
        pass
    for run in range(RUNS + 1):
        if run == 0:
            sh(f"{LMS} unload --all"); drop_caches()
            start = time.time()
            r = sh(f"{LMS} load {key} -y --gpu max --context-length {CTX} --identifier cz")
            log({"tool": "lmstudio", "model": model_id, "step": "load", "ok": r.returncode == 0,
                 "secs": round(time.time() - start, 1), "out": (r.stdout + r.stderr)[-400:]})
            if r.returncode != 0:
                break
            load_s = time.time() - start
        body = {"model": "cz", "stream": False, "max_tokens": 64, "temperature": 0.8,
                "messages": [{"role": "user", "content": prompt()}]}
        start = time.time()
        try:
            d = post("http://127.0.0.1:1234/api/v0/chat/completions", body)
        except Exception as e:
            log({"tool": "lmstudio", "model": model_id, "run": run, "ok": False, "error": repr(e)[:300]}); continue
        st = d.get("stats") or {}; u = d.get("usage") or {}
        secs = time.time() - start + (load_s if run == 0 else 0)
        log({"tool": "lmstudio", "model": model_id, "run": run, "ok": True, "secs": round(secs, 2),
             "prompt_tokens": u.get("prompt_tokens"), "prompt_tps": tps(u.get("prompt_tokens"), st.get("time_to_first_token")),
             "gen_tokens": u.get("completion_tokens"), "gen_tps": round(st["tokens_per_second"], 1) if st.get("tokens_per_second") else None,
             "gpu_mib": gpu_mib(), "runtime": (d.get("runtime") or {}).get("name"),
             "reply": ((d.get("choices") or [{}])[0].get("message") or {}).get("content", "")[:120]})
    log({"tool": "lmstudio", "model": model_id, "step": "ps", "out": sh(f"{LMS} ps").stdout[-400:]})
    sh(f"{LMS} unload --all"); sh(f"{LMS} server stop"); time.sleep(3)


def cluaiz_round(model_id):
    drop_caches()
    server = subprocess.Popen([B, "serve"], env=ENV, stdout=open(OUT / f"cluaiz_{model_id}.log", "w"), stderr=subprocess.STDOUT)
    try:
        wait("http://127.0.0.1:8012/health")
        token = (HOME / "session.token").read_text().strip()
        for run in range(RUNS + 1):
            body = {"model": model_id, "max_tokens": 64, "think_mode": "off", "temporary_chat": "strict",
                    "messages": [{"role": "user", "content": prompt()}]}
            start = time.time()
            try:
                d = post("http://127.0.0.1:8012/v1/chat/completions", body, {"Authorization": f"Bearer {token}"})
            except Exception as e:
                log({"tool": "cluaiz", "model": model_id, "run": run, "ok": False, "error": repr(e)[:300]}); continue
            eng = d.get("engine") or {}; t = eng.get("timings") or {}; load = eng.get("load") or {}
            log({"tool": "cluaiz", "model": model_id, "run": run, "ok": True, "secs": round(time.time() - start, 2),
                 "prompt_tokens": t.get("prompt_n"), "prompt_tps": round(t["prompt_per_second"], 1) if t.get("prompt_per_second") else None,
                 "gen_tokens": t.get("predicted_n"), "gen_tps": round(t["predicted_per_second"], 1) if t.get("predicted_per_second") else None,
                 "placement": load.get("placement"), "n_gpu_layers": load.get("n_gpu_layers"), "n_cpu_moe": load.get("n_cpu_moe"),
                 "n_ctx": load.get("n_ctx"), "gpu_mib": gpu_mib(),
                 "reply": ((d.get("choices") or [{}])[0].get("message") or {}).get("content", "")[:120]})
    finally:
        server.kill(); server.wait(); time.sleep(3)


log({"step": "versions", "ollama": sh("ollama --version").stdout.strip(), "cluaiz": sh(f"{B} --version").stdout.strip(),
     "lms": sh(f"{LMS} version").stdout.strip()[-200:] if Path(LMS).exists() else None,
     "gpu": sh("nvidia-smi --query-gpu=name,driver_version,memory.total --format=csv,noheader").stdout.strip(), "ctx": CTX})
sh("sudo systemctl stop ollama")
rounds = {"cluaiz": cluaiz_round, "ollama": ollama_round, "lmstudio": lms_round}
for m in MODELS:
    for tool in TOOLS:
        try:
            rounds[tool](m)
        except Exception as e:
            log({"tool": tool, "model": m, "ok": False, "error": "round failed: " + repr(e)[:300]})

rows = [json.loads(l) for l in open(OUT / "raw.jsonl") if l.strip()]
avg = lambda xs: round(sum(xs) / len(xs), 1) if xs else None
lines = ["| Model | Tool | Cold request s | Prompt t/s | Generation t/s | GPU MiB |", "|---|---|---|---|---|---|"]
for m in MODELS:
    for tool in TOOLS:
        rs = [r for r in rows if r.get("tool") == tool and r.get("model") == m and "run" in r and r.get("ok")]
        cold = [r for r in rs if r["run"] == 0]; warm = [r for r in rs if r["run"] > 0]
        lines.append(f"| {m} | {tool} | {cold[0]['secs'] if cold else '-'} | {avg([r['prompt_tps'] for r in warm if r.get('prompt_tps')])} | "
                     f"{avg([r['gen_tps'] for r in warm if r.get('gen_tps')])} | {max([r['gpu_mib'] or 0 for r in warm], default='-')} |")
(OUT / "results.md").write_text("\n".join(lines) + "\n")
print("\n".join(lines))
print("== compare done", flush=True)
