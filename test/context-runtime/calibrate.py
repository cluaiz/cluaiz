"""Routing-set check and calibration for the tool matcher.

Sends every prompt of routing_set.jsonl to a running Engine's POST /v1/tools/discover (with
"explain": true), scores what the Engine suggests today, then tries other level rules on the
returned evidence: the rules are fitted on one half of the set and reported on the other half.

    python calibrate.py --url http://127.0.0.1:8011 --token <session token>

The token defaults to $CLUAIZ_TOKEN, then <CLUAIZ_HOME>/session.token. The results depend on
which tools are installed and switched on, so run it against a known home (see README.md).
"""
import argparse
import hashlib
import itertools
import json
import os
import sys
import urllib.request
from pathlib import Path

HERE = Path(__file__).resolve().parent
MAX_CARDS = 3
MAX_TITLES = 5

# Done-when of P1.3 (held-out half)
TARGET_HIT = 0.90
TARGET_CHITCHAT_CLEAN = 0.98


def load_rows(path):
    with open(path, encoding="utf-8") as f:
        return [json.loads(line) for line in f if line.strip()]


def held_out(row):
    """Stable split: half of the rows by a hash of their id"""
    return hashlib.md5(row["id"].encode()).digest()[0] % 2 == 1


def discover(url, token, row):
    body = {"query": row["query"], "previous": row.get("previous"), "explain": True}
    request = urllib.request.Request(
        url.rstrip("/") + "/v1/tools/discover",
        data=json.dumps(body).encode(),
        headers={"Authorization": "Bearer " + token, "Content-Type": "application/json"},
    )
    return json.load(urllib.request.urlopen(request, timeout=30))


def levels(evidence, actionable, rule):
    """Suggestions from the evidence under a rule, with the Engine's caps and order"""
    picked = []
    for e in evidence:
        if e["decisive"] or e["good"] >= rule["card_good"]:
            level = "card"
        elif (
            (rule["title_named"] and e["named"])
            or len(e["matched"]) >= rule["title_matched"]
            or (rule["title_actionable_good"] and actionable and e["good"] >= 1)
        ):
            level = "title"
        else:
            continue
        if e["score"] < rule["min_score"]:
            continue
        picked.append((level, e["score"], e["id"]))
    picked.sort(key=lambda p: (p[0] != "card", -p[1], p[2]))
    out, cards, titles = {}, 0, 0
    for level, _, tool in picked:
        if level == "card" and cards < MAX_CARDS:
            out[tool] = "card"
            cards += 1
        elif titles < MAX_TITLES:
            out[tool] = "title"
            titles += 1
    return out


def metrics(rows, suggested):
    """suggested: row id -> {tool id: "card" | "title"}"""
    positive = [r for r in rows if r["expect"]]
    chitchat = [r for r in rows if r["kind"] == "chitchat"]
    traps = [r for r in rows if not r["expect"]]
    hit = [r for r in positive if all(t in suggested[r["id"]] for t in r["expect"])]
    card_hit = [r for r in positive if all(suggested[r["id"]].get(t) == "card" for t in r["expect"])]
    clean = [r for r in chitchat if not suggested[r["id"]]]
    trap_cards = [r for r in traps if "card" in suggested[r["id"]].values()]
    wrong_cards = [
        r for r in positive if any(level == "card" and t not in r["expect"] for t, level in suggested[r["id"]].items())
    ]
    ratio = lambda a, b: len(a) / len(b) if b else 1.0
    return {
        "hit": ratio(hit, positive),
        "card_hit": ratio(card_hit, positive),
        "chitchat_clean": ratio(clean, chitchat),
        "trap_cards": len(trap_cards),
        "wrong_cards": len(wrong_cards),
        # Titles and cards shown with messages that need no tool (cost tokens, change nothing)
        "noise": sum(len(suggested[r["id"]]) for r in traps),
        "rows": len(rows),
        "misses": [r["id"] for r in positive if r not in hit],
        "trap_card_rows": [r["id"] for r in trap_cards],
        "unclean_chitchat": [r["id"] for r in chitchat if r not in clean],
        "wrong_card_rows": [r["id"] for r in wrong_cards],
    }


def passes(m):
    return m["hit"] >= TARGET_HIT and m["chitchat_clean"] >= TARGET_CHITCHAT_CLEAN and m["trap_cards"] == 0


