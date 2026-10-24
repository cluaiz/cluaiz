"""Tool-call format suite: does a model call the bound tools well-formed, and is the answer right?

Every row binds tools by id (`tools`), turns the tool suggestions off, approves every permission
request and records from the stream: the calls the Engine ran, the answer text, and whether the
answer still holds tool-call markup (a call the Engine did not run: malformed or in a format it
does not read). Rows with `expect_calls: []` must call nothing.

    python test/context-runtime/tool_calls.py --url http://127.0.0.1:8011 --token <token> \
        --model <model id> [--runs 1] [--out results.jsonl]

The rows assume the isolated test home: the MCP servers `filesystem`, `pynotes` and `everything`,
the `code-interpreter` skill, and a workspace with notes.txt (MANGO-42) and calc.py (print(6 * 7)).
"""
import argparse, json, re, sys, time, urllib.request, uuid

TOOLS = ["filesystem", "pynotes", "everything", "code-interpreter"]
ROWS = [
    {"id": "fs-read", "text": "Read notes.txt and tell me the secret word.", "expect_calls": ["filesystem .*read"], "answer": ["MANGO-42"]},
    {"id": "fs-read-hi", "text": "notes.txt padho aur secret word batao", "expect_calls": ["filesystem .*read"], "answer": ["MANGO-42"]},
    {"id": "fs-list", "text": "Which files are in the project folder? List them.", "expect_calls": ["filesystem .*(list|director|tree|search)"], "answer": ["notes.txt", "calc.py"]},
    {"id": "fs-line", "text": "What is written in calc.py?", "expect_calls": ["filesystem .*read"], "answer": ["6 * 7"]},
    {"id": "note-secret", "text": "Read the note named secret from pynotes.", "expect_calls": ["read_note"], "answer": ["KIWI-7"]},
    {"id": "note-groceries", "text": "groceries wala note padho", "expect_calls": ["read_note"], "answer": ["milk"]},
    {"id": "note-meeting", "text": "What time is the meeting? Check the meeting note.", "expect_calls": ["read_note"], "answer": ["10:00"]},
    {"id": "echo", "text": "Use the echo tool to echo the text: hello cluaiz", "expect_calls": ["echo"], "answer": ["hello cluaiz"]},
    {"id": "add", "text": "Use the add tool to add 17 and 25.", "expect_calls": ["get-sum|add"], "answer": ["42"]},
    {"id": "py-run", "text": "Run this python and tell me the output: print(2+2)", "expect_calls": ["code-interpreter|run|exec|python"], "answer": ["4"]},
    {"id": "two-step", "text": "First read notes.txt, then read the pynotes note named secret. Tell me both secrets.", "expect_calls": ["filesystem .*read", "read_note"], "answer": ["MANGO-42", "KIWI-7"]},
    {"id": "chat-hi", "text": "Hi! How are you today?", "expect_calls": [], "answer": []},
    {"id": "chat-fact", "text": "What is the capital of France?", "expect_calls": [], "answer": ["Paris"]},
    {"id": "chat-explain", "text": "Explain in one sentence what a file system is.", "expect_calls": [], "answer": []},
]
# Tool-call markup left in an answer: Hermes, Llama 3.x, Mistral, Gemma, Functionary, generic JSON
MARKUP = re.compile(r"<\s*/?\s*tool_call|\[TOOL_CALLS\]|<\|python_tag\|>|<\|tool_call|tool_call\|>|<start_function_call>|<function[=>]|\"arguments\"\s*:|\{\s*\"name\"\s*:\s*\"")


def post(url, token, body, stream=False, timeout=900):
    req = urllib.request.Request(url, data=json.dumps(body).encode(), headers={"Authorization": "Bearer " + token, "Content-Type": "application/json"})
    return urllib.request.urlopen(req, timeout=timeout)


def calls_ok(row, calls):
    """Every expected call (a regex over "tool function") is made, in any order; none when none"""
    names = [f"{c['tool']} {c['name']}".lower() for c in calls]
    expected = row["expect_calls"]
    return all(any(re.search(p, n) for n in names) for p in expected) if expected else not calls


def summary(model, results):
    n = len(results)
    tool_rows = [r for r in results if any(x["id"] == r["id"] and x["expect_calls"] for x in ROWS)]
    return (f"{model}: pass {sum(r['pass'] for r in results)}/{n} | tool rows called right "
            f"{sum(r['called_ok'] for r in tool_rows)}/{len(tool_rows)} | markup left {sum(r['markup'] for r in results)} | "
            f"right answer {sum(r['answer_ok'] for r in results)}/{n} | errors {sum(bool(r['error']) for r in results)}")


