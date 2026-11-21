"""Run 39: how often small local models make a correct tool call, through the Engine's OpenAI route.

Each model gets the same 40 tasks (temperature 0, thinking off, 5 tools offered every time):
  - 24 "call" tasks: one tool is right; the call must name it and carry the right arguments.
  - 8 "pick" tasks: two tools look close; only one is right.
  - 8 "none" tasks: no tool is needed; a call counts as a wrong call.

A call is "correct" when the tool name matches and every expected argument is present with the expected
value (strings compared ignoring case and end punctuation, numbers by value). A "no_call" on a task that
needs a tool is a reply in plain text, often an invented answer. Raw replies go to <model>.jsonl.

Usage: python probe.py <host> <token_file> <model>[:think] [...]
A ":think" suffix runs the same tasks with thinking on (max_tokens 1536 for the thinking).
"""

import json
import sys
import time
import urllib.error
import urllib.request

HOST, TOKEN_FILE, MODELS = sys.argv[1], sys.argv[2], sys.argv[3:]
TOKEN = open(TOKEN_FILE, encoding="utf-8").read().strip()


def fn(name, desc, props, required):
    return {"type": "function", "function": {"name": name, "description": desc, "parameters": {
        "type": "object", "properties": props, "required": required}}}


TOOLS = [
    fn("get_weather", "Current weather for a city", {
        "city": {"type": "string"}, "unit": {"type": "string", "enum": ["celsius", "fahrenheit"]}}, ["city"]),
    fn("convert_currency", "Convert an amount between two currencies (ISO codes)", {
        "amount": {"type": "number"}, "from_currency": {"type": "string"}, "to_currency": {"type": "string"}},
        ["amount", "from_currency", "to_currency"]),
    fn("create_reminder", "Create a reminder at a time today (24h HH:MM)", {
        "text": {"type": "string"}, "time": {"type": "string"}}, ["text", "time"]),
    fn("search_files", "Search the user's files by keyword and optional file extension", {
        "query": {"type": "string"}, "extension": {"type": "string"}}, ["query"]),
    fn("send_email", "Send an email", {
        "to": {"type": "string"}, "subject": {"type": "string"}, "body": {"type": "string"}}, ["to", "subject"]),
]

# (kind, prompt, expected tool or None, expected arguments)
TASKS = [
    ("call", "What's the weather in Paris right now?", "get_weather", {"city": "Paris"}),
    ("call", "Weather in Tokyo in fahrenheit please.", "get_weather", {"city": "Tokyo", "unit": "fahrenheit"}),
    ("call", "Is it raining in Mumbai?", "get_weather", {"city": "Mumbai"}),
    ("call", "How hot is it in Cairo, in celsius?", "get_weather", {"city": "Cairo", "unit": "celsius"}),
    ("call", "Check the weather for Berlin.", "get_weather", {"city": "Berlin"}),
    ("call", "Convert 100 USD to EUR.", "convert_currency", {"amount": 100, "from_currency": "USD", "to_currency": "EUR"}),
    ("call", "How many Japanese yen is 50 British pounds?", "convert_currency", {"amount": 50, "from_currency": "GBP", "to_currency": "JPY"}),
    ("call", "I have 2500 INR, what is that in USD?", "convert_currency", {"amount": 2500, "from_currency": "INR", "to_currency": "USD"}),
    ("call", "Change 75.5 euros into Swiss francs.", "convert_currency", {"amount": 75.5, "from_currency": "EUR", "to_currency": "CHF"}),
    ("call", "Convert 1000 CAD to AUD", "convert_currency", {"amount": 1000, "from_currency": "CAD", "to_currency": "AUD"}),
    ("call", "Remind me to call mom at 18:30.", "create_reminder", {"time": "18:30"}),
    ("call", "Set a reminder for 09:00 to submit the report.", "create_reminder", {"time": "09:00"}),
    ("call", "At 14:15 remind me about the dentist.", "create_reminder", {"time": "14:15"}),
    ("call", "Create a reminder 'water plants' at 07:45.", "create_reminder", {"time": "07:45"}),
    ("call", "Remind me at 22:00 to take medicine.", "create_reminder", {"time": "22:00"}),
    ("call", "Find my files about invoices.", "search_files", {"query": "invoices"}),
    ("call", "Search for PDF files about taxes.", "search_files", {"query": "taxes", "extension": "pdf"}),
    ("call", "Look through my files for anything mentioning 'roadmap'.", "search_files", {"query": "roadmap"}),
    ("call", "Find python files that mention parser.", "search_files", {"query": "parser", "extension": "py"}),
    ("call", "Search my documents for 'contract'.", "search_files", {"query": "contract"}),
    ("call", "Email bob@example.com with subject 'Lunch' saying see you at noon.", "send_email", {"to": "bob@example.com", "subject": "Lunch"}),
    ("call", "Send an email to anna@example.org, subject Invoice, body: attached.", "send_email", {"to": "anna@example.org", "subject": "Invoice"}),
    ("call", "Write to team@example.com with subject 'Standup moved' and tell them it's at 11.", "send_email", {"to": "team@example.com", "subject": "Standup moved"}),
    ("call", "Mail sam@example.net, subject: Hello.", "send_email", {"to": "sam@example.net", "subject": "Hello"}),
    ("pick", "Do I need an umbrella in London today?", "get_weather", {"city": "London"}),
    ("pick", "Price check: 20 dollars in rupees?", "convert_currency", {"amount": 20, "from_currency": "USD", "to_currency": "INR"}),
    ("pick", "Don't let me forget the meeting at 16:00.", "create_reminder", {"time": "16:00"}),
    ("pick", "Where did I save my notes about the budget?", "search_files", {"query": "budget"}),
    ("pick", "Let carol@example.com know, subject 'Delay', that I'm late.", "send_email", {"to": "carol@example.com", "subject": "Delay"}),
    ("pick", "Is it cold enough for a jacket in Oslo?", "get_weather", {"city": "Oslo"}),
    ("pick", "My flight costs 300 EUR; how much is that in USD?", "convert_currency", {"amount": 300, "from_currency": "EUR", "to_currency": "USD"}),
    ("pick", "Ping me at 12:30 to eat lunch.", "create_reminder", {"time": "12:30"}),
    ("none", "What is 17 times 23?", None, {}),
    ("none", "Write a haiku about autumn.", None, {}),
    ("none", "What does HTTP stand for?", None, {}),
    ("none", "Translate 'good morning' into Spanish.", None, {}),
    ("none", "Who wrote Romeo and Juliet?", None, {}),
    ("none", "Explain recursion in one sentence.", None, {}),
    ("none", "What is the capital of Canada?", None, {}),
    ("none", "Give me a synonym for 'happy'.", None, {}),
]


