# Run 55: soak readiness on the shared Windows laptop (2026-11-24)

This run checked the updated client and whether a long soak was safe to interpret. It did not run for hours. Windows 11, Ryzen 7 7435HS, 23.69 GiB RAM, RTX 3050 4 GB. Existing debug `target/debug/deps/cluaiz.exe`, SHA-256 `9A91A8A73339F3F6A90503B45968AE21D09C8A0848BD423427E8B69DB00A5F19`, was used without rebuilding; exact source commit of the binary is unknown. Checkout before the client change was `a54d8937`. Isolated home `output/run55-soak-home` and port 8013; model registry points to the existing read-only Qwen3.5 0.8B Q4_K_M model fixture in the earlier isolated test home. The owner's server and `~/.cluaiz` were not used.

Before the tests, free RAM was 5.01 GiB and free VRAM was 2561 MiB. The machine is shared. During the second smoke, the GPU sample reached 3688 MiB used / 205 MiB free; after the smokes, free RAM was 3.41 GiB and free VRAM was 1682 MiB. Those host GPU numbers include other programs, so they cannot be used as Engine-only memory. The native Windows process sampler did record Engine working set, including 1,201,312, 1,222,992 and 1,227,604 KiB in three consecutive samples in `smoke2/memory.jsonl`.

| Invocation | Setup | Result | Evidence |
|---|---|---:|---|
| Smoke 1 | seed 5500, target 2 minutes, 3 users, restart every 2 sweeps | 2/3 checks passed | `smoke/` JSONL and Engine log |
| Smoke 2 | seed 5501, target 0.25 minutes, 3 users, restart every 10 sweeps, long-input and think-on excluded | 2/2 checks passed | `smoke2/` JSONL and Engine log |

Smoke 1's first two checks passed: thinking chat HTTP 200 and logprobs refused with 400. Its long-input request took 151,910.94 ms and returned HTTP 200 with empty content and tool-shaped reasoning text instead of marker 4917. This repeats the Run 52 answer failure on a different existing binary. See [`bugs/long-input-empty-answer.md`](bugs/long-input-empty-answer.md). Smoke 2 passed malformed-JSON rejection and max-completion-token enforcement. Both invocations completed one partial sweep; no scheduled restart occurred, so restart pacing remains unverified.

The client change in `test/release_soak/soak.py` adds configurable restart spacing, explicit scenario exclusions and a Windows native working-set sampler. The two smoke commands ran the changed client, and `smoke2/memory.jsonl` contains non-null working-set readings. The failed long-input check remains in the raw record; exclusions affect only the proposed long-duration load mix.

The two-hour soak was not started because RAM fell to 3.41 GiB free and the shared GPU reached 205 MiB free during the readiness checks. A leak or speed conclusion under that pressure would be confounded. Model switching, in-flight restart, providers, workflows and other feature areas were not checked in this run. No Engine code was changed. The raw files were scanned for the session token, username, loopback IP and common provider-key prefixes; none of those markers appeared.
