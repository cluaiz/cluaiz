"""Unified workload runner for vps26 across Cluaiz, Ollama, and llama-server.

Supports W1 (ticket classification JSON), W2 (1000-word summary), W3 (5-turn chat),
W4 (embeddings + cosine search hit rate), W5 (TTS + STT), and W6 (all at once),
plus step-ramp breaking-point detection, optional soak, and crash recovery probe.
"""
import argparse
import base64
import json
import math
import operator
import os
import signal
import struct
import subprocess
import threading
import time
import urllib.error
import urllib.request
from pathlib import Path

LABELS = ("billing", "bug", "account", "feature", "outage")

CLUAIZ_MODEL_MAP = {
    "qwen3-0.6b": "qwen3-0.6b-gguf-Q4_K_M",
    "qwen3-1.7b": "qwen3-1.7b-gguf-Q4_K_M",
    "llama3.2-3b": "llama-3.2-3b-instruct-gguf-Q4_K_M",
    "embed-gemma": "embeddinggemma-300m-gguf-Q8_0",
    "kokoro": "kokoro-82m-v1.0-onnx-UINT8",
    "moonshine-tiny": "moonshine-tiny-onnx-INT8",
    "moonshine-base": "moonshine-base-onnx-INT8",
}

OLLAMA_MODEL_MAP = {
    "qwen3-0.6b": "vps26-qwen3-0.6b",
    "qwen3-1.7b": "vps26-qwen3-1.7b",
    "llama3.2-3b": "vps26-llama3.2-3b",
    "embed-gemma": "vps26-embed-gemma",
}


def percentile(vals, p):
    if not vals:
        return None
    s = sorted(vals)
    idx = (len(s) - 1) * (p / 100.0)
    lo = int(math.floor(idx))
    hi = int(math.ceil(idx))
    if lo == hi:
        return round(s[lo], 3)
    return round(s[lo] + (s[hi] - s[lo]) * (idx - lo), 3)


def wav_seconds(wav):
    if len(wav) < 44 or wav[:4] != b"RIFF":
        return None
    rate = struct.unpack("<I", wav[24:28])[0]
    block = struct.unpack("<H", wav[32:34])[0]
    if rate <= 0 or block <= 0:
        return None
    i = 12
    while i + 8 <= len(wav):
        cid = wav[i : i + 4]
        size = struct.unpack("<I", wav[i + 4 : i + 8])[0]
        if cid == b"data":
            return min(size, len(wav) - i - 8) / float(rate * block)
        i += 8 + size
    return None


def cgroup_oom_kills(out_dir):
    cg_file = out_dir / "cgroup_path"
    try:
        cg = cg_file.read_text().strip()
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


def port_alive(port):
    url = f"http://127.0.0.1:{port}/api/version" if port == 11435 else f"http://127.0.0.1:{port}/health"
    try:
        with urllib.request.urlopen(url, timeout=3.0) as r:
            return r.status == 200
    except Exception:
        return False


def parse_ticket_json(text):
    clean = text.strip()
    if clean.startswith("```"):
        clean = clean.strip("`").strip()
        if clean.lower().startswith("json"):
            clean = clean[4:].strip()
    try:
        obj = json.loads(clean)
        if isinstance(obj, dict) and isinstance(obj.get("label"), str):
            lbl = obj["label"].strip().lower()
            return True, lbl
        return True, ""
    except Exception:
        start = clean.find("{")
        end = clean.rfind("}")
        if start != -1 and end > start:
            try:
                obj = json.loads(clean[start : end + 1])
                if isinstance(obj, dict) and isinstance(obj.get("label"), str):
                    return True, obj["label"].strip().lower()
                return True, ""
            except Exception:
                pass
        return False, ""