def same(want, got):
    if isinstance(want, (int, float)):
        try:
            return abs(float(got) - want) < 1e-6
        except (TypeError, ValueError):
            return False
    # Case, surrounding spaces, quotes and end punctuation are ignored ("Hello." matches "Hello")
    return isinstance(got, str) and got.strip(" .!?'\"").lower() == str(want).lower()


def ask(model, prompt):
    think = model.endswith(":think")
    body = {"model": model.removesuffix(":think"), "temperature": 0, "max_tokens": 1536 if think else 256,
            "think_mode": "on" if think else "off", "temporary_chat": "strict",
            "tools": TOOLS, "messages": [{"role": "user", "content": prompt}]}
    req = urllib.request.Request(HOST + "/v1/chat/completions", json.dumps(body).encode(),
                                 {"Content-Type": "application/json", "Authorization": f"Bearer {TOKEN}"})
    t0 = time.monotonic()
    try:
        with urllib.request.urlopen(req, timeout=300) as r:
            return json.loads(r.read()), round(time.monotonic() - t0, 2), None
    except urllib.error.HTTPError as e:
        return None, round(time.monotonic() - t0, 2), f"HTTP {e.code}: {e.read().decode()[:200]}"


def grade(kind, want_tool, want_args, reply):
    msg = reply["choices"][0]["message"]
    calls = msg.get("tool_calls") or []
    content = msg.get("content") or ""
    leak = any(s in content for s in ("<tool_call>", '"name":', "<|python_tag|>", "[TOOL_CALLS]"))
    if want_tool is None:
        return ("correct" if not calls else "wrong_call"), leak, calls, content
    if not calls:
        return ("no_call_leaked" if leak else "no_call"), leak, calls, content
    c = calls[0]["function"]
    if c.get("name") != want_tool:
        return "wrong_tool", leak, calls, content
    try:
        args = json.loads(c.get("arguments") or "{}")
    except json.JSONDecodeError:
        return "bad_json", leak, calls, content
    if all(same(v, args.get(k)) for k, v in want_args.items()):
        return "correct", leak, calls, content
    return "wrong_args", leak, calls, content


for model in MODELS:
    tally, out = {}, open(f"{model.replace(':', '-')}.jsonl", "w", encoding="utf-8")
    secs = []
    for kind, prompt, want_tool, want_args in TASKS:
        reply, s, err = ask(model, prompt)
        secs.append(s)
        if err:
            verdict, leak, calls, content = "http_error", False, [], err
        else:
            verdict, leak, calls, content = grade(kind, want_tool, want_args, reply)
        tally.setdefault(kind, {}).setdefault(verdict, 0)
        tally[kind][verdict] += 1
        out.write(json.dumps({"kind": kind, "prompt": prompt, "want_tool": want_tool, "want_args": want_args,
                              "verdict": verdict, "leak": leak, "tool_calls": calls, "content": content[:400],
                              "seconds": s}) + "\n")
    out.close()
    ok = sum(v.get("correct", 0) for v in tally.values())
    print(f"## {model}: {ok}/{len(TASKS)} correct, median {sorted(secs)[len(secs) // 2]} s per request")
    for kind in ("call", "pick", "none"):
        print(f"   {kind}: {tally.get(kind)}")
