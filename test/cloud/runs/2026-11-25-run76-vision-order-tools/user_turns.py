"""Run 76: print the rendered user turn of the first few prompts in a `cluaiz -vv serve` log.

    python user_turns.py <serve.log> [count]

Shows where the tool hint, the user's text and the media marker ([cluaiz-media-N-M]) sit.
"""
import sys

lines = open(sys.argv[1], encoding="utf-8", errors="replace").read().split("<|im_start|>user\n")[1:]
for turn in lines[: int(sys.argv[2]) if len(sys.argv) > 2 else 3]:
    print(turn.split("<|im_end|>")[0])
    print("-----")
