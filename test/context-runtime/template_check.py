"""Template check (plan P2.3): does each installed model's chat template show every part of a
conversation with a tool step, inline and native?

    python template_check.py --exe target/release/cluaiz.exe --home <test home> --workspace <fixture> \
        --models qwen3-4b-gguf-Q4_K_M,gemma-4-e4b-it-gguf-Q4_K_M,llama_3.2_instruct-3b-gguf-q4_k_m

Each request asks for the rendered prompt (`return_prompt`) and generates one token. A part is
"shown" when its text is in the prompt; `template_dropped` is the Engine's own check of the same,
and a part either check misses fails the model.
"""
import argparse
import json
import sys

from harness import add_engine_args, engine_from

CALL = '<tool_call>\n{"name": "read_text_file", "arguments": {"path": "notes.txt"}}\n</tool_call>'
CONVERSATION = [
    {"role": "system", "content": "Project rule: always answer in plain English."},
    {"role": "user", "content": "Read notes.txt please"},
    {"role": "assistant", "content": CALL},
    {"role": "tool", "content": json.dumps({"content": [{"type": "text", "text": "The secret word is MANGO-42."}]})},
    # The answer does not repeat the result, so "result" is shown only by the tool turn
    {"role": "assistant", "content": "Found it, the file names a secret word."},
    {"role": "user", "content": "Thanks, and what is in calc.py?"},
]
PARTS = {
    "system": "Project rule: always answer in plain English.",
    "user": "Read notes.txt please",
    "call": "read_text_file",
    "result": "MANGO-42",
    "last user": "Thanks, and what is in calc.py?",
}


def check(engine, model, tool_format):
    response = engine.chat({"model": model, "messages": CONVERSATION, "max_tokens": 1, "think_mode": "off",
                            "storage": "none", "tools": ["filesystem"], "tool_search": False, "return_prompt": True,
                            "context_policy": {"tool_format": tool_format}})
    context = response["engine"]["context"]
    prompt = context.get("prompt") or ""
    # The function name is also in the tool list: the call itself makes a second mention
    missing = [name for name, text in PARTS.items() if prompt.count(text) < (2 if name == "call" else 1)]
    used = (response.get("context_report") or {}).get("tools", {}).get("format")
    return missing, context.get("template_dropped", []), used, len(prompt)


def main():
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    add_engine_args(parser)
    parser.add_argument("--models", required=True)
    args = parser.parse_args()
    engine = engine_from(args)
    if engine.exe:
        engine.start()
    failed = False
    try:
        for model in args.models.split(","):
            for tool_format in ("inline", "native"):
                try:
                    missing, dropped, used, size = check(engine, model, tool_format)
                except Exception as e:
                    print(f"FAIL {model:<40} {tool_format:<6} {type(e).__name__}: {e}", flush=True)
                    failed = True
                    continue
                ok = not missing and not dropped
                failed |= not ok
                print(f"{'PASS' if ok else 'FAIL'} {model:<40} asked {tool_format:<6} used {used:<6} prompt {size:>6} chars"
                      f"  missing {missing}  engine check: dropped {dropped}", flush=True)
    finally:
        engine.stop()
    sys.exit(1 if failed else 0)


if __name__ == "__main__":
    main()
