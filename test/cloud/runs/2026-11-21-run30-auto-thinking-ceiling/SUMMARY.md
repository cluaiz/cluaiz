# Run 30: Auto thinking ceiling follows the measured speed

Windows 11 (build 26300), Engine 0.1.0 debug build (`cargo build -p cmd`), Qwen3-0.6B Q4_K_M on the
machine's GPU, 16k context, an isolated `CLUAIZ_HOME` on port 8011 (never the owner's 8000 server).
Script: `auto-effort.sh`. Raw output: `output.txt` (usage lines, then the server log lines).

## What changed in the code

`reasoning_effort: "auto"` was the same as `"high"` (`engine/core/src/settings.rs`). It is now 180 seconds
of thinking at the loaded model's last measured decode speed, at least 1,024 tokens, at most High's share
of the room, never Max. The first answer after a load has no speed yet and uses High's share.

## Result

| Request | Ceiling (log) | Speed used | Thinking tokens the model used | Ended by |
|---|---|---|---|---|
| "What is 2+2?" | 11,976 | none yet (first answer) | 285 | the model |
| 17 x 23 with steps | 11,971 | 180.2 t/s | 1,749 | the model |
| train trip length | 11,963 | 173.9 t/s | 1,176 | the model |

180 t/s x 180 s is about 32,400 tokens, more than 75% of the room, so the room share applied, as the
rule says. The model ended every thought on its own, far below the ceiling.

## Not tested here

A slow machine, where the speed rule lowers the ceiling (5 t/s gives 1,024, 36 t/s gives 6,480). Covered
by unit tests in `engine/core/src/settings_tests.rs`; a CPU-only run is pending (8 GB cloud VM).
