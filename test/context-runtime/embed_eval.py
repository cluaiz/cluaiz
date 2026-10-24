"""Optional embeddings (P5.3, step 1): do installed embedding models help the word matcher?

Two checks, each against what the Engine does today without vectors:

1. Tools: for every routing-set row that expects a tool, is the expected tool the nearest one
   by cosine (message vs the tool's name + description)? Compared with the word matcher's hit
   (POST /v1/tools/discover). Reported by language (English, Hinglish, Devanagari) and for the
   paraphrase rows, plus how close trap and chit-chat rows get to any tool.
2. Memory: in a 200-message synthetic chat (memory_suite.py), is the planted message in the top 3
   by cosine for each question? Compared with BM25 recall (POST /v1/recall/search).

    python embed_eval.py --url http://127.0.0.1:8011 --token <token> --models embeddinggemma-2-GGUF,bge-m3-GGUF

Uses the Engine's /v1/embeddings with models the user installed; nothing is bundled.
"""
import argparse
import json
import math
import os
import sys
import time
import urllib.error
import urllib.request
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
import memory_suite  # noqa: E402


class Client:
    def __init__(self, url, token):
        self.url = url.rstrip("/")
        self.headers = {"Authorization": "Bearer " + token, "Content-Type": "application/json"}

    def call(self, path, body=None, method=None):
        request = urllib.request.Request(self.url + path, data=json.dumps(body).encode() if body is not None else None,
                                         headers=self.headers, method=method)
        return json.load(urllib.request.urlopen(request, timeout=900))

    def embed(self, model, texts, batch=32):
        out = []
        for i in range(0, len(texts), batch):
            data = self.call("/v1/embeddings", {"model": model, "input": texts[i:i + batch]})["data"]
            out.extend(d["embedding"] for d in sorted(data, key=lambda d: d["index"]))
        return out


def prompts(model, kind, texts):
    # EmbeddingGemma is trained with task prefixes; bge-m3 takes plain text
    if "gemma" in model.lower():
        prefix = "task: search result | query: " if kind == "query" else "title: none | text: "
        return [prefix + t for t in texts]
    return texts


def cosine(a, b):
    dot = sum(x * y for x, y in zip(a, b))
    na = math.sqrt(sum(x * x for x in a)) or 1.0
    nb = math.sqrt(sum(y * y for y in b)) or 1.0
    return dot / (na * nb)


def tool_check(client, model, rows, tools):
    names = [t["id"] for t in tools]
    docs = client.embed(model, prompts(model, "doc", [f"{t['id']}: {t.get('description') or ''}" for t in tools]))
    queries = client.embed(model, prompts(model, "query", [r["query"] for r in rows]))
    results = []
    for row, q in zip(rows, queries):
        sims = sorted(((cosine(q, d), n) for d, n in zip(docs, names)), reverse=True)
        results.append({"row": row, "top": sims[0][1], "top_sim": sims[0][0], "ranked": [n for _, n in sims]})
    return results


def main():
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--url", default="http://127.0.0.1:8011")
    parser.add_argument("--token", default=os.environ.get("CLUAIZ_TOKEN"))
    parser.add_argument("--models", default="embeddinggemma-2-GGUF,bge-m3-GGUF")
    args = parser.parse_args()
    client = Client(args.url, args.token)
    rows = [json.loads(l) for l in open(HERE / "routing_set.jsonl", encoding="utf-8") if l.strip()]
    tools = [t for t in client.call("/v1/tools")["tools"] if t.get("enabled") and t.get("category") in ("skill", "plugin", "mcp")]
    positive = [r for r in rows if r["expect"]]
    negative = [r for r in rows if not r["expect"]]

    # The word matcher, for comparison
    matcher_hit = {}
    for r in positive:
        found = client.call("/v1/tools/discover", {"query": r["query"], "previous": r.get("previous")})["candidates"]
        matcher_hit[r["id"]] = all(e in [c["id"] for c in found] for e in r["expect"])

    # A 200-message chat for the memory check
    chat = f"embed-eval-{int(time.time())}"
    history = memory_suite.build_chat(200, seed=200)
    client.call("/v1/chat/completions", {"stream": False, "max_tokens": 1, "think_mode": "off", "tool_search": False, "session_id": chat,
                                         "history_mode": "none", "messages": history + [{"role": "user", "content": "ok"}],
                                         **({"model": os.environ["CLUAIZ_MODEL"]} if os.environ.get("CLUAIZ_MODEL") else {})})
    questions = [(q, text) for _, text, q, _ in memory_suite.FACTS]
    questions.append((memory_suite.CORRECTION[4], memory_suite.CORRECTION[3]))
    bm25_top3 = 0
    for question, planted in questions:
        found = client.call("/v1/recall/search", {"chat_id": chat, "query": question, "limit": 3})["results"]
        texts = [client.call(f"/v1/recall/item/{r['ref']}?chat_id={chat}")["text"] for r in found]
        bm25_top3 += any(planted in t for t in texts)

    print(f"Word matcher: tool found {sum(matcher_hit.values())}/{len(positive)}; memory planted message in top 3: {bm25_top3}/{len(questions)}")
    for model in args.models.split(","):
        started = time.time()
        try:
            results = tool_check(client, model, positive + negative, tools)
        except urllib.error.HTTPError as e:
            print(f"{model}: {e.code} {e.read()[:200]!r}")
            continue
        by_id = {r["row"]["id"]: r for r in results}
        top1 = lambda part: sum(by_id[r["id"]]["top"] in r["expect"] for r in part)
        lines = []
        for label, part in [("all", positive), ("en", [r for r in positive if r["lang"] == "en"]),
                            ("hi-latn", [r for r in positive if r["lang"] == "hi-latn"]), ("hi-deva", [r for r in positive if r["lang"] == "hi-deva"]),
                            ("paraphrase", [r for r in positive if r["kind"] == "paraphrase"])]:
            matcher = sum(matcher_hit[r["id"]] for r in part)
            lines.append(f"{label} {top1(part)}/{len(part)} (matcher {matcher})")
        pos_sims = sorted(by_id[r["id"]]["top_sim"] for r in positive)
        neg_sims = sorted(by_id[r["id"]]["top_sim"] for r in negative)
        median = lambda v: v[len(v) // 2] if v else 0
        # Memory: planted message in the top 3 by cosine
        log = client.call(f"/v1/chat/{chat}/events?limit=5000")["events"]
        messages = [(e["pos"], e["text"]) for e in log if e["kind"] == "message" and e.get("source") == "client"]
        mvecs = client.embed(model, prompts(model, "doc", [t for _, t in messages]))
        qvecs = client.embed(model, prompts(model, "query", [q for q, _ in questions]))
        mem_top3 = 0
        for (question, planted), qv in zip(questions, qvecs):
            ranked = sorted(((cosine(qv, mv), t) for mv, (_, t) in zip(mvecs, messages)), reverse=True)[:3]
            mem_top3 += any(planted in t for _, t in ranked)
        print(f"{model} ({time.time() - started:.0f}s): tool top-1 " + " | ".join(lines))
        print(f"    similarity of the best tool: median {median(pos_sims):.3f} when a tool fits, {median(neg_sims):.3f} for traps and chit-chat"
              f" (highest {neg_sims[-1]:.3f}) | memory top 3: {mem_top3}/{len(questions)}")
    client.call(f"/v1/chat/{chat}/history", method="DELETE")


if __name__ == "__main__":
    main()