class EngineClient:
    def __init__(self, engine, model_key, ctx, threads, token_file):
        self.engine = engine
        self.model_key = model_key
        self.ctx = ctx
        self.threads = threads
        self.token = ""
        if engine == "cluaiz" and token_file and Path(token_file).exists():
            self.token = Path(token_file).read_text().strip()

    def headers(self):
        h = {"Content-Type": "application/json"}
        if self.engine == "cluaiz" and self.token:
            h["Authorization"] = f"Bearer {self.token}"
        return h

    def chat_port(self):
        if self.engine == "cluaiz":
            return 8011
        if self.engine == "ollama":
            return 11435
        return 8012

    def embed_port(self):
        if self.engine == "cluaiz":
            return 8011
        if self.engine == "ollama":
            return 11435
        return 8013

    def chat_stream(self, messages, max_tokens, json_mode=False, timeout=120):
        t0 = time.monotonic()
        first_tok = None
        pieces = 0
        text_parts = []
        think_leak = False
        prompt_toks = None
        comp_toks = None
        status = 0

        if self.engine == "ollama":
            url = "http://127.0.0.1:11435/api/chat"
            body = {
                "model": OLLAMA_MODEL_MAP[self.model_key],
                "messages": messages,
                "stream": True,
                "think": False,
                "keep_alive": "60m",
                "options": {
                    "num_ctx": self.ctx,
                    "num_gpu": 0,
                    "num_thread": self.threads,
                    "temperature": 0,
                    "num_predict": max_tokens,
                },
            }
            if json_mode:
                body["format"] = "json"
        elif self.engine == "cluaiz":
            url = "http://127.0.0.1:8011/v1/chat/completions"
            body = {
                "model": CLUAIZ_MODEL_MAP[self.model_key],
                "messages": messages,
                "stream": True,
                "stream_options": {"include_usage": True},
                "temperature": 0,
                "max_tokens": max_tokens,
                "think_mode": "off",
                "reasoning_effort": 0,
                "temporary_chat": "strict",
            }
            if json_mode:
                body["response_format"] = {"type": "json_object"}
        else:
            url = "http://127.0.0.1:8012/v1/chat/completions"
            body = {
                "model": self.model_key,
                "messages": messages,
                "stream": True,
                "stream_options": {"include_usage": True},
                "temperature": 0,
                "max_tokens": max_tokens,
            }
            if json_mode:
                body["response_format"] = {"type": "json_object"}

        req = urllib.request.Request(url, json.dumps(body).encode(), self.headers())
        try:
            with urllib.request.urlopen(req, timeout=timeout) as resp:
                status = resp.status
                for raw in resp:
                    line = raw.decode(errors="replace").strip()
                    if not line:
                        continue
                    if self.engine == "ollama":
                        d = json.loads(line)
                        msg = d.get("message") or {}
                        if msg.get("thinking"):
                            think_leak = True
                        piece = msg.get("content") or ""
                        if piece:
                            if first_tok is None:
                                first_tok = time.monotonic() - t0
                            pieces += 1
                            text_parts.append(piece)
                        if d.get("done"):
                            prompt_toks = d.get("prompt_eval_count")
                            comp_toks = d.get("eval_count")
                    else:
                        if not line.startswith("data:"):
                            continue
                        payload = line[5:].strip()
                        if payload == "[DONE]":
                            continue
                        d = json.loads(payload)
                        u = d.get("usage")
                        if isinstance(u, dict):
                            prompt_toks = u.get("prompt_tokens") or prompt_toks
                            comp_toks = u.get("completion_tokens") or comp_toks
                        choices = d.get("choices") or []
                        if choices:
                            delta = choices[0].get("delta") or {}
                            if delta.get("reasoning_content"):
                                think_leak = True
                            piece = delta.get("content") or ""
                            if piece:
                                if first_tok is None:
                                    first_tok = time.monotonic() - t0
                                pieces += 1
                                text_parts.append(piece)
            total_s = time.monotonic() - t0
            text = "".join(text_parts)
            if "<think>" in text or "</think>" in text:
                think_leak = True
            out_n = comp_toks if comp_toks is not None else pieces
            gen_tps = None
            if first_tok is not None and total_s > first_tok and out_n:
                gen_tps = round(out_n / (total_s - first_tok), 2)
            return {
                "ok": bool(text.strip()),
                "http_status": status,
                "ttft_s": round(first_tok if first_tok is not None else total_s, 3),
                "total_s": round(total_s, 3),
                "in_tokens": prompt_toks,
                "out_tokens": out_n,
                "gen_tps": gen_tps,
                "think_leak": think_leak,
                "text": text,
                "error": "" if text.strip() else "empty_response",
            }
        except urllib.error.HTTPError as e:
            err_body = e.read().decode(errors="replace")[:300]
            return {
                "ok": False,
                "http_status": e.code,
                "ttft_s": None,
                "total_s": round(time.monotonic() - t0, 3),
                "in_tokens": None,
                "out_tokens": 0,
                "gen_tps": None,
                "think_leak": think_leak,
                "text": "",
                "error": f"HTTP {e.code}: {err_body}",
            }
        except Exception as e:
            return {
                "ok": False,
                "http_status": 0,
                "ttft_s": None,
                "total_s": round(time.monotonic() - t0, 3),
                "in_tokens": None,
                "out_tokens": 0,
                "gen_tps": None,
                "think_leak": think_leak,
                "text": "",
                "error": repr(e)[:300],
            }

    def embed_batch(self, texts, timeout=120):
        t0 = time.monotonic()
        if self.engine == "ollama":
            url = "http://127.0.0.1:11435/api/embed"
            body = {
                "model": OLLAMA_MODEL_MAP["embed-gemma"],
                "input": texts,
                "keep_alive": "60m",
                "options": {"num_ctx": self.ctx, "num_gpu": 0, "num_thread": self.threads},
            }
        elif self.engine == "cluaiz":
            url = "http://127.0.0.1:8011/v1/embeddings"
            body = {"model": CLUAIZ_MODEL_MAP["embed-gemma"], "input": texts}
        else:
            url = "http://127.0.0.1:8013/v1/embeddings"
            body = {"model": "embed-gemma", "input": texts}

        req = urllib.request.Request(url, json.dumps(body).encode(), self.headers())
        try:
            with urllib.request.urlopen(req, timeout=timeout) as resp:
                status = resp.status
                d = json.loads(resp.read())
            total_s = time.monotonic() - t0
            if self.engine == "ollama":
                vecs = d.get("embeddings") or []
            else:
                items = sorted(d.get("data") or [], key=lambda x: x.get("index", 0))
                vecs = [it["embedding"] for it in items]
            ok = len(vecs) == len(texts)
            return {
                "ok": ok,
                "http_status": status,
                "total_s": round(total_s, 3),
                "count": len(vecs),
                "dims": len(vecs[0]) if vecs else 0,
                "vectors": vecs,
                "error": "" if ok else f"count_mismatch {len(vecs)}!={len(texts)}",
            }
        except urllib.error.HTTPError as e:
            err_body = e.read().decode(errors="replace")[:300]
            return {
                "ok": False,
                "http_status": e.code,
                "total_s": round(time.monotonic() - t0, 3),
                "count": 0,
                "dims": 0,
                "vectors": [],
                "error": f"HTTP {e.code}: {err_body}",
            }
        except Exception as e:
            return {
                "ok": False,
                "http_status": 0,
                "total_s": round(time.monotonic() - t0, 3),
                "count": 0,
                "dims": 0,
                "vectors": [],
                "error": repr(e)[:300],
            }

    def tts_call(self, text, timeout=120):
        t0 = time.monotonic()
        if self.engine != "cluaiz":
            return {"ok": False, "status": "no_route", "total_s": 0.0, "wav": b"", "uri": "", "audio_s": None, "error": "no_route"}
        url = "http://127.0.0.1:8011/v1/audio/speech"
        body = {"model": CLUAIZ_MODEL_MAP["kokoro"], "input": text, "voice": "af_heart"}
        req = urllib.request.Request(url, json.dumps(body).encode(), self.headers())
        try:
            with urllib.request.urlopen(req, timeout=timeout) as resp:
                status = resp.status
                d = json.loads(resp.read())
            total_s = time.monotonic() - t0
            uri = ((d.get("output") or {}).get("audio_data") or "") if isinstance(d, dict) else ""
            wav = base64.b64decode(uri.split(",", 1)[1]) if "," in uri else b""
            secs = wav_seconds(wav)
            return {
                "ok": status == 200 and bool(secs),
                "http_status": status,
                "total_s": round(total_s, 3),
                "audio_s": round(secs, 3) if secs else None,
                "rtf": round(total_s / secs, 3) if secs else None,
                "wav": wav,
                "uri": uri,
                "error": "" if secs else "invalid_wav",
            }
        except urllib.error.HTTPError as e:
            return {
                "ok": False,
                "http_status": e.code,
                "total_s": round(time.monotonic() - t0, 3),
                "audio_s": None,
                "rtf": None,
                "wav": b"",
                "uri": "",
                "error": f"HTTP {e.code}: {e.read().decode(errors='replace')[:250]}",
            }
        except Exception as e:
            return {
                "ok": False,
                "http_status": 0,
                "total_s": round(time.monotonic() - t0, 3),
                "audio_s": None,
                "rtf": None,
                "wav": b"",
                "uri": "",
                "error": repr(e)[:250],
            }

    def stt_call(self, model_key, data_uri, expected_text, audio_s, timeout=120):
        t0 = time.monotonic()
        if self.engine != "cluaiz":
            return {"ok": False, "status": "no_route", "total_s": 0.0, "error": "no_route"}
        url = "http://127.0.0.1:8011/v1/audio/transcriptions"
        body = {"model": CLUAIZ_MODEL_MAP[model_key], "input_source": {"type": "base64", "data": data_uri}}
        req = urllib.request.Request(url, json.dumps(body).encode(), self.headers())
        try:
            with urllib.request.urlopen(req, timeout=timeout) as resp:
                status = resp.status
                d = json.loads(resp.read())
            total_s = time.monotonic() - t0
            out = ((d.get("output") or {}).get("text") or "") if isinstance(d, dict) else ""
            words = set(w.strip(".,!?").lower() for w in expected_text.split())
            got = set(w.strip(".,!?").lower() for w in out.split())
            recall = round(len(words & got) / max(1, len(words)), 3)
            return {
                "ok": status == 200 and bool(out.strip()),
                "http_status": status,
                "total_s": round(total_s, 3),
                "audio_s": audio_s,
                "rtf": round(total_s / audio_s, 3) if audio_s else None,
                "word_recall": recall,
                "text": out[:160],
                "error": "" if out.strip() else "empty_transcript",
            }
        except urllib.error.HTTPError as e:
            return {
                "ok": False,
                "http_status": e.code,
                "total_s": round(time.monotonic() - t0, 3),
                "audio_s": audio_s,
                "rtf": None,
                "word_recall": 0.0,
                "text": "",
                "error": f"HTTP {e.code}: {e.read().decode(errors='replace')[:250]}",
            }
        except Exception as e:
            return {
                "ok": False,
                "http_status": 0,
                "total_s": round(time.monotonic() - t0, 3),
                "audio_s": audio_s,
                "rtf": None,
                "word_recall": 0.0,
                "text": "",
                "error": repr(e)[:250],
            }


