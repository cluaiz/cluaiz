"""Writes engine/onnx/src/audio/tts/data/numbers/<lang>.json from cldr-json (pinned commit) for the
languages that have CLDR spell-out rules. Dev tool; the generated files are what ships.

usage (from the repo root):
  python -I tools/cldr/make_numbers.py engine/onnx/src/audio/tts/data/numbers es fr it pt ja zh ar fa gu ne ta cs pl ro hu fi tr hr bs sr
en.json and hi.json are written by hand and are not generated.
"""
import json
import re
import sys
import urllib.request

COMMIT = "91c267402229a59e3ef2774544f001bf959e8809"  # same cldr-json commit as reading_rules.json
BASE = f"https://raw.githubusercontent.com/unicode-org/cldr-json/{COMMIT}/cldr-json/"
CURRENCIES = {"₹": "INR", "$": "USD", "€": "EUR", "£": "GBP", "¥": "JPY"}
ENTRY = "%spellout-numbering"
# Editorial overrides where CLDR's written form is not what is said aloud
OVERRIDES = {
    "ja": {"point": "点"},  # CLDR writes "・" between the parts; it is said 点 (ten)
    "zh": {"suffixes": {"%": "百分之{0}"}},  # CLDR's long unit is "{0}%"; it is said 百分之…
}


def get(path):
    with urllib.request.urlopen(BASE + path, timeout=60) as res:
        return json.loads(res.read())


plurals = get("cldr-core/supplemental/plurals.json")["supplemental"]["plurals-type-cardinal"]
out_dir, langs = sys.argv[1], sys.argv[2:]

for lang in langs:
    rules = get(f"cldr-rbnf/rbnf/{lang}.json")["rbnf"]["rbnf"]["SpelloutRules"]
    # Rule sets reachable from the entry set
    reach, todo = {}, [ENTRY]
    while todo:
        name = todo.pop()
        if name in reach or name not in rules:
            continue
        reach[name] = rules[name]
        for _, body in rules[name]:
            todo += re.findall(r"%%?[\w-]+", body)
    # The decimal word: literal text of the first reachable "x.x" rule
    point = None
    for name in [ENTRY] + sorted(reach):
        for descriptor, body in reach[name]:
            if descriptor == "x.x":
                literal = re.sub(r"[←→<>=][^←→<>=]*[←→<>=]+", " ", body.rstrip(";"))
                point = " ".join(literal.split()) or None
                break
        if point:
            break
    main = f"cldr-numbers-full/main/{lang}"
    names = get(f"{main}/currencies.json")["main"][lang]["numbers"]["currencies"]
    numbers = get(f"{main}/numbers.json")["main"][lang]["numbers"]
    pattern = numbers.get("currencyFormats-numberSystem-latn", {}).get("unitPattern-count-other", "{0} {1}")
    currency = {
        sym: pattern.replace("{1}", names[code].get("displayName-count-other") or names[code]["displayName"])
        for sym, code in CURRENCIES.items() if code in names
    }
    units = get(f"cldr-units-full/main/{lang}/units.json")["main"][lang]["units"]["long"]
    percent = units.get("concentr-percent", {}).get("unitPattern-count-other")
    plural = [[k.split("-")[-1], v.split("@")[0].strip()] for k, v in plurals.get(lang, {}).items()]
    data = {
        "language": lang,
        "_about": f"Generated from CLDR (cldr-json {COMMIT[:7]}, Unicode-3.0, see ../CLDR-LICENSE.txt). "
                  "Whole numbers use the RBNF rule sets reachable from %spellout-numbering ('spellout'), with "
                  "the cardinal plural rules; 'point' is the decimal word of the x.x rule; currency words are "
                  "CLDR currency names placed by the currency unit pattern ({0} = the number); '%' is the "
                  "percent unit pattern. Regenerate rather than edit by hand.",
        "point": point or "",
        "read_digits_from": 10,
        "currency": currency,
        "suffixes": {"%": percent} if percent else {},
        "spellout": {"entry": ENTRY, "sets": reach, "plural": plural},
        "examples": {},
    }
    data.update(OVERRIDES.get(lang, {}))
    if lang in OVERRIDES:
        data["_about"] += " Editorial overrides: " + ", ".join(OVERRIDES[lang]) + " (said aloud differently from CLDR's written form)."
    path = f"{out_dir}/{lang}.json"
    old = None
    try:
        old = json.load(open(path, encoding="utf-8"))
    except FileNotFoundError:
        pass
    if old and old.get("examples"):
        data["examples"] = old["examples"]  # examples are written by hand; keep them
    open(path, "w", encoding="utf-8", newline="\n").write(json.dumps(data, ensure_ascii=False, indent=1) + "\n")
    print(lang, "sets", len(reach), "point", point, "%", percent, "₹", currency.get("₹"))
