# Run 74: workflow acceptance suite on Linux

Date: 2026-11-25 (raw files saved 2026-11-27). Machine: GCP L4 VM, Node v18.19.1.
Binary: `target/debug/cluaiz`, sha256 `29958376...e1e5` (`raw/versions.txt`).

## Result: 15 passed, 0 failed

Engine check (`engine-check.cjs`, 7/7): call, switch, parallel, repeat, wait, run, return, rerun_from;
fail step returns an explicit error; emit + event + Engine event triggers; poll, file, schedule and every
triggers.

Runtime check (`runtime-check.cjs`, 8/8): startup trigger; restart recovery keeps the completed first
step; webhook rejects a missing secret (401) and accepts a valid body (202); tool trigger listed and
callable through MCP; Code step runs real WebAssembly (7*6=42); Tool step runs the math plugin; Ask
answers with `qwen3.5-0.8b-gguf-Q4_K_M`; Agent calls the math tool and answers.

Raw: `raw/runner.out`, `raw/*-console.txt`, `raw/*-result-*.json`, Engine logs.
Rerun: `runner.sh` on a Linux VM with an isolated `CLUAIZ_HOME`.