def normalize_vec(v):
    norm = math.sqrt(sum(x * x for x in v))
    if norm <= 0:
        return v
    return [x / norm for x in v]


def dot(a, b):
    return sum(map(operator.mul, a, b))


def run_step_workers(level, duration_s, worker_fn, max_reqs=None):
    stop_at = time.monotonic() + duration_s
    idx_lock = threading.Lock()
    next_idx = [0]
    done = []

    def loop(wid):
        while time.monotonic() < stop_at:
            with idx_lock:
                if max_reqs is not None and next_idx[0] >= max_reqs:
                    return
                idx = next_idx[0]
                next_idx[0] += 1
            res = worker_fn(wid, idx, stop_at)
            if res is None:
                return
            with idx_lock:
                done.append(res)
            if not res.get("ok"):
                time.sleep(0.25)

    threads = [threading.Thread(target=loop, args=(w,)) for w in range(level)]
    for th in threads:
        th.start()
    for th in threads:
        th.join()
    return done


def summarize_step(mode, engine, model_key, workload, level, phase_kind, rows, wall_s, oom_delta, alive):
    total = len(rows)
    ok_rows = [r for r in rows if r.get("ok")]
    failed = total - len(ok_rows)
    err_rate = round(failed / total, 4) if total else 1.0
    lat = [r["total_s"] for r in ok_rows if r.get("total_s") is not None]
    ttft = [r["ttft_s"] for r in ok_rows if r.get("ttft_s") is not None]
    tps_vals = [r["gen_tps"] for r in ok_rows if r.get("gen_tps") is not None]
    total_out = sum(r.get("out_tokens") or 0 for r in ok_rows)
    p50 = percentile(lat, 50)
    p95 = percentile(lat, 95)
    p99 = percentile(lat, 99)

    break_reason = ""
    if not alive or oom_delta > 0:
        break_reason = f"crash_or_oom (alive={alive}, oom_delta={oom_delta})"
    elif total == 0:
        break_reason = "no_requests_completed"
    elif err_rate > 0.05:
        break_reason = f"error_rate={err_rate:.2%}"
    elif p95 is not None and p95 > 60.0:
        break_reason = f"p95={p95}s > 60s"

    summary = {
        "ts": time.strftime("%Y-%m-%dT%H:%M:%S"),
        "mode": mode,
        "engine": engine,
        "model": model_key,
        "workload": workload,
        "phase_kind": phase_kind,
        "level": level,
        "wall_s": round(wall_s, 2),
        "total": total,
        "ok": len(ok_rows),
        "failed": failed,
        "error_rate": err_rate,
        "p50_s": p50,
        "p95_s": p95,
        "p99_s": p99,
        "ttft_p50_s": percentile(ttft, 50),
        "ttft_p95_s": percentile(ttft, 95),
        "agg_tps": round(total_out / wall_s, 2) if wall_s > 0 and total_out else None,
        "req_tps_mean": round(sum(tps_vals) / len(tps_vals), 2) if tps_vals else None,
        "think_leaks": sum(1 for r in rows if r.get("think_leak")),
        "oom_delta": oom_delta,
        "alive": alive,
        "passed": not bool(break_reason),
        "break_reason": break_reason,
    }
    if workload == "W1" and total:
        summary["valid_json_rate"] = round(sum(1 for r in ok_rows if r.get("valid_json")) / total, 4)
        summary["label_accuracy"] = round(sum(1 for r in ok_rows if r.get("label_correct")) / total, 4)
    return summary


