"""Long-chat memory suite (P3.10): history_mode "recent" against "adaptive" (with recall).

Builds synthetic chats of several sizes from filler turns, plants facts at several depths (and a
corrected fact and an absent one), then asks about them with each history mode. Every question
runs in its own chat (session id), sent the way the App sends it: the whole history plus the
question. The Engine logs the history on the first request and recalls from it.

    CLUAIZ_MODEL=<model id> python memory_suite.py --url http://127.0.0.1:8011 --token <token> \\
        [--sizes 200,1000,2000] [--n-ctx 4096]

Prints a table: correct answers and average prompt tokens per mode and size. Time questions are
not in this version: messages sent by a client all get the time they were logged.
"""
import argparse
import json
import os
import random
import sys
import time
import urllib.error
import urllib.request
from pathlib import Path

TOPICS = [
    ("cricket", ["over", "wicket", "yorker", "maiden", "no ball", "run rate"]),
    ("cooking", ["dal", "paneer", "roti", "biryani", "chutney", "halwa"]),
    ("travel", ["Goa", "Manali", "Jaipur", "Kerala", "Ladakh", "Rishikesh"]),
    ("movies", ["comedy", "thriller", "drama", "anime", "documentary", "musical"]),
    ("weather", ["baarish", "garmi", "sardi", "humidity", "toofan", "kohra"]),
]

FACTS = [
    # (depth 0..1, user message, question, expected substrings (any))
    (0.05, "Yaad rakhna: mere deploy server ka port 8011 hai aur uska naam orion hai.",
     "Mere deploy server ka port kya tha?", ["8011"]),
    (0.5, "Meri behen ka naam Riya hai, wo Pune me rehti hai.",
     "Meri behen ka naam kya hai?", ["Riya", "riya"]),
    (0.9, "Project ka codename Falcon rakha hai humne.",
     "Project ka codename kya hai?", ["Falcon", "falcon"]),
]
CORRECTION = (0.2, "Team meeting Monday ko hai.", 0.6, "Nahi, galat bola maine: team meeting Wednesday ko hai, Monday ko nahi.",
              "Team meeting kis din hai?", ["Wednesday", "wednesday", "बुधवार"])
ABSENT = ("Mera passport number kya hai?", ["nahi", "not", "don't", "do not", "pata nahi", "unknown", "no information", "mention"])


def build_chat(size, seed):
    rng = random.Random(seed)
    messages = []
    for i in range(size // 2):
        topic, words = TOPICS[rng.randrange(len(TOPICS))]
        word = words[rng.randrange(len(words))]
        messages.append({"role": "user", "content": f"{topic} ke baare me: {word} ke baare me ek line batao (#{i})."})
        messages.append({"role": "assistant", "content": f"{word.capitalize()} {topic} ka ek hissa hai; isme kai baatein hoti hain (#{i})."})

    def plant(depth, text, reply="Theek hai, yaad rakh liya."):
        at = min(len(messages), int(len(messages) * depth)) // 2 * 2
        messages[at:at] = [{"role": "user", "content": text}, {"role": "assistant", "content": reply}]

    for depth, text, _, _ in FACTS:
        plant(depth, text)
    plant(CORRECTION[0], CORRECTION[1])
    plant(CORRECTION[2], CORRECTION[3], "Theek hai, Wednesday note kar liya.")
    return messages


def ask(url, token, model, session, history, question, mode, n_ctx):
    body = {"stream": False, "max_tokens": 80, "temperature": 0, "seed": 7, "think_mode": "off", "tool_search": False,
            "session_id": session, "history_mode": mode, "llama": {"n_ctx": n_ctx},
            "messages": history + [{"role": "user", "content": question + " Ek chhoti line me jawab do."}]}
    if model:
        body["model"] = model
    request = urllib.request.Request(url.rstrip("/") + "/v1/chat/completions", data=json.dumps(body).encode(),
                                     headers={"Authorization": "Bearer " + token, "Content-Type": "application/json"})
    started = time.time()
    try:
        data = json.load(urllib.request.urlopen(request, timeout=1800))
    except urllib.error.HTTPError as e:
        return {"error": e.read()[:200].decode(errors="replace")}
    plan = data.get("context_report", {}).get("plan", {})
    return {
        "answer": data["choices"][0]["message"]["content"] or "",
        "prompt": data["engine"]["context"]["prompt_tokens"],
        "dropped": data["engine"]["context"].get("dropped_messages", 0),
        "recalled": len((plan.get("recall") or {}).get("recalled", [])),
        "seconds": round(time.time() - started, 1),
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--url", default="http://127.0.0.1:8011")
    parser.add_argument("--token", default=os.environ.get("CLUAIZ_TOKEN"))
    parser.add_argument("--sizes", default="200,1000,2000")
    parser.add_argument("--n-ctx", type=int, default=4096)
    parser.add_argument("--modes", default="recent,adaptive")
    parser.add_argument("--out", help="write every answer to this JSON file")
    args = parser.parse_args()
    token = args.token
    if not token and os.environ.get("CLUAIZ_HOME"):
        token = Path(os.environ["CLUAIZ_HOME"], "session.token").read_text().strip()
    if not token:
        sys.exit("No token: pass --token, or set CLUAIZ_TOKEN or CLUAIZ_HOME")
    model = os.environ.get("CLUAIZ_MODEL")

    questions = [(q, expect, "fact") for _, _, q, expect in FACTS]
    questions.append((CORRECTION[4], CORRECTION[5], "correction"))
    questions.append((ABSENT[0], ABSENT[1], "absent"))
    run_id = int(time.time())
    rows = []
    for size in [int(s) for s in args.sizes.split(",")]:
        history = build_chat(size, seed=size)
        for mode in args.modes.split(","):
            for index, (question, expect, kind) in enumerate(questions):
                session = f"mem-{run_id}-{size}-{mode}-{index}"
                result = ask(args.url, token, model, session, history, question, mode, args.n_ctx)
                answer = result.get("answer", "")
                correct = any(e in answer for e in expect) and "error" not in result
                if kind == "correction":
                    correct = correct and "Monday" not in answer.split("Wednesday")[0][-20:]
                rows.append({"size": size, "mode": mode, "kind": kind, "question": question, "correct": correct, **result})
                print(f"  {size:5d} {mode:8s} {kind:10s} {'OK ' if correct else 'NO '} prompt {result.get('prompt')} "
                      f"dropped {result.get('dropped')} recalled {result.get('recalled')} {result.get('seconds')}s | {answer[:70]!r}"
                      + (f" ERROR {result['error']}" if "error" in result else ""), flush=True)
                # The suite's chats are not kept
                urllib.request.urlopen(urllib.request.Request(f"{args.url.rstrip('/')}/v1/chat/{session}/history", method="DELETE",
                                                              headers={"Authorization": "Bearer " + token}), timeout=60)

    print("\n| size | mode | correct | avg prompt tokens |\n| --- | --- | --- | --- |")
    for size in sorted({r["size"] for r in rows}):
        for mode in args.modes.split(","):
            part = [r for r in rows if r["size"] == size and r["mode"] == mode]
            prompts = [r["prompt"] for r in part if r.get("prompt")]
            print(f"| {size} | {mode} | {sum(r['correct'] for r in part)}/{len(part)} | {round(sum(prompts) / max(len(prompts), 1))} |")
    if args.out:
        Path(args.out).write_text(json.dumps(rows, ensure_ascii=False, indent=1), encoding="utf-8")


if __name__ == "__main__":
    main()
