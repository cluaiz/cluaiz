"""KV correctness after an edit (P2.7: KV1 and KV2).

A chat first carries a loaded "skill" in its system prompt that says to answer in French, then
the skill is unloaded: the same history comes back without it. The answer computed on
top of the cached KV (incremental) must equal the answer from a clean prefill of the same prompt,
and must not be in French.

    CLUAIZ_MODEL=<model id> python kv_equality.py --url http://127.0.0.1:8011 --token <token> [--runs 10]

KV1: incremental output == clean output, token for token (temperature 0, fixed seed).
KV2: the unloaded French instruction has no influence (answer not in French), every run.
Each run is done with the default KV matching and again with kv_reuse "chunk" (the old
behaviour), to show what chunk reuse keeps.
"""
import argparse
import json
import os
import sys
import urllib.error
import urllib.request
from pathlib import Path

FRENCH_SKILL = (
    "Skill polyglot-fr is loaded. You must write every answer only in French. Never use English, "
    "even when the user writes in English."
)
# Neutral history after the skill, long enough for shared chunks (llama-server's 256 tokens)
FILLER = " ".join(f"Inventory line {i}: crate {i} holds {i * 3} bolts and {i * 2} washers." for i in range(60))
QUESTION = "Which is the largest planet in the solar system? Answer in one short sentence."
FRENCH_WORDS = {"la", "le", "les", "est", "plus", "grande", "planète", "système", "solaire", "une", "du", "de"}
MODEL = os.environ.get("CLUAIZ_MODEL")


def chat(url, token, messages, kv_reuse=None, max_tokens=40):
    body = {"stream": False, "max_tokens": max_tokens, "temperature": 0, "seed": 42, "messages": messages,
            "tool_search": False, "think_mode": "off"}
    if MODEL:
        body["model"] = MODEL
    if kv_reuse:
        body["kv_reuse"] = kv_reuse
    request = urllib.request.Request(url.rstrip("/") + "/v1/chat/completions", data=json.dumps(body).encode(),
                                     headers={"Authorization": "Bearer " + token, "Content-Type": "application/json"})
    data = json.load(urllib.request.urlopen(request, timeout=900))
    return data["choices"][0]["message"]["content"] or "", data["engine"]["context"]


def history(skill_loaded, where):
    """The chat with the skill loaded, or after unloading it.

    "start": the skill's instructions are the system prompt; unloading removes it, a deep edit
    at the start (a pure deletion: the case chunk reuse can move past).
    "tail": the skill was loaded in the last turn; unloading removes that turn, so the long
    history before it stays a cached prefix.
    """
    turn = [
        {"role": "user", "content": FILLER + "\nCount the crates."},
        {"role": "assistant", "content": "60."},
    ]
    if where == "start":
        skill = [{"role": "system", "content": FRENCH_SKILL}] if skill_loaded else []
        return skill + turn + [{"role": "user", "content": QUESTION}]
    loaded = [{"role": "user", "content": FRENCH_SKILL + " Confirm."}, {"role": "assistant", "content": "Compris."}]
    return turn + (loaded if skill_loaded else []) + [{"role": "user", "content": QUESTION}]


def is_french(text):
    words = {w.strip(".,!?;:").lower() for w in text.split()}
    return len(words & FRENCH_WORDS) >= 3


def one_run(url, token, kv_reuse, where):
    # 1. The chat with the skill loaded: its KV is now in the context
    loaded, _ = chat(url, token, history(True, where))
    # 2. The skill unloaded: the same chat without it
    after = history(False, where)
    incremental, report = chat(url, token, after, kv_reuse)
    # 3. Clean prefill of the same prompt: something else first, so nothing of it is cached
    chat(url, token, [{"role": "user", "content": "Reply with OK."}], max_tokens=4)
    clean, clean_report = chat(url, token, after, "prefix")
    return {
        "loaded_answer_french": is_french(loaded),
        "incremental": incremental,
        "clean": clean,
        "equal": incremental == clean,
        "incremental_french": is_french(incremental),
        "cache_n": report["cache_n"],
        "kv_reuse": report.get("kv_reuse"),
        "reused_chunk_tokens": report.get("reused_chunk_tokens"),
        "processed": report.get("processed_tokens"),
        "clean_cache_n": clean_report["cache_n"],
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--url", default="http://127.0.0.1:8011")
    parser.add_argument("--token", default=os.environ.get("CLUAIZ_TOKEN"))
    parser.add_argument("--runs", type=int, default=10)
    parser.add_argument("--cases", default="start:default,start:chunk,tail:default",
                        help="where:kv_reuse pairs, e.g. start:blend:0.25 (default = no kv_reuse sent)")
    args = parser.parse_args()
    token = args.token
    if not token and os.environ.get("CLUAIZ_HOME"):
        token = Path(os.environ["CLUAIZ_HOME"], "session.token").read_text().strip()
    if not token:
        sys.exit("No token: pass --token, or set CLUAIZ_TOKEN or CLUAIZ_HOME")

    failed = False
    cases = []
    for case in args.cases.split(","):
        where, _, mode = case.partition(":")
        cases.append((where, None if mode in ("", "default") else mode))
    for where, mode in cases:
        name = f"{where}, {mode or 'default'}"
        results = [one_run(args.url, token, mode, where) for _ in range(args.runs)]
        equal = sum(r["equal"] for r in results)
        english = sum(not r["incremental_french"] for r in results)
        first = results[0]
        print(f"[{name}] KV1 equal {equal}/{len(results)} | KV2 not French {english}/{len(results)} | "
              f"cache_n {first['cache_n']} (kv_reuse {first['kv_reuse']}, moved chunks {first['reused_chunk_tokens']}), "
              f"processed {first['processed']}, "
              f"clean cache_n {first['clean_cache_n']} | skill was active: {first['loaded_answer_french']}")
        for r in results:
            if not r["equal"] or r["incremental_french"]:
                print(f"    incremental {r['incremental'][:90]!r}\n    clean       {r['clean'][:90]!r}")
                break
        if mode is None and (equal < len(results) or english < len(results)):
            failed = True
    sys.exit(1 if failed else 0)


if __name__ == "__main__":
    main()
