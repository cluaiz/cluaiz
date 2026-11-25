# Run 76: media before text on image requests with Engine tools installed

Date: 2026-11-25. Machine: Windows 11 laptop, Ryzen 7 7435HS, RTX 3050 4 GB. Debug builds (`cargo build -p cmd`).
Model: Qwen3.5-0.8B Q4_K_M with mmproj F16 (files of the run49-e2e test home, read only).
Home: isolated `C:\Users\<user>\AppData\Local\cluaiz-test\run76home` (config and tools copied from run49-e2e:
math plugin + filesystem MCP installed; tool paths rewritten to the new home). Port 8013.

## Problem

With Engine tools installed, a user message "text, image" made Qwen3.5-0.8B call `read_item` (for example
`{"kind":"rule","name":"image"}` or `"color"`) and then answer about the tool result instead of the image.
The rendered user turn before the fix (raw/prompts_before.txt):

    <tools_hint>...call read_item with its name and follow it.</tools_hint>
    What colour is this image? One word.[cluaiz-media-0-0]

## Change (commit 0961d0ee)

`api/src/handlers/chat/context_plan.rs:112` and `:227` (`media_first`): while history is rendered, each user
message's attachments (image, audio, video, file parts) are put in front of its text parts, both groups in
the client's order, the words unchanged. The Engine's block (tool hint, notes, old turn blocks) is put in
front afterwards, so the model reads: block, media, the user's text (raw/prompts_after.txt):

    <tools_hint>...</tools_hint>[cluaiz-media-0-0]What colour is this image? One word.

Tools and tool schemas are unchanged.

## Try 1 (not committed): media before the block

First attempt in `engine/media/src/request.rs` put the media in front of every text part, including the
Engine's hint: block after media (raw/prompts_try1.txt, `[media]<tools_hint>...</tools_hint>question`).
All layouts failed 0 / 30 (the model called `read_item` for "color"). Reverted. So the block placed
"after the media" as first asked is worse than block, media, text.

## Results (pass = no tool call and "red" in the reply; weather pass = `get_weather` tool call)

| Layout | before (5c2eeeea) | try 1 | after (0961d0ee) |
|---|---|---|---|
| text first | 0 / 10 | 0 / 10 | 10 / 10 |
| image first | 10 / 10 | 0 / 10 | 10 / 10 |
| text, image, text | 10 / 10 | 0 / 10 | 9 / 9 (1 x HTTP 500) |
| get_weather (rotated layouts) | 10 / 10 | 10 / 10 | 9 / 9 (1 x HTTP 500) |

The 2 HTTP 500 replies in the after run were "Media needs 1279333812 host bytes; live RAM budget is
1045941862" (free laptop RAM at that moment), not a wrong answer; they are not counted.

## llama.cpp reference

llama-server keeps the client's part order: each `image_url` part becomes a `media_marker` text part in
place (`tools/server/server-common.cpp:1351-1352`, local copy in `~/.unsloth/llama.cpp`, commit unknown).
mtmd-cli puts the marker before the prompt when the user did not place one ("most models require the
marker before each image", `tools/mtmd/mtmd-cli.cpp:602-606`, PR 17616). The Engine now differs from
llama-server for user messages on purpose (media first), and matches mtmd-cli's default.

## Rerun

    python vision_order.py --exe <repo>/target/debug/cluaiz.exe --home <isolated home> \
        --model qwen3.5-0.8b-gguf-Q4_K_M --rounds 10 --out raw --tag after
    python user_turns.py raw/after_serve.log 3

## Files

- vision_order.py: the test (starts the server, 3 probes + 10 rounds, writes <tag>.jsonl/.txt/_serve.log).
- user_turns.py: prints the rendered user turns from a -vv serve log.
- raw/before*, raw/try1_media_before_hint_after*, raw/after*: requests, replies, server logs.
- raw/prompts_*.txt: rendered user turns per phase.

## Not done

- One model (0.8B), one image, one machine; 2B and other vision models not tested.
- Unit tests for `media_first` not added; api test suite not run (forbidden for this task).
- Progress events name attachments by the prepared part index; after the reorder that index can differ
  from the client's part index (it already differed by one when the hint is added). Not changed.
- The reorder also applies to provider (cloud) models and to every user message in the history.
- "image, text, hint" order not tested.
