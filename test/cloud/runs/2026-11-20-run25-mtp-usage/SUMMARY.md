# Run 25: MTP usage reporting

Date: 2026-11-20. Windows build 26300. Engine base `e114cc07` plus the local reporting patch.

## Checks

- `cargo test --release -p cluaiz-api --lib --test mtp_contract handlers::chat::turn_end::tests` passed:
  `test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured; 82 filtered out`.
  The `mtp_contract` test binary compiled and lists
  `mtp_usage_is_returned_for_streaming_and_non_streaming`. The live test was filtered out.
- `cargo build -p cmd` finished successfully: `Finished dev profile [unoptimized] target(s) in 45m 10s`.

- The API payload unit tests and CLI percentage calculation ran in an isolated Rust harness using
  the source from `api/src/handlers/chat/turn_end.rs`, `engine/llama/src/native/core.rs`,
  and `cmd/src/chat/mod.rs`. The harness used the locked serde and serde_json versions.
  Output: `test result: ok. 6 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out`.
- App `npm run sync:api` and `npm run check:api`: both printed
  `API docs: 123 requests in 27 groups`. Direct catalog inspection found `usage.mtp`
  on `/v1/chat/completions`.
- App `npm run test:api-docs`: 19 passed, 1 failed on
  `notes.move: Parameters, defaults and value effects`. The same assertion failed with the
  pre-change Engine chat page restored in a separate baseline docs copy.

- In an isolated home, `cluaiz run <TinyLLama-v0.1-5M-F16.gguf> --local --mtp on --n-gpu-layers 0 --n-predict 4 --temp 0 -v -p 'Hello'`
  generated four tokens on CPU. The observed checks were `CLI exit: 0` and `MTP kept lines: 0`.
  The fixture has no MTP layers. This was an output check, not a throughput benchmark.

## Scope

These checks cover JSON serialization, existing token totals, CLI percentage arithmetic and catalog
generation. The CLI smoke check exercised a GGUF without MTP. The live API model test requires an MTP-capable
GGUF available by path outside the owner's runtime home.

To run the repository tests from the Engine root:

```powershell
cargo test --release -p cluaiz-api --lib handlers::chat::turn_end::tests
```

For the live test, set `CLUAIZ_HOME` and `CLUAIZ_MTP_TEST_HOME` to the same fresh empty
absolute directory and set `CLUAIZ_MTP_TEST_MODEL` to the path of an MTP GGUF on the same volume.
The test uses an in-process router and hard-links the model into a temporary package.

```powershell
cargo test --release -p cluaiz-api --test mtp_contract -- --ignored
```

## Live check at review (2026-11-20, laptop RTX 3050 4 GB)

Same patch, `cargo build -p cmd --release`, isolated `CLUAIZ_HOME`, port 8011, `CLUAIZ_MTP=on`,
`unsloth/Qwen3.5-2B-MTP-GGUF` Q4_K_M:

- `cluaiz run <model> --local -v -n 120 -p "Write a JSON list of 8 fruits with name and color."` printed
  `MTP 80% kept`; the log line said 411 of 513 guessed tokens accepted.
- `POST /v1/chat/completions` (not streaming) returned `usage.mtp = {"accepted": 2012, "drafted": 3211, "stopped": false}`,
  matching the log line "2012 of 3211 guessed tokens accepted (63%)".
- The streaming request finished on the server (log: 1699 of 2610 accepted), but the client command was cut
  before its final usage event was read, so the streaming field is not shown here.
- `max_tokens: 120` did not limit the answer of this thinking model (3,211 drafted tokens); this is the known
  pending max_tokens-with-thinking decision, not part of this patch.