def apply(rows, evidence, actionable, rule):
    return {r["id"]: levels(evidence[r["id"]], actionable[r["id"]], rule) for r in rows}


def fit(rows, evidence, actionable):
    """Best rule on `rows`: fewest trap cards and clean chit-chat first, then hits, cards, fewest
    wrong cards"""
    grid = {
        "card_good": [1, 2, 3],
        "title_named": [True, False],
        "title_matched": [1, 2, 3],
        "title_actionable_good": [True, False],
        "min_score": [0.0, 1.0, 2.0, 3.0, 4.0],
    }
    best = None
    for values in itertools.product(*grid.values()):
        rule = dict(zip(grid.keys(), values))
        m = metrics(rows, apply(rows, evidence, actionable, rule))
        key = (
            -m["trap_cards"],
            m["chitchat_clean"] >= TARGET_CHITCHAT_CLEAN,
            round(m["hit"], 4),
            round(m["card_hit"], 4),
            -m["wrong_cards"],
        )
        if best is None or key > best[0]:
            best = (key, rule, m)
    return best[1], best[2]


def show(name, m):
    print(
        f"  {name:<22} hit {m['hit']:.1%}  card {m['card_hit']:.1%}  chit-chat clean {m['chitchat_clean']:.1%}"
        f"  trap cards {m['trap_cards']}  wrong cards {m['wrong_cards']}  noise {m['noise']}  ({m['rows']} rows)"
        f"  {'PASS' if passes(m) else 'FAIL'}"
    )


def main():
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--url", default="http://127.0.0.1:8011")
    parser.add_argument("--token", default=os.environ.get("CLUAIZ_TOKEN"))
    parser.add_argument("--set", default=str(HERE / "routing_set.jsonl"))
    parser.add_argument("--out", help="write every row's suggestions and evidence to this JSON file")
    args = parser.parse_args()
    token = args.token
    if not token and os.environ.get("CLUAIZ_HOME"):
        token = Path(os.environ["CLUAIZ_HOME"], "session.token").read_text().strip()
    if not token:
        sys.exit("No token: pass --token, or set CLUAIZ_TOKEN or CLUAIZ_HOME")

    rows = load_rows(args.set)
    engine, evidence, actionable = {}, {}, {}
    for row in rows:
        result = discover(args.url, token, row)
        engine[row["id"]] = {c["id"]: c["level"] for c in result["candidates"]}
        evidence[row["id"]] = result.get("evidence", [])
        actionable[row["id"]] = result.get("actionable", False)

    train = [r for r in rows if not held_out(r)]
    test = [r for r in rows if held_out(r)]

    print("Engine as built:")
    for name, part in [("all", rows), ("fit half", train), ("held-out half", test)]:
        show(name, metrics(part, engine))
    m = metrics(rows, engine)
    by_id = {r["id"]: r for r in rows}
    for label, ids in [("missed", m["misses"]), ("trap with a card", m["trap_card_rows"]),
                       ("chit-chat with tools", m["unclean_chitchat"]), ("wrong card", m["wrong_card_rows"])]:
        for rid in ids:
            print(f"    {label:<20} {rid} {by_id[rid]['query'][:60]!r} -> {engine[rid]} (expect {by_id[rid]['expect']})")

    print("By kind and language (all rows, engine as built):")
    for field in ("kind", "lang"):
        for value in sorted({r[field] for r in rows}):
            part = [r for r in rows if r[field] == value]
            pm = metrics(part, engine)
            print(f"  {field}={value:<11} hit {pm['hit']:.1%}  clean {pm['chitchat_clean']:.1%}  trap cards {pm['trap_cards']}  ({len(part)})")

    rule, fitted = fit(train, evidence, actionable)
    print(f"Best rule on the fit half: {rule}")
    show("fit half", fitted)
    show("held-out half", metrics(test, apply(test, evidence, actionable, rule)))
    current = {"card_good": 2, "title_named": True, "title_matched": 2, "title_actionable_good": True, "min_score": 0.0}
    show("held-out (current rule)", metrics(test, apply(test, evidence, actionable, current)))

    if args.out:
        Path(args.out).write_text(
            json.dumps({"rule": rule, "rows": [{**r, "engine": engine[r["id"]], "evidence": evidence[r["id"]]} for r in rows]},
                       ensure_ascii=False, indent=1),
            encoding="utf-8",
        )
    sys.exit(0 if passes(metrics(test, engine)) else 1)


if __name__ == "__main__":
    main()