def ensure_w5_clips(client, inputs_dir):
    clips_dir = inputs_dir / "w5_clips"
    clips_dir.mkdir(parents=True, exist_ok=True)
    spec = json.loads((inputs_dir / "w5_speech.json").read_text())["texts"]
    clips = []
    for item in spec:
        wav_path = clips_dir / f"{item['id']}.wav"
        if not wav_path.exists():
            if client.engine != "cluaiz":
                return []
            res = client.tts_call(item["text"])
            if not res["ok"]:
                raise RuntimeError(f"Failed to generate reference WAV for {item['id']}: {res['error']}")
            wav_path.write_bytes(res["wav"])
        raw = wav_path.read_bytes()
        secs = wav_seconds(raw)
        uri = "data:audio/wav;base64," + base64.b64encode(raw).decode()
        clips.append({"id": item["id"], "text": item["text"], "audio_s": secs, "uri": uri})
    return clips


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--mode", required=True, choices=["native", "docker"])
    ap.add_argument("--engine", required=True, choices=["cluaiz", "ollama", "llama"])
    ap.add_argument("--model-key", default="qwen3-0.6b")
    ap.add_argument("--workload", required=True, choices=["W1", "W2", "W3", "W4", "W5", "W6"])
    ap.add_argument("--inputs-dir", required=True)
    ap.add_argument("--out-dir", required=True)
    ap.add_argument("--levels", default="1,2,4,8,16,32")
    ap.add_argument("--step-secs", type=int, default=300)
    ap.add_argument("--soak-secs", type=int, default=3600)
    ap.add_argument("--threads", type=int, default=1)
    ap.add_argument("--ctx", type=int, default=4096)
    ap.add_argument("--w4-limit", type=int, default=2000)
    ap.add_argument("--phase-kind", default="ramp", choices=["ramp", "soak"])
    ap.add_argument("--cluaiz-token-file", default="/tmp/vps26_cluaiz.token")
    ap.add_argument("--check-recovery", action="store_true")
    args = ap.parse_args()

    out_dir = Path(args.out_dir)
    inputs_dir = Path(args.inputs_dir)
    samples_dir = out_dir / "samples"
    samples_dir.mkdir(parents=True, exist_ok=True)
    req_log = out_dir / f"requests_{args.mode}_{args.engine}.jsonl"
    step_log = out_dir / f"steps_{args.mode}_{args.engine}.jsonl"
    phase_file = out_dir / "phase"
    levels = [int(x.strip()) for x in args.levels.split(",") if x.strip()]

    client = EngineClient(args.engine, args.model_key, args.ctx, args.threads, args.cluaiz_token_file)
    log_lock = threading.Lock()

    def write_req(row):
        row["ts"] = time.strftime("%Y-%m-%dT%H:%M:%S")
        row["mode"] = args.mode
        row["engine"] = args.engine
        row["model"] = args.model_key
        row["workload"] = args.workload
        with log_lock:
            with open(req_log, "a") as f:
                f.write(json.dumps(row, ensure_ascii=False) + "\n")

    def write_step(srow):
        with open(step_log, "a") as f:
            f.write(json.dumps(srow) + "\n")
        print(json.dumps(srow), flush=True)

    # W5 non-Cluaiz: record honest no_route
    if args.workload == "W5" and args.engine != "cluaiz":
        write_req({"status": "no_route", "ok": False, "error": "no_route"})
        write_step({
            "ts": time.strftime("%Y-%m-%dT%H:%M:%S"),
            "mode": args.mode, "engine": args.engine, "model": "speech",
            "workload": "W5", "phase_kind": "no_route", "level": 0,
            "passed": False, "break_reason": "no_route",
        })
        return

    # W4: first run full serial indexing + search quality check, then step ramp
    if args.workload == "W4":
        data = json.loads((inputs_dir / "w4_embeddings.json").read_text())
        chunks = data["chunks"][: args.w4_limit]
        chunk_ids = {c["id"] for c in chunks}
        queries = [q for q in data["queries"] if q["target_id"] in chunk_ids][: max(20, min(len(data["queries"]), args.w4_limit // 10))]
        phase_file.write_text(f"{args.mode}_{args.engine}_W4_index")
        t0 = time.monotonic()
        oom0 = cgroup_oom_kills(out_dir)
        chunk_vecs = []
        index_ok = 0
        index_errs = []
        for i in range(0, len(chunks), 20):
            batch = [c["text"] for c in chunks[i : i + 20]]
            r = client.embed_batch(batch)
            write_req({
                "phase_kind": "index", "level": 1, "batch_idx": i // 20, "batch_size": len(batch),
                "ok": r["ok"], "http_status": r["http_status"], "total_s": r["total_s"],
                "count": r["count"], "dims": r["dims"], "error": r["error"],
            })
            if r["ok"]:
                index_ok += r["count"]
                chunk_vecs.extend([normalize_vec(v) for v in r["vectors"]])
            else:
                index_errs.append(r["error"])
        index_wall = time.monotonic() - t0

        query_vecs = []
        q_ok = 0
        t_q0 = time.monotonic()
        for i in range(0, len(queries), 20):
            batch = [q["query"] for q in queries[i : i + 20]]
            r = client.embed_batch(batch)
            write_req({
                "phase_kind": "query", "level": 1, "batch_idx": i // 20, "batch_size": len(batch),
                "ok": r["ok"], "http_status": r["http_status"], "total_s": r["total_s"],
                "count": r["count"], "dims": r["dims"], "error": r["error"],
            })
            if r["ok"]:
                q_ok += r["count"]
                query_vecs.extend([normalize_vec(v) for v in r["vectors"]])
        query_wall = time.monotonic() - t_q0

        top1 = top5 = scored = 0
        if len(chunk_vecs) == len(chunks) and len(query_vecs) == len(queries):
            for qi, q in enumerate(queries):
                qv = query_vecs[qi]
                scores = sorted(((dot(qv, chunk_vecs[ci]), chunks[ci]["id"]) for ci in range(len(chunks))), reverse=True)
                top_ids = [cid for _, cid in scores[:5]]
                if top_ids and top_ids[0] == q["target_id"]:
                    top1 += 1
                if q["target_id"] in top_ids:
                    top5 += 1
                scored += 1

        oom1 = cgroup_oom_kills(out_dir)
        search_summary = {
            "ts": time.strftime("%Y-%m-%dT%H:%M:%S"),
            "mode": args.mode, "engine": args.engine, "model": "embed-gemma",
            "workload": "W4", "phase_kind": "index_and_search", "level": 1,
            "chunks_ok": index_ok, "chunks_total": len(chunks), "index_s": round(index_wall, 2),
            "chunks_per_s": round(index_ok / max(0.01, index_wall), 2),
            "queries_scored": scored, "query_s": round(query_wall, 2),
            "top1_hit_rate": round(top1 / max(1, scored), 4) if scored else None,
            "top5_hit_rate": round(top5 / max(1, scored), 4) if scored else None,
            "oom_delta": oom1 - oom0,
            "passed": index_ok == len(chunks) and scored == len(queries),
            "errors": index_errs[:3],
        }
        write_step(search_summary)

        # Concurrent embedding ramp
        for lvl in levels:
            phase_file.write_text(f"{args.mode}_{args.engine}_W4_c{lvl}")
            oom_a = cgroup_oom_kills(out_dir)
            tw0 = time.monotonic()

            def w4_worker(wid, idx, stop_at):
                batch = [chunks[(idx * 20 + k) % len(chunks)]["text"] for k in range(20)]
                r = client.embed_batch(batch)
                row = {
                    "phase_kind": "ramp", "level": lvl, "req_idx": idx,
                    "ok": r["ok"], "http_status": r["http_status"], "total_s": r["total_s"],
                    "count": r["count"], "dims": r["dims"], "error": r["error"],
                }
                write_req(row)
                return row

            rows = run_step_workers(lvl, args.step_secs, w4_worker)
            wall = time.monotonic() - tw0
            oom_b = cgroup_oom_kills(out_dir)
            srow = summarize_step(args.mode, args.engine, "embed-gemma", "W4", lvl, "ramp", rows, wall, oom_b - oom_a, port_alive(client.embed_port()))
            total_chunks = sum(r.get("count") or 0 for r in rows if r.get("ok"))
            srow["chunks_per_s"] = round(total_chunks / max(0.01, wall), 2)
            write_step(srow)
            if not srow["passed"]:
                break
        return

    # W5: speech ramp (Cluaiz only)
    if args.workload == "W5":
        clips = ensure_w5_clips(client, inputs_dir)
        texts = json.loads((inputs_dir / "w5_speech.json").read_text())["texts"]
        for lvl in levels:
            phase_file.write_text(f"{args.mode}_{args.engine}_W5_c{lvl}")
            oom_a = cgroup_oom_kills(out_dir)
            tw0 = time.monotonic()

            def w5_worker(wid, idx, stop_at):
                if idx % 2 == 0:
                    item = texts[(idx // 2) % len(texts)]
                    r = client.tts_call(item["text"])
                    row = {
                        "phase_kind": "tts", "level": lvl, "req_idx": idx, "clip": item["id"],
                        "ok": r["ok"], "http_status": r.get("http_status", 0), "total_s": r["total_s"],
                        "audio_s": r.get("audio_s"), "rtf": r.get("rtf"), "error": r["error"],
                    }
                else:
                    c = clips[(idx // 2) % len(clips)]
                    mkey = "moonshine-tiny" if (idx // 2) % 2 == 0 else "moonshine-base"
                    r = client.stt_call(mkey, c["uri"], c["text"], c["audio_s"])
                    row = {
                        "phase_kind": f"stt_{mkey}", "level": lvl, "req_idx": idx, "clip": c["id"],
                        "ok": r["ok"], "http_status": r.get("http_status", 0), "total_s": r["total_s"],
                        "audio_s": r.get("audio_s"), "rtf": r.get("rtf"),
                        "word_recall": r.get("word_recall"), "error": r["error"],
                    }
                write_req(row)
                return row

            rows = run_step_workers(lvl, args.step_secs, w5_worker)
            wall = time.monotonic() - tw0
            oom_b = cgroup_oom_kills(out_dir)
            srow = summarize_step(args.mode, args.engine, "speech", "W5", lvl, "ramp", rows, wall, oom_b - oom_a, port_alive(8011))
            write_step(srow)
            if not srow["passed"]:
                break
        return

    # W6: everything at once
    if args.workload == "W6":
        tickets = json.loads((inputs_dir / "w1_tickets.json").read_text())
        chunks = json.loads((inputs_dir / "w4_embeddings.json").read_text())["chunks"]
        clips = ensure_w5_clips(client, inputs_dir) if args.engine == "cluaiz" else []
        texts = json.loads((inputs_dir / "w5_speech.json").read_text())["texts"]

        def run_w6_phase(phase_kind, lvl, duration_s):
            phase_file.write_text(f"{args.mode}_{args.engine}_W6_{phase_kind}_c{lvl}")
            oom_a = cgroup_oom_kills(out_dir)
            tw0 = time.monotonic()
            stop_at = time.monotonic() + duration_s
            rows = []
            rlock = threading.Lock()

            def record(r):
                write_req(r)
                with rlock:
                    rows.append(r)

            def chat_worker(wid):
                i = wid
                while time.monotonic() < stop_at:
                    tk = tickets[i % len(tickets)]
                    i += 1
                    prompt = (
                        "Classify this support ticket into one label from: billing, bug, account, feature, outage. "
                        "Return ONLY valid JSON with key \"label\".\n\nTicket: " + tk["text"]
                    )
                    r = client.chat_stream([{"role": "user", "content": prompt}], max_tokens=32, json_mode=True)
                    vjson, lbl = parse_ticket_json(r["text"]) if r["ok"] else (False, "")
                    record({
                        "phase_kind": phase_kind, "subtask": "W1_chat", "level": lvl,
                        "ok": r["ok"], "http_status": r["http_status"], "ttft_s": r["ttft_s"],
                        "total_s": r["total_s"], "in_tokens": r["in_tokens"], "out_tokens": r["out_tokens"],
                        "gen_tps": r["gen_tps"], "think_leak": r["think_leak"],
                        "valid_json": vjson, "label_correct": lbl == tk["label"], "error": r["error"],
                    })

            def embed_worker(wid):
                i = wid * 10
                while time.monotonic() < stop_at:
                    batch = [chunks[(i + k) % len(chunks)]["text"] for k in range(10)]
                    i += 10
                    r = client.embed_batch(batch)
                    record({
                        "phase_kind": phase_kind, "subtask": "W4_embed", "level": lvl,
                        "ok": r["ok"], "http_status": r["http_status"], "total_s": r["total_s"],
                        "count": r["count"], "error": r["error"],
                    })

            def speech_worker(wid):
                i = wid
                while time.monotonic() < stop_at:
                    if i % 2 == 0:
                        item = texts[(i // 2) % len(texts)]
                        r = client.tts_call(item["text"])
                        record({
                            "phase_kind": phase_kind, "subtask": "W5_tts", "level": lvl,
                            "ok": r["ok"], "http_status": r.get("http_status", 0), "total_s": r["total_s"],
                            "rtf": r.get("rtf"), "error": r["error"],
                        })
                    else:
                        c = clips[(i // 2) % len(clips)]
                        r = client.stt_call("moonshine-tiny", c["uri"], c["text"], c["audio_s"])
                        record({
                            "phase_kind": phase_kind, "subtask": "W5_stt", "level": lvl,
                            "ok": r["ok"], "http_status": r.get("http_status", 0), "total_s": r["total_s"],
                            "rtf": r.get("rtf"), "word_recall": r.get("word_recall"), "error": r["error"],
                        })
                    i += 1

            ths = [threading.Thread(target=chat_worker, args=(k,)) for k in range(lvl)]
            ths.append(threading.Thread(target=embed_worker, args=(0,)))
            if args.engine == "cluaiz":
                ths.append(threading.Thread(target=speech_worker, args=(0,)))
            else:
                write_req({"phase_kind": phase_kind, "subtask": "W5_speech", "status": "no_route", "ok": False, "error": "no_route"})
            for th in ths:
                th.start()
            for th in ths:
                th.join()

            wall = time.monotonic() - tw0
            oom_b = cgroup_oom_kills(out_dir)
            alive = port_alive(client.chat_port()) and (args.engine != "llama" or port_alive(client.embed_port()))
            srow = summarize_step(args.mode, args.engine, args.model_key, "W6", lvl, phase_kind, rows, wall, oom_b - oom_a, alive)
            write_step(srow)
            return srow

        if args.phase_kind == "soak":
            run_w6_phase("soak", levels[0], args.step_secs)
            return

        last_good = None
        for lvl in levels:
            sr = run_w6_phase("ramp", lvl, args.step_secs)
            if sr["passed"]:
                last_good = lvl
            else:
                break
        if last_good is not None and args.soak_secs > 0:
            run_w6_phase("soak", last_good, args.soak_secs)
        return

    # Chat workloads: W1, W2, W3
    if args.workload == "W1":
        tickets = json.loads((inputs_dir / "w1_tickets.json").read_text())
    elif args.workload == "W2":
        reports = json.loads((inputs_dir / "w2_reports.json").read_text())
    elif args.workload == "W3":
        chats = json.loads((inputs_dir / "w3_chats.json").read_text())

    sample_saved = [0]

    def run_chat_phase(phase_kind, lvl, duration_s):
        phase_file.write_text(f"{args.mode}_{args.engine}_{args.workload}_{args.model_key}_{phase_kind}_c{lvl}")
        oom_a = cgroup_oom_kills(out_dir)
        tw0 = time.monotonic()

        def chat_worker(wid, idx, stop_at):
            if args.workload == "W1":
                tk = tickets[idx % len(tickets)]
                prompt = (
                    "Classify this support ticket into one label from: billing, bug, account, feature, outage. "
                    "Return ONLY valid JSON with key \"label\".\n\nTicket: " + tk["text"]
                )
                r = client.chat_stream([{"role": "user", "content": prompt}], max_tokens=32, json_mode=True, timeout=120)
                vjson, lbl = parse_ticket_json(r["text"]) if r["ok"] else (False, "")
                row = {
                    "phase_kind": phase_kind, "level": lvl, "req_idx": idx, "ticket_id": tk["id"],
                    "ok": r["ok"], "http_status": r["http_status"], "ttft_s": r["ttft_s"],
                    "total_s": r["total_s"], "in_tokens": r["in_tokens"], "out_tokens": r["out_tokens"],
                    "chars": len(r["text"]),
                    "gen_tps": r["gen_tps"], "think_leak": r["think_leak"],
                    "valid_json": vjson, "parsed_label": lbl, "expected_label": tk["label"],
                    "label_correct": lbl == tk["label"], "error": r["error"],
                }
                write_req(row)
                return row

            if args.workload == "W2":
                rep = reports[idx % len(reports)]
                prompt = (
                    "Summarize the following incident report in exactly 5 short lines covering: "
                    "service, symptoms, peak resource impact, root cause clues, and mitigations.\n\n" + rep["text"]
                )
                r = client.chat_stream([{"role": "user", "content": prompt}], max_tokens=220, timeout=240)
                if r["ok"]:
                    with log_lock:
                        if sample_saved[0] < 3:
                            sample_saved[0] += 1
                            sf = samples_dir / f"w2_{args.mode}_{args.engine}_{args.model_key}.txt"
                            with open(sf, "a") as f:
                                f.write(f"=== Report #{rep['id']} ===\n{r['text']}\n\n")
                row = {
                    "phase_kind": phase_kind, "level": lvl, "req_idx": idx, "report_id": rep["id"],
                    "ok": r["ok"], "http_status": r["http_status"], "ttft_s": r["ttft_s"],
                    "total_s": r["total_s"], "in_tokens": r["in_tokens"], "out_tokens": r["out_tokens"],
                    "chars": len(r["text"]),
                    "gen_tps": r["gen_tps"], "think_leak": r["think_leak"], "error": r["error"],
                }
                write_req(row)
                return row

            # W3: 5-turn session
            sess = chats[idx % len(chats)]
            msgs = []
            turn_rows = []
            transcript = []
            for ti, user_msg in enumerate(sess["turns"]):
                if ti > 0 and time.monotonic() >= stop_at:
                    break
                msgs.append({"role": "user", "content": user_msg})
                r = client.chat_stream(msgs, max_tokens=96, timeout=180)
                trow = {
                    "phase_kind": phase_kind, "level": lvl, "req_idx": idx,
                    "session_id": sess["id"], "turn_idx": ti + 1,
                    "ok": r["ok"], "http_status": r["http_status"], "ttft_s": r["ttft_s"],
                    "total_s": r["total_s"], "in_tokens": r["in_tokens"], "out_tokens": r["out_tokens"],
                    "chars": len(r["text"]),
                    "gen_tps": r["gen_tps"], "think_leak": r["think_leak"], "error": r["error"],
                }
                write_req(trow)
                turn_rows.append(trow)
                if not r["ok"]:
                    break
                msgs.append({"role": "assistant", "content": r["text"]})
                transcript.append(f"U{ti+1}: {user_msg}\nA{ti+1}: {r['text']}\n")
            if len(turn_rows) == 5 and all(t["ok"] for t in turn_rows):
                with log_lock:
                    if sample_saved[0] < 1:
                        sample_saved[0] += 1
                        sf = samples_dir / f"w3_{args.mode}_{args.engine}_{args.model_key}.txt"
                        sf.write_text("\n".join(transcript))
            # Return aggregate session row or last turn row for step health
            ok_s = bool(turn_rows) and all(t["ok"] for t in turn_rows)
            return {
                "ok": ok_s,
                "ttft_s": turn_rows[0]["ttft_s"] if turn_rows else None,
                "total_s": round(sum(t["total_s"] for t in turn_rows), 3),
                "out_tokens": sum(t.get("out_tokens") or 0 for t in turn_rows),
                "gen_tps": turn_rows[-1].get("gen_tps") if turn_rows else None,
                "think_leak": any(t.get("think_leak") for t in turn_rows),
            }

        rows = run_step_workers(lvl, duration_s, chat_worker)
        wall = time.monotonic() - tw0
        oom_b = cgroup_oom_kills(out_dir)
        srow = summarize_step(args.mode, args.engine, args.model_key, args.workload, lvl, phase_kind, rows, wall, oom_b - oom_a, port_alive(client.chat_port()))
        write_step(srow)
        return srow

    if args.phase_kind == "soak":
        run_chat_phase("soak", levels[0], args.step_secs)
        return

    last_good = None
    for lvl in levels:
        sr = run_chat_phase("ramp", lvl, args.step_secs)
        if sr["passed"]:
            last_good = lvl
        else:
            break

    if last_good is not None and args.soak_secs > 0:
        run_chat_phase("soak", last_good, args.soak_secs)

    if args.check_recovery and last_good is not None:
        phase_file.write_text(f"{args.mode}_{args.engine}_{args.workload}_recovery")
        port = client.chat_port()
        results = [None] * 4

        def job(k):
            results[k] = client.chat_stream(
                [{"role": "user", "content": f"Write two short sentences about service uptime #{k}."}],
                max_tokens=48,
                timeout=30,
            )

        ths = [threading.Thread(target=job, args=(k,)) for k in range(4)]
        for th in ths:
            th.start()
        time.sleep(1.5)
        # Kill listener PID on port with SIGKILL to simulate sudden engine crash
        try:
            out_ss = subprocess.check_output(["ss", "-ltnpH", f"sport = :{port}"], text=True, stderr=subprocess.DEVNULL)
        except Exception:
            out_ss = ""
        killed_pids = []
        for tok in out_ss.split("pid=")[1:]:
            pid_s = "".join(ch for ch in tok if ch.isdigit())
            if pid_s:
                pid_i = int(pid_s)
                if pid_i not in killed_pids:
                    killed_pids.append(pid_i)
                    try:
                        os.kill(pid_i, signal.SIGKILL)
                    except OSError:
                        pass
        for th in ths:
            th.join()
        time.sleep(5.0)
        auto_up = port_alive(port)
        rec = {
            "ts": time.strftime("%Y-%m-%dT%H:%M:%S"),
            "mode": args.mode, "engine": args.engine, "model": args.model_key,
            "killed_pids": killed_pids,
            "auto_restarted_after_5s": auto_up,
            "in_flight_ok": sum(1 for r in results if r and r.get("ok")),
            "in_flight_lost": sum(1 for r in results if not r or not r.get("ok")),
        }
        with open(out_dir / f"recovery_{args.mode}_{args.engine}.jsonl", "a") as f:
            f.write(json.dumps(rec) + "\n")
        print("RECOVERY:", json.dumps(rec), flush=True)


if __name__ == "__main__":
    main()
