"""Cluaiz and Ollama on the same VM, the same GGUF files, the same prompt, each with its own defaults.

Each model: Ollama is given the file Cluaiz already has (`ollama create` from the GGUF), then
1 cold + RUNS warm chat requests on each tool, one tool at a time (the other is stopped, so the GPU
is free). Prompt = run.py's prompt (random number + ~540 tokens), 64 new tokens, thinking off.
Every reply is written to OUT/raw.jsonl as it arrives; OUT/results.md at the end.

    python3 ollama_compare.py <out dir> <ctx> <model id> [<model id> ...]
"""
import json, os, random, subprocess, sys, time, urllib.request
from pathlib import Path

OUT = Path(sys.argv[1]); OUT.mkdir(parents=True, exist_ok=True)
CTX = int(sys.argv[2])
MODELS = sys.argv[3:]
HOME = Path.home() / "ch"
B = str(Path.home() / ".cluaiz/bin/cluaiz")
RUNS = 3
FILLER = ("The river carried small stones from the mountains to the valley, and the farmers along its "
          "banks used the water for their fields in every season. ")
ENV = dict(os.environ, CLUAIZ_HOME=str(HOME), CLUAIZ_PORT="8012", CLUAIZ_HOST="127.0.0.1")
RAW = open(OUT / "raw.jsonl", "a")


def prompt():
    return f"Note {random.randint(0, 10**9)}. {FILLER * 18}\nIn two sentences, what is this text about?"


def post(url, body, headers=None, timeout=1800):
    req = urllib.request.Request(url, json.dumps(body).encode(), {"Content-Type": "application/json", **(headers or {})})
    with urllib.request.urlopen(req, timeout=timeout) as r:
        return json.loads(r.read())


def wait(url):
    for _ in range(180):
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


def sh(cmd):
    return subprocess.run(cmd, shell=True, capture_output=True, text=True)


def gguf_of(model_id):
    folder = HOME / "models/chat" / model_id
    files = sorted(folder.glob("*.gguf"))
    return [f for f in files if "mmproj" not in f.name.lower()][0]


def ollama_name(model_id):
    return "cz-" + model_id.lower().replace("_", "-")[:60]


def ollama_round(model_id):
    name = ollama_name(model_id)
    sh("sudo systemctl start ollama"); wait("http://127.0.0.1:11434/api/version")
    if name not in sh("ollama list").stdout:
        (OUT / "Modelfile").write_text(f"FROM {gguf_of(model_id)}\n")
        start = time.time(); r = sh(f"ollama create {name} -f {OUT / 'Modelfile'}")
        log({"tool": "ollama", "model": model_id, "step": "create", "ok": r.returncode == 0,
             "secs": round(time.time() - start, 1), "out": (r.stdout + r.stderr)[-300:]})
    for run in range(RUNS + 1):
        body = {"model": name, "stream": False, "think": False, "keep_alive": "10m",
                "messages": [{"role": "user", "content": prompt()}],
                "options": {"num_predict": 64, "num_ctx": CTX}}
        start = time.time()
        try:
            d = post("http://127.0.0.1:11434/api/chat", body)
        except Exception as e:
            body.pop("think"); d = None
            try:
                d = post("http://127.0.0.1:11434/api/chat", body)  # a model without thinking support
            except Exception as e2:
                log({"tool": "ollama", "model": model_id, "run": run, "ok": False, "error": repr(e2)[:300]}); continue
        ns = 1e9
        log({"tool": "ollama", "model": model_id, "run": run, "ok": True, "secs": round(time.time() - start, 2),
             "load_s": round(d.get("load_duration", 0) / ns, 2),
             "prompt_tokens": d.get("prompt_eval_count"), "prompt_tps": round(d["prompt_eval_count"] / (d["prompt_eval_duration"] / ns), 1) if d.get("prompt_eval_duration") else None,
             "gen_tokens": d.get("eval_count"), "gen_tps": round(d["eval_count"] / (d["eval_duration"] / ns), 1) if d.get("eval_duration") else None,
             "gpu_mib": gpu_mib(), "reply": (d.get("message") or {}).get("content", "")[:120]})
    log({"tool": "ollama", "model": model_id, "step": "ps", "out": sh("ollama ps").stdout})
    post("http://127.0.0.1:11434/api/chat", {"model": name, "messages": [], "keep_alive": 0})
    sh("sudo systemctl stop ollama"); time.sleep(3)


def cluaiz_round(model_id):
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
                 "placement": load.get("placement"), "n_gpu_layers": load.get("n_gpu_layers"), "n_ctx": load.get("n_ctx"),
                 "gpu_mib": gpu_mib(), "reply": ((d.get("choices") or [{}])[0].get("message") or {}).get("content", "")[:120]})
    finally:
        server.kill(); server.wait(); time.sleep(3)


log({"step": "versions", "ollama": sh("ollama --version").stdout.strip(), "cluaiz": sh(f"{B} --version").stdout.strip(),
     "gpu": sh("nvidia-smi --query-gpu=name,driver_version,memory.total --format=csv,noheader").stdout.strip(), "ctx": CTX})
sh("sudo systemctl stop ollama")
for m in MODELS:
    cluaiz_round(m)
    ollama_round(m)

# Table: warm runs (1..RUNS) averaged; run 0 is the cold one
rows = [json.loads(l) for l in open(OUT / "raw.jsonl") if l.strip()]
avg = lambda xs: round(sum(xs) / len(xs), 1) if xs else None
lines = ["| Model | Tool | Cold request s | Prompt t/s | Generation t/s | GPU MiB | Placement |", "|---|---|---|---|---|---|---|"]
for m in MODELS:
    for tool in ("cluaiz", "ollama"):
        rs = [r for r in rows if r.get("tool") == tool and r.get("model") == m and "run" in r and r.get("ok")]
        cold = [r for r in rs if r["run"] == 0]; warm = [r for r in rs if r["run"] > 0]
        place = warm[-1].get("placement") if tool == "cluaiz" and warm else next((r["out"].splitlines()[-1] for r in rows if r.get("tool") == "ollama" and r.get("model") == m and r.get("step") == "ps" and r.get("out")), "")
        lines.append(f"| {m} | {tool} | {cold[0]['secs'] if cold else '-'} | {avg([r['prompt_tps'] for r in warm if r.get('prompt_tps')])} | "
                     f"{avg([r['gen_tps'] for r in warm if r.get('gen_tps')])} | {max([r['gpu_mib'] or 0 for r in warm], default='-')} | {place} |")
(OUT / "results.md").write_text("\n".join(lines) + "\n")
print("\n".join(lines))
print("== compare done", flush=True)