def rescore(path):
    """Scores a results file again with the current rows (the calls are stored; answers are not
    checked again, only their last 300 characters are kept)"""
    by_model = {}
    for line in open(path, encoding="utf-8"):
        res = json.loads(line)
        row = next(r for r in ROWS if r["id"] == res["id"])
        res["called_ok"] = calls_ok(row, res["calls"])
        res["pass"] = res["called_ok"] and res["answer_ok"] and not res["markup"] and not res["error"]
        by_model.setdefault(res["model"], []).append(res)
    for model, results in by_model.items():
        print(summary(model, results))


def run_row(args, row):
    body = {"model": args.model, "session_id": f"tc-{row['id']}-{uuid.uuid4().hex[:8]}", "stream": True,
            "messages": [{"role": "user", "content": row["text"]}], "max_tokens": args.max_tokens,
            "tools": TOOLS, "tool_search": False, "temperature": args.temperature, "seed": 42, "storage": "none"}
    if args.extra:
        body.update(json.loads(args.extra))
    calls, text, error, t0 = [], [], None, time.time()
    try:
        with post(args.url + "/v1/chat/completions", args.token, body) as r:
            for raw in r:
                line = raw.decode("utf-8", "replace").strip()
                if not line.startswith("data:") or line[5:].strip() == "[DONE]":
                    continue
                try:
                    ev = json.loads(line[5:])
                except Exception:
                    continue
                if ev.get("error"):
                    error = str(ev["error"])[:200]
                for ch in ev.get("choices", []):
                    d = ch.get("delta", {}) or {}
                    if d.get("content"):
                        text.append(d["content"])
                    pr = d.get("permission_request")
                    if pr:
                        post(args.url + "/v1/system/permission/approve", args.token, {"request_id": pr["request_id"]}, timeout=60).read()
                    for tc in d.get("tool_calls") or []:
                        fn = tc.get("function", {})
                        # MCP servers report the server as the name and the function as sub_function
                        calls.append({"name": fn.get("sub_function") or fn.get("name", ""), "tool": fn.get("name", ""),
                                      "arguments": fn.get("arguments", "")})
    except Exception as e:
        error = f"{type(e).__name__}: {e}"[:200]
    answer = "".join(text)
    called_ok = calls_ok(row, calls)
    answer_ok = all(a.lower() in answer.lower() for a in row["answer"])
    markup = bool(MARKUP.search(answer))
    return {"id": row["id"], "calls": calls, "called_ok": called_ok, "answer_ok": answer_ok, "markup": markup,
            "pass": called_ok and answer_ok and not markup and not error, "error": error,
            "seconds": round(time.time() - t0, 1), "answer": answer[-300:]}


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--url", default="http://127.0.0.1:8011")
    ap.add_argument("--token")
    ap.add_argument("--model")
    ap.add_argument("--rescore", help="a results file (--out) to score again")
    ap.add_argument("--runs", type=int, default=1)
    ap.add_argument("--temperature", type=float, default=0.0)
    ap.add_argument("--max-tokens", type=int, default=1500)
    ap.add_argument("--extra", help="JSON merged into every request")
    ap.add_argument("--only", help="comma-separated row ids")
    ap.add_argument("--out")
    args = ap.parse_args()
    if args.rescore:
        return rescore(args.rescore)
    rows = [r for r in ROWS if not args.only or r["id"] in args.only.split(",")]
    results = []
    out = open(args.out, "a", encoding="utf-8") if args.out else None
    for run in range(args.runs):
        for row in rows:
            res = run_row(args, row)
            res.update(model=args.model, run=run)
            results.append(res)
            flag = "PASS" if res["pass"] else "FAIL"
            print(f"{flag} {row['id']:<14} calls={[c['name'] for c in res['calls']]} call_ok={res['called_ok']} answer_ok={res['answer_ok']} "
                  f"markup={res['markup']} {res['seconds']}s {('ERR ' + res['error']) if res['error'] else ''}", flush=True)
            if not res["pass"]:
                print("     answer:", res["answer"].replace("\n", " ")[-200:], flush=True)
            if out:
                out.write(json.dumps(res, ensure_ascii=False) + "\n")
                out.flush()
    print("\n" + summary(args.model, results))


if __name__ == "__main__":
    sys.exit(main())
