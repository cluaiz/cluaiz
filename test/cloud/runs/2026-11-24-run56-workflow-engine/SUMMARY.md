# Run 56: Engine workflow logic and trigger regression (2026-11-24)

Windows 11 laptop, Ryzen 7 7435HS, 23.69 GiB RAM, RTX 3050 4 GB. Existing debug `target/debug/deps/cluaiz.exe`, SHA-256 `9A91A8A73339F3F6A90503B45968AE21D09C8A0848BD423427E8B69DB00A5F19`, without a rebuild; exact binary source commit is unknown. A fresh `output/run56-workflow-home` and port 8013 were used. The owner's port 8000 server and `~/.cluaiz` were not touched. The home has a chat model registry entry pointing to an earlier isolated test fixture, and the math plugin was installed only into this test home.

`runner.py` started and stopped the Engine, then ran the adapted `engine-check.cjs`. It recorded seven scenario checks, **7/7 passed** (`raw/engine-result-1795486158344.json`, `raw/console.log`, `raw/engine.log`). The scenarios cover:

| Scenario | Result |
|---|---|
| Method call, switch, parallel, repeat, wait, child flow, return, rerun from child | pass; the first run and rerun succeeded with matching output |
| Explicit fail step | pass; run failed with the expected error text |
| Emit, flow event and Engine event triggers | pass; receiver got value 42 |
| Poll trigger | pass |
| File-created trigger | pass |
| Schedule trigger | pass |
| Every trigger | pass |

The math plugin install was first rejected by automatic approval review as an unauthorized download/install action. The owner's earlier release-test instruction explicitly listed plugin and MCP install checks, and Run 53 had already used the same command in an isolated home. After those facts were supplied, automatic review approved the same command; `raw/plugin_install.txt` records the isolated install. No install code or owner settings were changed.

The adapted `runtime-check.cjs full` was attempted to check startup, a waiting run across restart, webhook secret rejection, workflow-as-tool, WebAssembly Code, Tool, Ask and Agent. **None of those checks ran.** Its Engine-start readiness wait was 18 seconds and timed out before the Engine reported ready (`raw/runtime-result-1795486314725.json`, `raw/runtime-engine.log`). The readiness wait was raised to 90 seconds in the test script after this harness failure, but that change was not rerun. Free GPU memory was 447 MiB at inspection and 385 MiB at the later check on this shared laptop, so a forced GPU/model run would not give a clean result. This setup failure is excluded from the 7/7 workflow count.

The exact 30-flow collection mentioned in the release-test request was not found as saved flow files in this workspace and was not run here. Approval steps, webhook, tool-trigger, Code, Tool, Ask, Agent, startup recovery, in-flight restart, App Studio and all advanced field combinations remain outside Run 56's live evidence. Previous Runs 51-53 contain evidence for some of those paths on other setups. This run changed only test scripts and documentation, not Engine source.

The saved raw files were checked for the isolated session token, username and loopback IP. The plugin-install transcript's username was redacted before commit. The test script was syntax-checked with `node --check`, and the Python runner compiled. No performance claim is made from the trigger timings on a shared machine.
