# Workflow acceptance test

23 November 2026. Cluaiz App web build from the local checkout at `51a1a2c`, served by Vite on port 1430. Live tests used the existing release Engine binary, SHA-256 `69F2378367CC88951A18A6E9EBC1268F77FD7B45D247FD157D920F621B6A8744`. The Engine checkout was at `955c815c`, and the binary was not rebuilt for this test. The test Engine used a fresh workspace home and `127.0.0.2:8011` because another process owned the wildcard listener on port 8011. Browser setup pinned requests to this isolated host, including startup fallbacks. Port 8000 was blocked in the browser.

## What ran through the App

| Scenario | Observed result |
| --- | --- |
| New canvas | Added Return from the palette, used Undo and Redo, saved and started the flow. Run `run_5e919e59a94e` succeeded. Ten trigger choices, fifteen built-in step choices and the separate Installed tools and Engine methods sections were displayed. [Browser evidence](browser-result-1795442573396.json) |
| Import and reopen | Imported [weekly-report.flow.yaml](weekly-report.flow.yaml) and reopened it in Studio. The App started the run and fetched real step records. [Import evidence](browser-result-1795441807185.json), [completed browser test](browser-result-1795442003619.json) |
| Approve and reject | Engine approval produced all three sample updates in `run_f05e4119872e`. Rejection produced `approval_rejected` in `run_d7c255bec207`. Both runs started from the App. Decisions went through the Engine API because the App page offered no decision control. [Browser evidence](browser-result-1795442003619.json), [Approvals screenshot](02-approvals.png) |
| Actual notes-to-file workflow | The App started [weekly-report-ai.flow.yaml](weekly-report-ai.flow.yaml). The Engine read the notes and the local Qwen3.5 0.8B model drafted the report. No output file existed before approval. After approval via the Engine API, run `run_cb580e6491a4` succeeded and the file matched the draft byte for byte. [Result](browser-result-1795442787270.json), [written report](report.md) |

The AI example uses a `weekly-report/notes.txt` input in the current workspace and writes `weekly-report/report.md`. The folder must already exist, the editor must confirm its permissions, and a chat model must be selected. The browser test registers Qwen3.5 0.8B from an existing isolated test fixture and writes its identifier into the saved test copy. The first run with only inferred per-file paths failed at `fs.write` with `needs roots`. Adding permission for the existing report folder allowed the write. [Permission failure](browser-result-1795442696440.json)

## Live Engine coverage

**17 of 17 step kinds** ran in at least one scenario. The browser report covered Set, If, For each, Approve and Return. The Engine cases exercised Method, Switch, Parallel, Repeat, Wait, Run another flow, Emit and Fail, plus real WebAssembly Code, an installed Tool, a real local Ask and an Agent that called the installed math tool. The failure case was expected and checked for its error. Rerun from a selected step returned the same result. [Logic cases](engine-result-1795442053803.json), [runtime cases](runtime-result-1795442513507.json)

**10 of 10 trigger kinds** fired in the live environment. Manual fired from the App. Schedule, Every, Poll, File, Engine event and Flow event fired in the Engine suite. Startup, Webhook and Called as a tool fired in the runtime suite. The webhook rejected requests without its secret with HTTP 401 and accepted valid requests with HTTP 202. The tool trigger was listed and invoked using MCP. A run that was waiting before restart resumed, preserved its first completed step and returned `{"survived":true}`. [Logic and triggers](engine-result-1795442053803.json), [runtime and restart](runtime-result-1795442513507.json)

Focused existing tests also ran. `node --test test/automation_flow_model.test.cjs test/cep_flows.test.cjs` reported `12 tests, 12 passed, 0 failed`. `cargo test -p cluaiz-automation --offline` reported 71, 1, 6, 19 and 20 passed in the five groups, with no failures. Several core tests use a simulated host. The browser and live cases above used the actual Engine.

## App gaps reproduced

1. A workflow can reach an approval and appear on the Approvals page while offering zero Approve or Reject controls on that page. The Engine accepted both decisions through its API. The run returned `waiting` while its review step returned `running`. [Approval evidence](browser-result-1795442003619.json)
2. Saving an edit to a disabled workflow changed `enabled` from `false` to `true`. This was reproduced on the App-created test flow. [Save evidence](browser-result-1795442573396.json)
3. A flow using the inferred path of a new output file showed Valid, read its input and obtained approval, then got `needs roots` while writing. Declaring the existing output folder made the same workflow succeed. [Failed write](browser-result-1795442696440.json), [successful write](browser-result-1795442787270.json)

This run covers the App in Chrome, selected paths for every Engine step and trigger, and the installed math plugin. Native Tauri, calling the workflow from the App's Chat, other installed tools, and all combinations of advanced fields remain outside the live evidence. Product source changes were outside this test pass.

