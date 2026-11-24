# Cluaiz Llama Engine (`engine/llama`)

Direct Rust API wrapping custom `llama.cpp` C/C++ memory. Compiled as an `rlib` to eliminate dual-static memory copies between Exe and DLL.

`native::speech::SpeechPlacement` accepts the runtime-selected layer count, independent projector device, KV/operation offload flags, generation/batch threads and mmap preference. Explicit CPU mode disables all GPU use. `planning_settings` and `backbone_workspace_bytes` share this owner's fixed context/batch/F16 layout with admission; codec workspace stays in the media policy. Successful `SpeechTimings` separate preparation, frame generation, native codec decoding and output (including consumer backpressure), excluding model load. These measurements are diagnostic, not realtime or peak-memory guarantees.

The native fork backports [upstream EmbeddingGemma-2 support](https://github.com/ggml-org/llama.cpp/pull/30054). The existing text embedding owner queries its projected output width (768 for the installed Q8_0 test package); it does not substitute the 512-wide hidden state. This does not expose GGUF image/audio embeddings through the API. `llama-revision.txt` pins generated fallback clones to the reviewed fork revision; a dirty fallback clone is preserved and refused rather than overwritten. An explicit local native checkout remains the development source.

## Architecture

```mermaid
graph TD
    App["cluaiz-runtime / cluaiz-api"] -->|Direct Rust Call| Engine["LlamaEngine"]
    Engine --> Native["NativeLlama / Stream / Sampler / Templater"]
    Native --> FFI["ffi/llama_cpp.rs"]
    FFI --> CustomFork["Pinned llama.cpp"]
    Headers["Pinned mtmd headers"] --> Bindgen["Build-time bindgen"]
    Bindgen --> MediaFFI["ffi/mtmd.rs: generated C API"]
    MediaFFI --> Mtmd["Static mtmd + vendor-hash"]
    Mtmd --> CustomFork
    Library["EngineHost / library tests"] --> Session["NativeSession"]
    Session --> Model["Owned NativeLlama"]
    Session --> Projector["Lazy Projector: freed before model"]
    Projector --> MediaFFI
    Source["Shared source resolver / decoder"] --> Prepared["Ordered RGB8 / PCM F32 parts"]
    Prepared --> Session
    Session --> Template["Existing native chat template"]
    Template --> Parts["Typed mtmd text / bitmap parts"]
    Parts --> Prefill["Native token / position budget and encoder prefill"]
    Prefill --> Loop["Existing sampler / reasoning / stream loop"]
```

## Components

| Module | Responsibility |
|---|---|
| [`native/core.rs`](src/native/core.rs) | Safe model instantiation, parameter loading, and memory envelope binding. |
| [`native/stream.rs`](src/native/stream.rs) | Token generation loop, chunked prefill, and reasoning control emission. |
| [`native/sampler.rs`](src/native/sampler.rs) | Llama.cpp native sampler chain (temp, top_p, min_p, penalties). The request's `logit_bias` (`ChatEnvelope.logit_bias`) is llama.cpp's own logit-bias sampler, added after a tool-call grammar and before the penalties and cuts, the place `common_sampler_init` gives it (`llama.cpp/common/sampling.cpp:330-343`, ahead of the default list that starts with penalties, `common/common.h:268`). Why there: a ban or a push has to change the scores before top-k/top-p/min-p cut the list, else a -100 token could already be the only one left, or a +100 token already cut. Ids outside the vocabulary are dropped, as llama-server does (`tools/server/server-schema.cpp:433`); the API refuses them earlier. Empty: no sampler is added. Checked live on 2026-11-23 (Qwen3.5-0.8B, greedy and `temperature: 0.7`; see `api/src/handlers/chat/README.md`). |
| [`native/templater.rs`](src/native/templater.rs) | Direct C-ABI Jinja chat templating from the custom fork. |
| [`ffi/llama_cpp.rs`](src/ffi/llama_cpp.rs) | Raw `extern "C"` bindings to compiled `llama.cpp` (including `llama_sampler_init_logit_bias` and the `LlamaLogitBias` mirror of `llama_logit_bias`). |
| [`ffi/mtmd.rs`](src/ffi/mtmd.rs) | Header-generated multimodal bindings, typed parts, token/position queries and evaluation helpers. |
| [`native/session.rs`](src/native/session.rs) | Paired model/projector lifetime, memory preflight, prepared media/text generation and pointer-free model telemetry used by EngineHost. |
| [`native/media.rs`](src/native/media.rs) | Private projector/bitmap/chunk owners, queried audio preparation, template association and native encoder prefill. |
| [`native/context.rs`](src/native/context.rs) | Shared native batch ownership and context shifting. |
| [`native/prompt_cache.rs`](src/native/prompt_cache.rs) | RAM prompt cache (llama-server `--cache-ram`): earlier prompts' sequence states and the selection rule; off unless `CLUAIZ_PROMPT_CACHE_MB` is set. |
| [`native/speech.rs`](src/native/speech.rs) | Experimental libmtmd speech generator, independent backbone/projector, vocabulary-derived language guard and optional authorized speaker PCM through the shared bitmap owner. |
| [`ffi/reasoning.rs`](src/ffi/reasoning.rs), [`ffi/reasoning_bridge.cpp`](src/ffi/reasoning_bridge.cpp) | Narrow C ABI to the pinned upstream reasoning-budget sampler; no copied budget state machine. |
| [`llama.cpp/`](llama.cpp/) | The underlying C/C++ inference engine with custom M-RoPE and chat C-ABI patches, the MoE expert prefetch and the load prefetch skip (see below); fork patches and their re-apply steps: [`CLUAIZ_PATCHES.md`](llama.cpp/CLUAIZ_PATCHES.md). |
| [`native/projection.rs`](src/native/projection.rs) | What a model and context will take per device, from llama.cpp's own no-alloc dry run (the one behind `--fit`), so the planner can correct its byte estimate (`project`, `ProjectedMemory`, `DeviceBytes`). Nothing is allocated. |
| [`native/load_steps.rs`](src/native/load_steps.rs) | What a chat-model load is doing, for an observer set on the loading thread (`observe`): weights read in percent (llama.cpp's `progress_callback`, set only when someone observes), a fallback llama.cpp's own load took, context creation, warmup. Used by `engine/runtime` load events. |
| [`native/tokenizer.rs`](src/native/tokenizer.rs) | One native tokenizer (`VocabHandle`) for chat and embedding contexts. |
| [`native/media_tests.rs`](src/native/media_tests.rs) | Tests of the media owners. |
| [`ffi/chat.rs`](src/ffi/chat.rs) | Narrow C ABI to `chat_bridge.cpp` for native tool calls: llama.cpp's common chat layer renders tools in the model's own format (`ChatTools::apply`), gives the lazy tool-call grammar, parses the output (`ChatParser::parse`, partial too) and turns a JSON schema into a grammar (`json_schema_grammar`). |
| [`native/speculative.rs`](src/native/speculative.rs), [`ffi/speculative_bridge.cpp`](src/ffi/speculative_bridge.cpp) | MTP speculative decoding, experimental. Settings `optimization.mtp`: `auto` (default: the runtime's load rules, then a guard in each answer that stops guessing where it is slower), `on`, `off` (load flag `--mtp`; `CLUAIZ_MTP=on` / `off` forces it on or off for tests (also `1`/`true`/`yes`, `0`/`false`/`no`; other values are logged and ignored); `CLUAIZ_MTP_DRAFT` = guesses per step, default 3 as llama-server; `CLUAIZ_MTP_PMIN` = least drafter probability, default 0). A model with NextN layers (Qwen3.5 / 3.6, GLM-4.5+, DeepSeek-V3, ...) is loaded with `load_mtp`; llama.cpp's own `draft-mtp` drafter (`common_speculative`) guesses the next tokens on an MTP context; `stream.rs` decodes the token and its guesses in one batch and keeps a guess only when the target's sampler picks it, so the answer is the target's. The drafter sees every target decode (prompt chunks too). Media prompts do not draft; a context shift stops drafting for that answer. Its memory is not in the load plan yet (Auto turns MTP off when it would not fit next to the plan). |
| [`moe_override.rs`](src/moe_override.rs) | `--n-cpu-moe N`: the routed experts of the first N blocks stay in RAM (a CPU buffer), everything else of those blocks (attention, shared weights, KV) on the GPU; the same tensor overrides llama.cpp builds (`CpuMoeOverrides`). |

## Why Cluaiz was slower than llama.cpp on Windows (2026-11-23)

**Cause.** llama.cpp's own Windows release builds the CPU backends (`ggml-cpu-*.dll`) and `ggml-base.dll`
with clang (`cmake/x64-windows-llvm.cmake`, the windows-cpu job of the fork's `.github/workflows/release.yml`,
with LLVM OpenMP `libomp.dll`); only `ggml-cuda.dll` is built with MSVC. Cluaiz builds all of llama.cpp with
MSVC (Visual Studio generator, `build.rs`), so its CPU backends are MSVC code using MSVC OpenMP
(`VCOMP140.DLL`). Same source, slower code.

**Proof** (run 47, `test/cloud/runs/2026-11-23-run47-laptop-warm-tg-gap/`, Ryzen 7 7435HS, 8 threads, medians of
rounds 2 and later). Swap tests: the same exe, only the CPU backend files changed.

| Test | llama-bench | llama-bench + Cluaiz's MSVC CPU DLLs | `cluaiz bench` (MSVC CPU) | `cluaiz bench` + llama.cpp's clang CPU DLLs and libomp.dll |
|---|---|---|---|---|
| CPU only, Qwen3-1.7B IQ4_XS, pp512 | 139.24 | 121.18 | 120.99 | 137.34 |
| CPU only, same, tg128 | 27.20 | 28.09 | 25.73 | 27.64 |
| MoE split, granite-3.1-3b-a800m IQ4_XS, all experts on the CPU, tg128 (cleanest of three runs) | 39.43 | | 38.43 | 39.70 |

The CPU-only prompt gap is settled; split-mode generation lost 3% to 20% in different runs on a machine shared
with other programs. The fork's `ggml-cuda.dll` inside official llama-bench lost nothing.

After the fix (run 54, `test/cloud/runs/2026-11-23-run54-windows-llvm-cpu-backends/`, 8 rounds, the quieter of two
parts): CPU only pp512 llama-bench 136.16, Cluaiz with this build 129.45, Cluaiz MSVC 117.39 (tg128 26.15 /
25.03 / 24.54); MoE split tg128 37.14 / 34.95 / 32.43. The gap went from 14% to 5% on CPU-only prompts; it is
not closed: llama.cpp's own clang 20 DLLs reached 137.34 inside Cluaiz in run 47, this clang 22 build 129.45
(compiler or OpenMP version, not measured).

**Fix (owner's choice, 2026-11-23): upstream's exact setup.** `build.rs` (`build_llvm_cpu_backends`): on
Windows, after the MSVC build, a second CMake build of the same tree with upstream's toolchain file
(`cmake/x64-windows-llvm.cmake`: clang, Ninja) and LLVM OpenMP builds `ggml-base` and the CPU variants; they
replace the MSVC ones beside the exe, with LLVM's `libomp.dll`. `ggml-cuda` and the rest stay MSVC, as upstream.
`libomp.dll` is required: `ggml-base.dll` imports it, so `cluaiz.exe` does not start without it (exit code
0xC0000135, run 47). It ships in the core (`.github/scripts/package_core.sh`, notice in
`THIRD_PARTY_NOTICES.md`) and the dev sync copies it (`cmd/src/core/bootstrapper.rs`, prefix `omp`, because the
sync compares names without their `lib` prefix). Differences from upstream's release, each with its reason:
- `OpenMP_libomp_LIBRARY` points at LLVM's `lib/libomp.lib`: CMake's own search took Visual Studio's
  `libomp.lib`, whose `libomp140.x86_64.dll` is not a runtime to ship (upstream downloads LLVM 20's instead,
  `GGML_OPENMP_FETCH`, which requires exactly clang 20).
- `-Wno-error=incompatible-pointer-types`: clang 22 stops on `_mm_prefetch(&x[ib] + ...)` in
  `ggml-cpu/arch/x86/quants.c`; upstream's CI uses clang 20, where it is a warning.
- The CPU variants are the ones the MSVC build makes, not upstream's longer clang list (that adds ivybridge,
  piledriver, cooperlake, zen4, sapphirerapids): clang 22 did not finish `amx/mmq.cpp` of the sapphirerapids
  variant in 30 minutes. Machines that would pick one of those get the same variant as before this change.
- Rejected: clang without OpenMP (`GGML_OPENMP=OFF`, no `libomp.dll`). MoE split generation fell to 9.31 t/s
  against 39.43, because ggml's own thread pool is then used and Cluaiz attaches none to its contexts
  (llama-bench and llama.cpp's common code do, `llama_attach_threadpool`).

**What happens if LLVM is missing.** clang is looked up in `CLUAIZ_LLVM_DIR`, then `PATH`, then
`C:\Program Files\LLVM\bin`; LLVM's `libomp.dll` / `lib/libomp.lib` and Ninja must be there too. Missing, or the
LLVM build fails: the build goes on with the MSVC libraries (today's speed) and prints
`CPU backends built with MSVC, slower than llama.cpp's release: <reason>`. `CLUAIZ_REQUIRE_LLVM_CPU=1` (set by
the release workflow) makes that a build error, so a release never ships the slow modules.

**Rule from now on.** Build each backend with the toolchain upstream's release uses for it. On every llama.cpp
upgrade, read upstream's `.github/workflows/release.yml` (toolchains and CMake options per backend) against this
`build.rs`.

**How to verify** (PowerShell, Engine root; a small model so RAM never limits the run; official llama-bench
unpacked in `%LOCALAPPDATA%\cluaiz-test\llama-official-b12428`):

```
powershell -ExecutionPolicy Bypass -File test\cloud\remote\warm_tg_gap\run_warm_tg_gap.ps1 `
  -Model <small IQ4_XS .gguf> -ExpectedSize 0 -LlamaPlacement '-ngl 0 -dev none' `
  -CluaizPlacement '--n-gpu-layers 0' -Cluaiz 'C|target\release\cluaiz.exe' -Rounds 4
```

`results.md` in the output folder lists pp512 and tg128 per config; `cluaiz bench` should be within the
round-to-round spread of llama-bench. A `target\release\ggml-cpu-haswell.dll` that names `VCOMP140.DLL` is an
MSVC module.

## Why `cluaiz bench` prompt speed was below llama-bench on a model read from the disk (2026-11-24)

**Shape.** Run 48 / run 59: GCP L4 with about 8 GB of VRAM left, 12 GB RAM cgroup, Qwen3.6-35B-A3B UD-IQ4_XS
(experts 14.5 GB, so they are read from the SSD), all experts in RAM (`--n-cpu-moe 41`), no MoE cache, 4 threads.
`cluaiz bench` was 0.90x of llama-bench (same fork) at pp512.

**Cause 1, fixed: different random tokens.** `NativeLlama::bench` (`src/native/core.rs`) drew its prompt tokens from
its own xorshift stream; llama-bench draws them from `std::rand()` without `srand`. When the experts come from the
disk, the tokens decide which experts each batch needs and so how many bytes are read, and each tool repeats its
own stream in every run. Same binary, only the stream swapped (run 59 part d, 2 rounds): pp512 45.37 / 45.19 with
the own stream, 48.82 / 49.12 with llama-bench's; pp2048 90.36 / 91.58 against 96.16 / 95.10.
**Fix:** `bench` now takes its tokens from the C library's `rand()` (the same call as llama-bench on each
platform), draws no number for the BOS slot and no generation token in a prompt-only run, as llama-bench does.
Only `cluaiz bench` changes; chat and serve never use this code. Alternative not taken: a seeded Rust copy of
glibc's generator (it would not match llama-bench on Windows, where `rand()` is MSVC's).

After the fix (run 59 part e, 2 rounds, same session; t/s):

| | pp512 | pp2048 | tg128 |
|---|---|---|---|
| before (Engine `03de2c96`) | 45.24 / 45.33 | 90.13 / 91.69 | 11.81 / 11.35 |
| after (`03de2c96` + this fix) | 49.56 / 50.61 | 92.03 / 86.37 | 12.07 / 10.65 |
| llama-bench (fork) | 50.76 / 51.13 | 94.83 / 99.59 | 11.79 / 13.04 |

pp512 went from 0.89x to 0.98x of llama-bench. pp2048 is inside the run-to-run spread on both sides (Cluaiz
86-92, llama-bench 95-108 over runs 48 / 59), so a 5-10% gap there is not proven either way.

**Cause 2, not changed: the load-time warmup.** `NativeLlama::warmup` decodes one full batch at load (so the
planner learns the GPU pool, `cluaiz_plan::margin`); llama-bench has no load-time warmup. With a test switch that
skipped it, the major page faults per run doubled to llama-bench's pattern (1.19-1.22 million against 0.50
million), the first prompt after load took 385 s instead of 195 s (llama-bench's first prompt: 355-361 s), and
pp512 rose by 1-3 t/s (run 59 parts c and e). The warmup makes the first real prompt about twice as fast on
this shape, so it stays.

**Not the cause** (run 59): memory left to the page cache (anon 410-440 MiB and file 11.7 GB in both tools), the
llama.cpp parameters (`n_ctx` aside, the same as llama-bench's; field by field in the run's SUMMARY).

**Test trap found on the way.** `CLUAIZ_MTP=off` did nothing then (the old `native::speculative::requested`
read only on / 1 / true; since fixed: `env_override` reads off / 0 / false / no as off). Run 48's `-c 4608` and run 59's `-c 2048` loaded MTP through Auto (no stored measurement for that
context), which explains their slower prompts and larger disk reads. Use `--mtp off` in tests.

**Rerun:** `test/cloud/remote/ssd_prefetch/pp_gap.sh` (configs and usage in its header; `analyze_pp_gap.py` splits
each run into phases by the time-stamped logs).

## Boundary with llama.cpp

- The only crate that depends on `llama-engine` is `engine/runtime` (`engine/runtime/Cargo.toml`); 9 of its
  files use `llama_engine` (`chat_memory.rs`, `embedding_host.rs`, `host.rs`, `lib.rs`, `load_events.rs`,
  `media_generation.rs`, `native_load.rs`, `speech_host.rs`, `speech_plan.rs`). `engine/models` names it only in
  a doc comment (`src/taxonomy/quantization.rs:343`). Checked with
  `grep -rl "llama_engine" --include=*.rs . | grep -v "^./target\|^./engine/llama/"` on 2026-11-23.
- The fork's own changes, with their re-apply steps: [`llama.cpp/CLUAIZ_PATCHES.md`](llama.cpp/CLUAIZ_PATCHES.md).
- A lost hook fails the build: the fork's `src/CMakeLists.txt` stops the configure step when the MoE prefetch
  call is missing from `llama-moe-cache.cpp` (patch 1 in `CLUAIZ_PATCHES.md`) or the load-range call is missing
  from `llama-model-loader.cpp` (patch 3).
- Upgrade checklist: re-apply the patches in `CLUAIZ_PATCHES.md`; compare the build toolchains and CMake options
  with upstream's `.github/workflows/release.yml` (section above); before a release, run the same-machine speed
  check against official llama-bench (command above, and a GPU placement).

## Native text embedding boundary

`native::embedding::EmbeddingModel` takes an embedding-enabled `NativeLlama`, retaining its existing model/context destructor. It queries native pooling and output width rather than selecting a model by name. Mean, CLS and last-token sequence pooling are accepted; no-pooling, reranking and encoder-decoder models return an error. Whole-sequence evaluation uses the existing RAII batch and tokenizer; inputs beyond the active sequence limit are rejected without truncation. Sequence output is copied before the next native operation and normalized through `cluaiz_core::contracts::normalize_embedding`. This boundary alone does not wire the HTTP endpoint or certify arbitrary GGUF exports.

```mermaid
graph LR
    Text["Bounded text"] --> Tokens["Existing native tokenizer"]
    Tokens --> Batch["Owned whole-sequence batch"]
    Batch --> Native["Selected GGUF context and native pooling"]
    Native --> Copy["Copy queried-width sequence vector"]
    Copy --> Normalize["Shared finite L2 normalization"]
```

The opt-in `tests/embedding.rs` fixture takes external model paths/text, checks repeated and distinct inputs, one supplied semantic ranking, normalized finite output, oversize refusal and recovery. This is selected-model acceptance, not general retrieval quality or GPU/platform certification. Speech integration additionally accepts `CLUAIZ_TTS_TEST_GPU_LAYERS` and `CLUAIZ_TTS_TEST_CODEC_GPU` for actual split-placement PCM/tempo/cancellation/recovery checks; neither is a production setting.

## GGUF speech ownership

```mermaid
graph LR
    API["Authorized text and optional decoded reference"] --> Speech["SpeechModel: own backbone/context/projector"]
    Speech --> Language["Actual codec-language tokens + data-owned ISO aliases"]
    Speech --> Bitmap["Shared Bitmap::from_audio: bounded mono/resampling"]
    Language --> Generator["libmtmd generator borrows strings and bitmap"]
    Bitmap --> Generator
    Generator --> PCM["Native cumulative PCM / new deltas only"]
    PCM --> Tempo["Shared tempo stream / original sample rate"]
    Tempo --> WAV["PCM callback and checked final WAV collector"]
```

`SpeechModel` does not borrow or evict the chat model. Speaker sources are authorized/decoded above this layer; it never opens arbitrary user paths. Native input rate is queried from the projector and shared PCM conversion/copy budgets apply. The reference bitmap and input strings outlive the generator. `speak_pcm` polls the existing public native helper every 16 generated frames and once at the end, validates its cumulative F32 layout/rate, and forwards only newly generated samples through shared tempo. Native decoder state is retained across flushes; more frequent flushing can affect quality/performance and requires model acceptance. The old `speak` entry collects the same output as a checked PCM16 WAV. Cancellation and callback errors stop at prompt/frame boundaries; a single native operation/load is not preempted. The generator is dropped before the projector/context/model; a frame-limit failure returns an error instead of truncated success (a streaming caller may already have received earlier valid parts). Language support is queried from actual codec-language vocabulary tokens; ISO aliases are data in `native/data/speech_languages.json`. The pinned API is experimental, and arbitrary voice/model/language quality needs native acceptance rather than a registry declaration.

The opt-in `speech_stream` integration test accepts `CLUAIZ_TTS_TEST_THREADS` (integer, default 2) and `CLUAIZ_TTS_TEST_GPU` (boolean, default false) alongside its externally supplied model, projector, text and output paths. It prints native load time and first-utterance synthesis time separately. These controls belong to the test harness; they do not change production placement or establish real-time audio quality.

## Native reasoning control

```mermaid
graph LR
    Request["Request-owned skip flag"] --> Session["Serialized NativeSession"]
    Session --> Poll["Generation checks before sampling"]
    Poll --> Gate["Upstream reasoning-budget sampler"]
    Gate --> Chain["Existing user sampler chain"]
    Chain --> Decode["Real tokens accepted and decoded"]
    Decode --> End["Actual template end marker"]
    End --> Answer["Same stream continues with answer"]
```

- **Tags and budget:** actual template/model tags and the existing context-aware token budget configure the upstream sampler. A prompt already inside reasoning starts counting immediately; otherwise the gate waits for a real start sequence. No model-name rules or fixed reasoning tags are used.
- **Ownership:** `SafeSampler` owns a composed chain: the reasoning gate precedes the existing already-prefilled user chain. The borrowed `ReasoningHandle` is used only on that generation thread while the root lives. Root destruction frees both samplers once. Initialization errors leave the old chain with its caller.
- **Auto effort:** after each answer of at least 32 tokens the stream keeps its decode speed (`NativeLlama.decode_per_second`, from `llama.timings()`), and the next Auto request gets that speed times 180 s as its thinking ceiling, between 1,024 tokens and High's share of the room (`ThinkingBudget::limit`). The first answer after a load has no speed and uses High's share. The ceiling only stops a long thought: the model still ends easy ones itself. Measured (Qwen3-0.6B Q4_K_M on the Windows dev machine's GPU, 16k context, 2026-11-21, run 30): 174-180 t/s gives about 31,000 tokens, so the room share (about 11,970) applied; the model used 285, 1,749 and 1,176 thinking tokens on three questions and ended each itself. The log line `Auto thinking ceiling` names the number and its reason.
- **Request `stop` strings** (`ChatEnvelope.stop`, OpenAI `stop`): answer text that could be the start of a stop string is held back (`stop_match` in `native/stream.rs`); at a match the text before it is sent, the rest is not, and `report.stop` is `stop` (`finish_reason: "stop"`). Thinking is not checked, only the answer. With native tool parsing the strings join the template's `additional_stops`. Why hold back: without it a client would already have received the first characters of the stop string. Like llama-server, which also holds partial stop matches. Run 34: `"5"` ended a 1-10 count at `4`; a two-string list stopped at the first match in the text; streaming gave the same text. Not tested: media requests (images, audio) with `stop`.
- **Gentle close:** a named effort's budget (`ThinkingLimit.tokens`) is a soft limit: once it is spent, thinking closes at the next token that ends a line (`ReasoningHandle::poll`, `line_done`). The sampler gets `tokens + grace` as its hard limit, where grace is a quarter of the budget, at most 500 (`THINK_GRACE_MAX`) and inside the room. Why: NVIDIA's Nemotron budget control ends at the next newline within 500 tokens; a cut mid-sentence leaves a broken thought. A number and Max keep an exact limit (grace 0), as asked. The forced text is `reasoning_budget_message`, else the managed prompt `chat.thinking_close` (Qwen's early-stopping sentence from the Qwen Quickstart, Thinking Budget), then the end tag. Run 31: `low` (2,048) closed at 2,069 and 2,124 tokens at a line end; a 300-token number closed mid-line with the sentence; all three answered. Alternatives not taken: telling the model its budget inside the thinking (Seed-OSS does this, but its model is trained for it; an untrained model may ignore or copy it), and a nudge before the limit (no source measured it).

  ![When the thinking budget runs out: finish the line (up to a quarter of the budget, at most 500 tokens), write the wrap-up sentence, then the end tag](../../docs/assets/api/thinking-close.png)

- **Skip:** `NativeSession` retains the request's `Arc<AtomicBool>`; no global skip pointer is used. Before sampling, the gate receives the manual-force request. It forces the configured optional message and real end sequence through normal accept/decode/history handling, then sampling continues. The API parser waits for that sequence rather than declaring reasoning complete itself.
- **Unclosed thinking:** when the model ends its turn (end of generation) while its thinking is still open, its answer or tool call is inside the thinking, where the parser does not take it. The gate then forces the same end sequence once and sampling continues, so the model writes its answer or call outside thinking (with the tool-call grammar awake). This happens at most once per generation.
- **Report:** `ThinkingReport.ended_by` distinguishes an accepted native `skip`, automatic `budget` forcing, thinking `closed` after the model ended inside it, and a natural `model` end. The HTTP control acknowledgment alone does not prove that the model has reached the answer.
- **Limits:** a skip is observed between native sampling/decode operations. It cannot make an in-flight kernel instantaneous or guarantee answer correctness, brevity or every model's instruction following. Cancellation remains a separate control. Platform/model coverage requires actual acceptance runs.

## Multimodal linkage contract

- **Build inputs:** `build.rs` reads this checkout's `mtmd-helper.h`, including `mtmd.h`, `llama.h` and ggml headers. It generates the allowlisted C API into `OUT_DIR`; no generated binding file is committed.
- **Build dependencies:** bindgen requires a discoverable libclang installation in addition to the existing CMake/C++ toolchain. `LIBCLANG_PATH` can point to its library directory. Cross builds require the target's headers/sysroot as well.
- **Static targets:** `LLAMA_BUILD_MTMD=ON`, `BUILD_SHARED_LIBS=OFF`. Cargo explicitly links `mtmd` and its `vendor-hash` dependency alongside the existing llama/ggml libraries. Header-only vendor dependencies are compiled into mtmd.
- **Standalone programs:** tools, examples, server and app remain disabled. `MTMD_VIDEO=OFF` disables mtmd's FFmpeg subprocess decoding; the shared media crate owns source decoding.
- **ABI:** parameter structs and callback signatures come from the same pinned headers used to compile C++. Generated size/alignment/offset tests check the Rust layout on the build target.
- **Native buffers:** `mtmd_bitmap_init` copies validated RGB8; `mtmd_bitmap_init_from_audio` copies PCM F32 samples. The latter expects audio already prepared for the loaded encoder's sample rate and mono layout. A bitmap must be freed exactly once; chunk lists have a separate native free function.
- **Failure boundary:** missing libclang, incompatible headers or unresolved static symbols fail the build. Raw FFI remains unsafe; callers must retain model/context lifetimes and serialize encoder evaluation.
- **Current limit:** prepared image/audio prefill and generation are connected to EngineHost and the existing chat API. Whole-message media history fitting, document/video providers and further platform acceptance remain P4/P8/P9 work. Runtime availability uses actual loaded encoder/pairing evidence; declarations alone do not initialize a projector.

## Paired projector lifecycle

- **Ownership:** `NativeSession` consumes a loaded `NativeLlama`. Its optional projector field precedes the model field, so the native projector and callback state are freed before their model. Native model pointers are not exposed mutably through this owner.
- **Reuse identity:** canonical projector path, size/modified time, actual encoder threads, GPU preference, RAM/VRAM buffer settings and weight/workspace estimates. Per-request decoded size is excluded, so a different image size alone does not reload the encoder. A different load identity unloads the previous projector before new allocations. Under host-RAM pressure, the idle projector is reclaimed before a fresh admission attempt; Chat model placement/context/KV remain unchanged. Stat checks cannot detect a same-size/same-time content replacement; metadata is checked again after load.
- **Budget:** `cluaiz_plan::media_budget` reuses existing safety-buffer policies against freshly queried free memory after old-projector unload. Unknown/zero free memory is not replaced by a stale cached value. Host cost includes weights, workspace and retained decoded copies; GPU cost also includes weights/workspace plus its safety buffer. CPU placement is chosen when the GPU estimate does not fit.
- **Request VRAM check:** before each request on a GPU projector, live free VRAM must cover weights + workspace + buffer minus what the projector already took since it was loaded (`projector_vram_held` in `native/session.rs`: the free-VRAM drop across `Projector::load`, plus the drop across each media request, capped at weights + workspace). Why: the encoder is loaded with `warmup = false`, so its compute buffers are allocated by the first encode and stay. The old check asked for the whole workspace (1 GiB default) plus the buffer free again, on top of what the projector already held. When the load plan had passed with a thin margin, every request after the first was refused with "Insufficient live VRAM for media request workspace", and the projector was never planned again (run 49: 1503 MiB free, 1024 + 512 MiB asked). The new rule is the load rule checked again with the projector's own memory added back, so it passes while nothing else took VRAM. When something else did (another model, the desktop), the reuse step in `ensure_projector` unloads the idle projector and plans it again from live free memory, as it already did for RAM, so the projector can move to the CPU instead of failing every later request. Not taken: `warmup = true` (llama-server's default) reserves the buffers at load, but only for the model's warmup size (for example 46x46 tokens in `mtmd-model.cpp`), so a larger image still grows them; reloading the projector on every refused request would also pass, but a projector that fits again would be reloaded on each request. Measured in run 50 (`test/cloud/runs/2026-11-23-run50-vision-vram/`, RTX 3050 4 GB, Qwen3.5-0.8B with mmproj-F16, 448x448 image): the load took 194-208 MiB (file 195 MiB), the first encode 41-45 MiB more, the second 4-7 MiB, later ones 0. With a 800 MiB workspace estimate the old rule would have asked 1312 MiB with 1295 MiB free before the second request; the new one asked 1266 MiB and all 5 requests answered. Limit: the drop is read for the whole GPU, so memory another process takes during a request is counted as the projector's (at most up to its estimate).
- **Estimate boundary:** callers supply non-zero weight/workspace estimates; weights must cover at least the file size, decoded data obeys media limits. This is a preflight estimate, not an OS reservation or a measured native peak. It does not eliminate races with other processes or promise allocation success. Encoder expansion/workspace still needs measurement with actual requests.
- **Encoder evidence:** successful `mtmd_init_from_file` performs upstream embedding-width checks. `mtmd_support_vision`, `mtmd_support_audio` and `mtmd_get_audio_sample_rate` then populate `ProjectorSupport`. These queries do not prove transcription quality, model answers or an enabled API route.
- **Cancellation/error:** a retained atomic flag is passed to the native load-progress callback. Native null results, invalid rates and changed-file checks return errors; local owners drop on Rust-side failure. Prepared generation probes free memory again for configured workspace and the additional geometry-derived native conversion peak; it no longer charges the full configured decoded ceiling. Already-resident source Arcs and weights are not counted as new allocations. Failed/cancelled prefill or a stopped consumer clears the partial model memory and returns an error. No automatic driver-failure retry is implemented here.
- **Lifecycle tests:** an ignored Rust test takes `CLUAIZ_MEDIA_MODEL` and `CLUAIZ_MEDIA_PROJECTOR`. It loads real weights on CPU and checks support against package declarations, same-session reuse, changed-thread reload, unload, missing file and paired drop. No complete native-leak/OOM assertion is made.

## Prepared media generation contract

- **Interface:** `NativeSession::stream_prepared` consumes `ChatEnvelope<Vec<PreparedPart>>`, the output token budget, the loaded model's `StructuralDNA`, request settings and a streaming consumer. It requires a successfully loaded paired projector. Source resolution stays in `cluaiz_media`; this native adapter receives decoded buffers only.
- **Audio:** common `prepare_mono` and `resample_linear` prepare finite PCM F32 at the encoder's queried rate. Channel mixing retains amplitude without automatic peak normalization. The native constructor copies samples; the pinned implementation uses a checked 32-bit sample count internally. The adapter does not duplicate audio decoding or implement the model's learned encoder.
- **Image:** validated RGB8 dimensions and byte layout go to the real copying bitmap constructor. Native mtmd owns model-specific resizing, patches and learned encoding. Unsupported loaded encoder inputs fail before evaluation.
- **Association:** collision-checked request placeholders keep ordered text/media positions through the existing chat template. Every placeholder must survive exactly once and in order. They are replaced with typed text/bitmap parts before tokenization; literal user text such as `<__media__>` is not interpreted as a bitmap delimiter.
- **Text parts:** consecutive text parts of one message are joined with a newline, and none is put next to a media placeholder, as llama-server joins content parts (`concat_content_parts` in the pinned `common/chat.cpp`). Before run 50 they were glued together, so the Engine's `<tools_hint>` block (added in front as its own part) ran straight into the question (`</tools_hint>What colour is this image?`). With the text part before the image, Qwen3.5-0.8B on a CPU projector then answered that it cannot see images (7 of 7 requests on fresh servers); after the change it answered "The image is red." (7 of 7). Image-first requests are unchanged (the image sits between the two texts).
- **Accounting:** aggregate retained media and conservative conversion/copy peaks obey the configured decoded ceiling. Native token and position queries, rather than pixel/sample counts, determine context usage. The larger native count must fit `n_ctx - (max_tokens + 16)`. On overflow, the configured context-shifting policy may remove complete oldest turns using the shared core turn selector, then count the remaining prompt again before learned encoding. The leading system message and the latest user turn/attachments (including its tool continuation) stay intact; Off refuses overflow. Removed native bitmaps are freed and retained IDs still refer to the original uploaded parts. Context reports include removed messages and the measured reduction in occupied positions. A current document/image that cannot fit is refused explicitly with active/input/output counts; no silent content truncation, context increase or multimodal KV shift is used.
- **Prefill:** the adapter orchestrates pinned native `mtmd_encode_chunk`, `mtmd_helper_decode_image_chunk` and text `mtmd_helper_eval_chunk_single` calls in the original order. Upstream still owns positioning, batching and attention behavior. Post-decode callbacks expose actual processed embedding batches; encoding counts represent native slices associated by copied bitmap IDs with the uploaded part. Last-text-chunk logits enter the existing sampler/reasoning/output loop. Raw bitmap copies are released after prefill; source Arcs needed by API/tool continuations remain owned. Sampler history receives real text tokens, without invented media token IDs.
- **Tools on image and audio requests** (since run 68, 2026-11-24): the envelope's function schemas (`ChatEnvelope.tools`) go to the template through llama.cpp's chat layer exactly as for a text request, with the same lazy call grammar and parser; the media parts are markers in the text, as llama-server renders a multimodal request (`oaicompat_chat_params_parse` in `tools/server/server-common.cpp` loads the media into markers, then applies the template with `inputs.tools`). Before, `native/stream.rs` left the tools out whenever the prompt had media (`req_tools.filter(|_| !is_media && !is_pivot)`, from the October native-tools commit `4dc7016b`, when the inline tool form still existed for that case), and the API built the media envelope before it added the tools (`tools: None` in `preflight.rs`). An image request then had no function schemas at all: run 64 measured 415 prompt tokens and 0 / 10 calls with the image against 933 and 10 / 10 without. After the fix (run 68, `test/cloud/runs/2026-11-24-run68-tools-schema-fixes/`, Windows laptop, RTX 3050 4 GB, Qwen3.5-2B Q4_K_M + mmproj-F16, temperature 0, 10 requests per case): image + `get_weather` 0 / 10 calls with 225 prompt tokens before, 10 / 10 with 482 after (log: `[NativeTools] Format peg-native | grammar on (lazy true, 1 triggers)`); the same question without the image 10 / 10 both times (284 tokens); the red-image question without tools 10 / 10 both times. Continuations (pivots) still use the inline form.
- **Cache boundary:** media turns accept no text-only prefix cache and return no prefix token history from `NativeSession`. An existing text-only caller retains its original API and prefill path. Agent continuation and mixed-history cache reuse are not yet integrated.
- **Budget boundary:** conversion accounting retains the full original source-buffer footprint while native copies coexist, including API-held Arc clones. Rendered text/template allocations and native workspace expansion are covered only by the conservative workspace policy, not measured individually. Caller-supplied workspace estimates are not measured peaks or OS reservations. Source decode/tokenization cancellation latency and GPU/Linux/macOS behavior remain unmeasured.
- **Observed audio proof:** on Windows CPU, the ignored `test_session_media_online_audio_and_text_regression` loaded the installed Gemma E4B Q4_K_M plus BF16 companion and fetched a public [PyTorch speech demo](https://docs.pytorch.org/audio/2.5.0/tutorials/speech_recognition_pipeline_tutorial.html). Actual model output was `i had that curiosity beside me at this moment`, matching the published transcript. Native logs showed 85 audio tokens. The same test rejected an excessive output reservation and a stopped consumer, then successfully ran text chat again: `1 passed; 0 failed`, 38.86 seconds. Model/projector/demo URL and expected text come from test environment variables; the expected transcript is never supplied to the model prompt.
- **Observed image proof:** `test_session_encoded_images_change_answers_and_recover_text` used the same installed CPU model/projector pair and the same question for three encoded 128x128 fixtures: red PNG returned `Red`, blue PNG returned `Blue`, and blue JPEG returned `Blue`. Actual data URIs pass through `SourceResolver`, `prepare_chat`, the shared RGB decoder, native image encoder and the existing generation loop. Native logs show 81 image tokens per fixture. Ground-truth colors are asserted only in the test, never supplied in the prompt. The temporary resolver directory is empty after requests and plain text generation succeeds afterward: `1 passed; 0 failed`, 61.13 seconds. Image encoding, base64 and temporary-directory dependencies are test-only; no production decoder/encoder was added by this test.
- **HTTP evidence:** `api/tests/media_contract.rs` additionally exercises actual loopback JSON/SSE chat routes, including one image plus inline audio request and image-preserving continuation. The final native CPU run passed in 104.92 seconds with fixture-specific configured workspace/decoded ceilings; see the API README for scope and resource refusal evidence.
- **Proof limits:** the speech and solid-color fixtures do not certify App UI behavior, photographs, multiple images in one turn, full transcript accuracy across languages, driver failure recovery or full model/platform coverage. Upstream logs label audio experimental.

**Pending (projector VRAM and text parts):**

| Item | Why it matters | Needs |
|---|---|---|
| Text before the image with the Engine's tool hint, GPU projector | Qwen3.5-0.8B still answered "I cannot identify the color of an image" (9 of 9 in run 50); with `tool_choice: "none"` on the GPU, or on a CPU projector, it answered red; official llama-server with the same system prompt and hint made tool calls in both orders, so the hint itself steers a 0.8B model | the owner's call whether image requests get the tool hint; the same check with a larger vision model |
| The reclaim path after a failed VRAM check | written to move a GPU projector to the CPU when other memory took VRAM; not hit in run 50 | a run that loads another GPU model between two image requests |
| A larger image after small ones | the growth of an encode is counted after it ran, so the first large image is checked against the remaining estimate only | a run with 448 px then 1792 px images on a GPU projector |
| Unit test for the request VRAM rule | the llama suite is not run on the dev machine | CI |

## llama.cpp log lines

`route_llama_logs` (`native/core.rs`) sends every llama.cpp / ggml (and mtmd) log line to the Engine log
through `llama_log_callback`: level 4 = error, 3 = warn, the rest = debug (so they show with `-vv`; the log
file `cluaiz_Core.log` has them at `-vv`). Before that, `NOISE_PATTERNS` drops the lines that repeat once
per tensor, GGUF metadata key, layer, mapped buffer, graph node or token, and the load progress dots:

| Pattern | llama.cpp source | Why |
|---|---|---|
| `create_tensor:`, `buffer type overridden to` | `llama-model-loader.cpp` | one line per tensor (515 + 48 for OLMoE) |
| `llama_model_loader: - kv ` | `llama-model-loader.cpp` | one per GGUF key; Cluaiz reads the header itself (`GgufProber`) |
| `assigned to device `, `llama_kv_cache: layer ` | `llama-model.cpp` `load_tensors`, `llama-kv-cache.cpp` | one per layer |
| `CPU_Mapped model buffer size` | `llama-model.cpp` `load_tensors` | one per mapped buffer (33 for OLMoE); the GPU buffer line stays |
| `llama_graph_n_input_tensors:` | `llama-context.cpp` | one per graph node at every graph reserve (602 lines for one OLMoE load) |
| `CUDA Graph`, `ggml_backend_cuda_graph_compute` | `ggml-cuda.cu` | per token |
| a line of only dots (or empty) | `llama.cpp` load progress | one `.` per percent |

| Decision | Why | Alternative not taken |
|---|---|---|
| Specific patterns, no `"."` | the old list had `"."` (added with `eprintln!` output in eb18953a, 2026-08-17, to hide the progress dots), so every line containing a dot was dropped: the MoE cache size line, model / KV / compute buffer sizes, `offloaded 17/17 layers to GPU`, the backend paths, the GPU line (`compute capability 8.9`) | keep `"."`; or drop no line at all (the per-node dump alone doubles the log) |
| The broad `load_tensors:` and `llama_kv_cache` became their per-layer / per-buffer lines only | the summary lines of both (layers offloaded, buffer sizes) explain a placement; the per-layer lines repeat it | keep hiding both families |
| Debug level, not info | unchanged; the lines are for debugging a load | info level would put llama.cpp's load report into every normal log |

**Measured (2026-11-22, Windows 11, RTX 3050 4 GB, `target\debug\cluaiz.exe serve -vv`, isolated
`CLUAIZ_HOME`, port 8014, OLMoE-1B-7B Q4_K_M from the test install home, one load + one chat at
temperature 0):**

| Build | `cluaiz_Core.log` lines | of them `[llama.cpp]` |
|---|---|---|
| Old list (with `"."`) | 769 | 693 (602 of them `llama_graph_n_input_tensors`) |
| No pattern at all (temporary build, not committed) | 1,466 | 1,390 |
| New list | 207 | 131 |

With the new list, 40 distinct llama.cpp lines appear that the old list hid (for example `MoE cache size =
1642.03 MiB for 3720.00 MiB of host experts, 80% of the misses on the CPU`, `CUDA0 KV buffer size`,
`offloaded 17/17 layers to GPU`, `load_mode = mmap`); the only family the old list showed and the new one
hides is `llama_graph_n_input_tensors`. No `ERROR` or panic in the log. The same load + chat with other
models (other architectures print other per-layer lines) was not run. Rerun: start `cluaiz serve -vv` on an
isolated home, load a model with one chat, count `[llama.cpp]` lines in `cluaiz_Core.log` and group them by
their first word. No unit test: the llama suite is not run on the dev machine.

**Failure and recovery:** a llama.cpp merge that renames a logged phrase makes that family show again
(more lines, nothing lost); a new per-tensor line in a merge shows up the same way and needs a pattern.

**Pending:**

| Item | Why it matters | Needs |
|---|---|---|
| Lines from other architectures, Vulkan, Metal, multimodal (mtmd / CLIP) loads | only an OLMoE load on CUDA was counted | the same count on those loads |
| `print_info:` (58 lines per load, fixed, not per tensor) | kept: it is llama.cpp's model summary | the owner's call if the log should be shorter |

## RAM prompt cache (off by default)

**What it does:** [`native/prompt_cache.rs`](src/native/prompt_cache.rs) is llama-server's `--cache-ram`
(`tools/server/server-task.cpp`, `server_prompt_cache`). Before a new prompt overwrites the context, the
sequence state of the old one (`llama_state_seq_get_data_ext`) is kept in RAM; then, if an earlier prompt
shares more tokens with the new one than the current context does, its state is restored
(`llama_state_seq_set_data_ext`) and only the rest is computed. The code is in `stream.rs`, just before the
common-prefix reuse.

**Why:** run 40 (`test/cloud/runs/2026-11-22-run40-cluaiz-ollama-website/README.md`, Angle 2): with 200
repeated tickets, llama-server reused a whole earlier prompt in 94 of 219 requests (Ollama) and 3,019 of
6,350 (llama.cpp), while Cluaiz compared only with the last prompt. That was the source of Ollama's lead
with 4 requests at once.

**Decisions:**
- Selection follows llama-server's rule (`server_prompt_cache::load`): an entry wins only if it keeps more of
  itself and covers more of the new prompt than the current context; an entry that would keep less than a
  quarter of itself is skipped.
- Size `CLUAIZ_PROMPT_CACHE_MB`, **default 0 = off** (owner, 2026-11-22). One state is large (below), so the
  cache would raise RAM above the published numbers; llama-server's default (8192 MiB) is not taken for the
  same reason. Oldest entries go first; a state larger than the whole limit is not kept.
- Only for a plain KV: sliding-window, recurrent and hybrid models and MTP's draft context do not come back
  whole from one sequence state, so the cache is skipped for them.

**Tested** (2026-11-22, Windows 11, debug build, OLMoE-1B-7B Q4_K_M, isolated home on port 8012, 3 tickets
sent 8 times in a fixed order with a ~260-token shared instruction): time to first token for a repeated
ticket was 1.0-1.3 s with the cache off, 0.18-0.23 s with 256 MB (3 restores; only 2 states fit) and
0.16-0.19 s with 1024 MB (5 of 5 repeats restored). One state was 103-123 MB. Answers were the same in all
three runs. Log line: `🗂️ [PromptCache] Restored an earlier prompt: ...`. Rerun: `CLUAIZ_PROMPT_CACHE_MB=1024`
on `cluaiz serve`, then send the same prompts twice in a different order.

## MoE expert prefetch (llama.cpp fork, off unless the plan sets a cap)

**What it does:** with llama.cpp's MoE GPU cache on, each layer's selected experts that are not in the
cache are uploaded from the host experts, which are mmap'd from the model file
(`llama.cpp/src/llama-moe-cache.cpp`, `copy()`). When those pages are not in RAM, the upload page-faults
them in one after another. With the prefetch on, the fork first passes the address ranges of those experts
(every fill and every miss left to the CPU, for each expert tensor, up to a cap) to llama.cpp's own
`llama_prefetch` in one call, so the OS can read them together (Windows: one `PrefetchVirtualMemory`
call; Linux: one `madvise(MADV_WILLNEED)` per 128 KiB window). Then the upload runs as before. On Windows each
expert slice is asked for once per context (see the decision below); Linux asks every time.

```mermaid
graph LR
    Plan["cluaiz_plan::expert_prefetch: expert_prefetch_bytes"] --> Load["runtime native_load.rs after the load"]
    Load --> Set["NativeLlama::set_moe_prefetch"]
    Set --> Setter["fork: llama_cluaiz_set_moe_prefetch(ctx, cap)"]
    Copy["fork: llama_moe_cache copy(), selected experts known"] --> Hook["llama_cluaiz_moe_prefetch: ranges up to the cap"]
    Setter -.->|"cap per GPU backend"| Hook
    Hook --> Prefetch["llama_prefetch (upstream)"]
    Hook --> Upload["upload loop, unchanged"]
```

**Where the code is:** fork files `include/llama-cluaiz.h`, `src/llama-cluaiz-prefetch.{h,cpp}`, two marked
lines in `src/llama-moe-cache.cpp` and the source entry plus a guard in `src/CMakeLists.txt`; the list with
re-apply steps is [`llama.cpp/CLUAIZ_PATCHES.md`](llama.cpp/CLUAIZ_PATCHES.md). Rust: the declaration in
[`ffi/llama_cpp.rs`](src/ffi/llama_cpp.rs) and `NativeLlama::set_moe_prefetch` in
[`native/core.rs`](src/native/core.rs), which keeps the value, drops the fork's entry before `llama_free`
(Drop and `resize_context`) and sets it again on a resized context. The decision is the planner's
(`engine/plan/README.md`, "MoE expert prefetch"); `engine/runtime/src/native_load.rs` is the only caller.

**Decisions and why:**

| Decision | Why | Alternative not taken |
|---|---|---|
| A setter in a fork-only header, no field in `llama_context_params` | upstream appends fields to that struct; every merge would conflict and the hand-written mirror in `ffi/llama_cpp.rs` would drift | a new `moe_prefetch_bytes` field (first idea in the research plan) |
| Fork table keyed by the context's GPU backends | `copy()` only receives its backend and `llama_context` keeps its MoE cache private; the setter reaches the backends through the public `get_sched()` | a field in the MoE cache class (an upstream struct) |
| Rust sets 0 before `llama_free` | a freed backend's address may be reused by a later context, which would then inherit the cap | no cleanup (stale entries) |
| Reuse `llama_prefetch` and the cache's own expert addresses (`w->data + expert*nb[2]`) | one owner: no second GGUF offset parser, cache, thread or CUDA call; the old Rust streaming code had its own offsets and was never connected (research plan, section 2) | own reads into a host buffer (a later phase) |
| Linux window steps in the fork file, `llama_prefetch` not edited | upstream PR #32515 (open on 2026-11-22): one `MADV_WILLNEED` reads at most one readahead window | editing `llama_prefetch` (a third upstream file to carry) |
| The build fails when the call is gone | a merge that drops the call would otherwise turn the feature off without a sign | a runtime check |
| Advisory, off by default | `llama_prefetch` only logs a warning on failure and the upload then reads as before | treating a failed prefetch as an error |
| Windows asks each expert slice once per context (`ask_once`) | run 44 asked 173-215 GB per run for 11.8 GB of experts, again and again for resident pages; `PrefetchVirtualMemory` costs about 43 us per MiB in the working set and 85 us per MiB on standby on the calling thread and saves nothing for them (run 60 micro-benchmark). A replay of the call pattern: asked bytes -64%, prefetch time -48-68% (run 60) | a byte cap per step (still re-asks resident pages); a residency check with `QueryWorkingSetEx` (one more call per range, standby pages look not resident) |
| The fork only obeys the cap; when to prefetch is decided by the planner | one owner per rule: Auto's rule (cache share and platform, runs 43 and 44) lives in `engine/plan/src/expert_prefetch.rs`, so changing it needs no llama.cpp rebuild | a run-time guard in the fork (hit rate or bytes per token); not needed while the plan-time rule separates the measured runs |

**Tested** (2026-11-22, Windows 11, debug build `cargo build -p cmd`, RTX 3050 Laptop 4 GB, CUDA,
OLMoE-1B-7B-0125-Instruct Q4_K_M 4.2 GB, isolated home on port 8014, the planner's own MoE split: every expert
in RAM, `n_cpu_moe = 16`, MoE cache 1.22-1.51 GB depending on the free VRAM at each load, CPU share 0.8).
The same prompt twice per server ("Explain in three sentences why the sky is blue.", temperature 0, seed 42,
96 tokens; the second with `keep_alive: 0`):

| Mode | Answer (sha256 of the text, first 16 hex) | Fork log lines |
|---|---|---|
| Setting Auto, env not set | `852d80ab80084db0` both answers | none |
| `CLUAIZ_EXPERT_PREFETCH=off` | `852d80ab80084db0` both answers | none |
| `CLUAIZ_EXPERT_PREFETCH=on` | `852d80ab80084db0` both answers | `on, at most 1414 MiB per MoE cache upload`; `first prefetch: 153 ranges, 203184 KiB`; at unload `off: 2105 calls, 19272 ranges, 23387 MiB prefetched` |

No warning, error or crash in any of the three logs. The model fits in RAM on this machine, so this proves
that the call runs, changes no answer and stays silent when off; it says nothing about speed. The fork's
log lines reach the Engine log at debug level (`serve -vv`). Configure guard: with the call removed from
`llama-moe-cache.cpp`, `cmake -S engine/llama/llama.cpp` stopped with `cluaiz: src/llama-moe-cache.cpp lost
the llama_cluaiz_moe_prefetch call in copy()` (exit 1). Rerun: build, then start `cluaiz serve -vv` on an
isolated `CLUAIZ_HOME` with and without `CLUAIZ_EXPERT_PREFETCH=on`, chat with an MoE model whose plan shows
`MoE cache`, compare the answers and grep the log for `llama_cluaiz_moe_prefetch`. The llama crate's own
tests were not run (the llama suite is not run on the dev machine).

**Failure and recovery:**
- `PrefetchVirtualMemory` or `madvise` fails: `llama_prefetch` logs a warning (one per failed call, upstream
  behaviour) and returns; the upload page-faults the experts as without the prefetch.
- The context has no MoE cache: the cap would be stored but `copy()` never runs, so nothing happens;
  `native_load.rs` does not set it then.
- A merge loses the call: the configure step fails (above). A merge renames `fills`, `cpu` or `l.experts`:
  the call does not compile.

**Sources:** pinned `llama.cpp/src/llama-moe-cache.cpp` (`copy()`), `src/llama-mmap.cpp` (`llama_prefetch`),
`src/llama-impl.cpp` (`llama_prefetch_rows`, the same pattern); llama.cpp PR #32515 (readahead window),
https://github.com/ggml-org/llama.cpp/pull/32515.

**Pending:**

| Item | Why it matters | Needs |
|---|---|---|
| Ask-once on a live load with a MoE cache (Windows) | run 61's 9 cache runs got no cache (1.0-2.2 GB of VRAM used by others); run 60's replay found no prefetch still fastest with resident pages, so Windows Auto stays off | a laptop run with about 2.7 GB of free VRAM |
| Speed with a model larger than RAM | the reason for the feature; not measured | Phase 0/2 on the L4 with gpt-oss-120b, page cache dropped, 3 rounds, against official llama.cpp |
| Linux window steps | written from #32515, not run | a Linux run with a model larger than RAM |
| macOS, Vulkan, Metal, CPU only | Metal and CPU only have no MoE cache upload; Vulkan's MoE cache is untested | runs on those machines; a CPU-only path is a later phase |
| One warning per failed call | could repeat for every layer if the OS keeps refusing | a counter in the fork file if it is ever seen |
| `docs/engine/moe_ssd_streaming.md` and `docs/engine/llm_optimization.md` do not mention it | public docs; the owner decides | the owner's word after a measured run |

## Load prefetch skip for CPU-mapped weights (llama.cpp fork, off unless the plan turns it on)

**What it does:** at load, llama.cpp asks the OS to read the whole model file (Windows: one
`PrefetchVirtualMemory` over the file; Linux: `MAP_POPULATE` plus `POSIX_MADV_WILLNEED`,
`llama.cpp/src/llama-mmap.cpp`, from `init_mappings(true, ...)` in `src/llama-model.cpp`). When the model does
not fit in free RAM, most of what that reads is evicted again before it is used. With the skip on, the fork
passes every weight that stays mapped from the file in the CPU's default buffer type (the MoE experts kept on
the CPU) to `llama_mmap` as a lazy range, upstream's own way to leave a range out of that prefetch. Weights
copied to the GPU at load keep the prefetch. The skipped pages are read on first use.

```mermaid
graph LR
    Plan["cluaiz_plan::load_prefetch::lazy_host_weights(plan)"] --> Rt["runtime native_load.rs: plan reads from disk?"]
    Rt -->|yes| Set["llama_cluaiz_set_lazy_host_weights(true), this thread"]
    Set --> Load["NativeLlama::load"]
    Load --> Init["fork init_mappings: llama_cluaiz_mmap_lazy_ranges"]
    Init --> Mmap["llama_mmap: prefetch the complement only (upstream)"]
    Load --> Reset["llama_cluaiz_set_lazy_host_weights(false)"]
```

**Where the code is:** fork files `include/llama-cluaiz.h` (setter), `src/llama-cluaiz-load.{h,cpp}`, two marked
lines in `src/llama-model-loader.cpp` and the source entry plus a second guard in `src/CMakeLists.txt` (patch 3
in [`llama.cpp/CLUAIZ_PATCHES.md`](llama.cpp/CLUAIZ_PATCHES.md)). Rust: the declaration in
[`ffi/llama_cpp.rs`](src/ffi/llama_cpp.rs); the call in `engine/runtime/src/native_load.rs`; the rule in
`engine/plan/src/load_prefetch.rs` (`engine/plan/README.md`).

**Decisions and why:**

| Decision | Why | Alternative not taken |
|---|---|---|
| Reuse upstream's lazy ranges in `llama_mmap` | upstream already skips them in all three prefetch calls; the fork adds ranges, no new mapping code | a new `prefetch` argument path (edits `llama-mmap.cpp` and `llama-model.cpp`) |
| Not upstream's `lazy_mode` | it applies only to tensors the architecture marks `TENSOR_READ_LAZY` and moves them to a row-gather path; there is no upstream setting for the load prefetch (`competitor-repos/llama.cpp` at `d3c3e24`) | `--lazy on` style flags (would not touch experts) |
| Only the CPU's default buffer type | that is the one buffer that wraps the mapping itself (`is_default_buft` in `llama-model.cpp`); every other buffer copies the data at load, where the prefetch helps | every host tensor (pinned host buffers would page-fault their copy) |
| A thread-local setter, set and reset around the load | no field in `llama_model_params` (the Rust mirror would drift on every merge); the load runs `init_mappings` on the calling thread, so a concurrent load on another thread is not affected | a global flag (a second load at the same time would inherit it) |
| The build fails when the call is gone | a merge that drops the line would silently turn the skip off | a runtime check |

**Measured** (run 61, `test/cloud/runs/2026-11-24-run61-laptop-lazy-load/SUMMARY.md`; Windows laptop, RTX 3050
4 GB, Gemma 4 26B A4B IQ4_XS 14.2 GB, free RAM 8.0-13.9 GB before each run, release build, `-p 512 -n 128 -r 1`,
`--n-cpu-moe 30`, 9 rotated pairs with and without `CLUAIZ_LAZY_HOST_WEIGHTS=on`): the fork logged `11797 MiB of
CPU-mapped weights left out of the load prefetch` (every expert). Medians: load 8.0 -> 4.3 s, disk read during the
load 2.90 -> 0.80 GB, pp512 80.71 -> 104.43, tg128 15.77 -> 15.54 t/s (ranges overlap). In the 2 pairs where free
RAM fell to 0 MiB the prompt pass with the skip was 7-12x slower (4.53 and 7.63 against 52.90 and 57.18 t/s): the
experts were then read by page faults, which the eviction repeated. So the plan keeps it off (Auto off,
`engine/plan/README.md`); the switch stays for further runs. Configure guard: the block in `src/CMakeLists.txt`
matches `llama_cluaiz_mmap_lazy_ranges(*this, idx)` in `llama-model-loader.cpp` (same pattern as patch 1, whose
negative test is above). Negative test (2026-11-24): with the call replaced by `lazy.for_file(idx)`,
`cmake -S engine/llama/llama.cpp -DGGML_CUDA=OFF` stopped with `cluaiz: src/llama-model-loader.cpp lost the
llama_cluaiz_mmap_lazy_ranges call` (exit 1); the file was restored.

**Failure and recovery:** the skipped pages are read by page faults on first use (upstream's lazy-range
behaviour); nothing fails. A merge that loses the call stops the configure step; a renamed loader member stops
the compile. `CLUAIZ_LAZY_HOST_WEIGHTS=off` turns it off for every load.

**Pending:**

| Item | Why it matters | Needs |
|---|---|---|
| Auto on for disk plans | halves load time, but 7-12x slower prompts at 0 MiB free RAM (run 61) | a free-RAM guard, or the skip together with the ask-once expert prefetch on a MoE cache |
| Linux | lazy ranges also get `POSIX_MADV_RANDOM` there (no read-around on faults); not measured | a Linux run with a model larger than RAM |
| Upstream | a load-prefetch setting upstream would remove patch 3 | an upstream issue or PR (owner's call) |

## MTP speculative decoding (experimental)

- **Status:** built and measured on CUDA (RTX 3050 Laptop 4 GB on Windows, L4 on Linux). Default
  `optimization.mtp = "auto"`: the runtime decides per load (`engine/runtime/src/native_load.rs`
  `mtp_mode`) and a guard in each answer stops guessing where it is slower (`native/stream.rs`, "MTP
  guard"); `on` and `off` force it (load flag `--mtp`; `CLUAIZ_MTP=on` forces on for tests, without
  the guard, and `CLUAIZ_MTP=off` forces off). Measuring tool `cluaiz bench <model> --mtp-compare` (off, on and auto per round).
  Records: runs [22](../../test/cloud/runs/2026-11-19-run22-laptop-mtp/SUMMARY.md),
  [23](../../test/cloud/runs/2026-11-20-run23-mtp-placements/SUMMARY.md),
  [24](../../test/cloud/runs/2026-11-20-run24-mtp-auto/SUMMARY.md).
- **Files:** `native/speculative.rs` (Rust owner `MtpDrafter`), `ffi/speculative_bridge.cpp` (C ABI),
  `native/stream.rs` (prefill hook, draft and check in the generation loop), `native/core.rs` (load),
  `native/sampler.rs` (`sample_token_at`).

### What it does

Some models ship multi-token-prediction ("NextN") layers trained to predict tokens further ahead:
Qwen3.5 / 3.6, GLM-4.5 and later, DeepSeek-V3, Step-3.5, MiMo and others (`src/models/*.cpp` in the
pinned llama.cpp lists the graphs that use them). The NextN layers guess the next few tokens; the
target model checks all guesses in one decode; every guess its own sampler would have picked is a
token gained without a decode of its own. A guess is kept only when the target's sampler picks the
same token, so the answer is the target's own. It is not always byte-identical to a run without MTP:
the target computes a batch of 2-4 tokens with slightly different floating-point rounding than one
token at a time, and where two tokens are nearly tied a greedy pick can flip. Measured below: 2 of 4
`bench --mtp-compare` prompts changed text; llama-server's own runs with and without MTP differed too.

```mermaid
graph LR
    P["Prompt chunk decoded (target)"] --> H["Drafter reads the target's hidden rows (cluaiz_mtp_process)"]
    T["Token t sampled"] --> D["Drafter guesses d1..dk (cluaiz_mtp_draft)"]
    D --> B["One target decode: t, d1..dk, logits for every row"]
    B --> H2["Drafter reads this batch too"]
    B --> S["Sample row 0, 1, ...: keep d_i while the target picks it"]
    S -->|"first refusal or all kept"| R["KV of target and drafter cut after the last kept token"]
    R --> T
```

### Decisions and why

| Decision | Why | Alternative not taken |
|---|---|---|
| Use llama.cpp's own drafter (`common_speculative`, type `draft-mtp`) through a C bridge | Same code as `llama-server --spec-type draft-mtp`; head modes per model (Gemma 4 shared KV, Step-3.5 chained heads, Qwen single head) stay upstream's job and follow llama.cpp updates | Re-implementing the drafter in Rust: duplicate logic that drifts from upstream |
| Target decode and the check stay in Rust (`stream.rs`) | The Engine's sampler chain (reasoning budget, lazy tool grammar, JSON schema check, penalties) must judge every token; llama.cpp's `common_sampler` is not the Engine's chain | `common_sampler_sample_and_accept_n`: would bypass the Engine's sampler |
| Each kept guess goes through the normal per-token path before the next row is sampled | Grammar and thinking state change after a token (the end of thinking wakes the tool grammar); sampling row i+1 before handling token i would use stale state | Sampling all rows first, then emitting |
| Load with `load_mtp` only when MTP is asked for | NextN tensors are skipped unless asked (`llama-model.cpp:4118`); loading them always costs memory for nothing | Always load |
| Target context gets `n_rs_seq = draft size` | Hybrid / recurrent models (Qwen3.5) can roll back refused guesses only with recurrent snapshots (`common_params_speculative::need_n_rs_seq`) | Checkpoint and restore the whole state per step |
| MTP context `n_outputs_max = 1` | The drafter reads one token per step (`common_base_params_to_speculative`). Without it the context kept logits for 512 rows: +0.63 GB on a 248k vocabulary, spill into shared GPU memory, 26 t/s instead of 68 (measured, fixed before the commit) | llama.cpp's default (n_batch outputs) |
| Draft size 3 (`CLUAIZ_MTP_DRAFT`) | llama-server's default `--spec-draft-n-max 3`; reports show acceptance falls with longer drafts (72% at 3, 83% at 2, 44% at 6 in the sources below) | Larger drafts |
| No drafting for media prompts and after a context shift | The drafter did not see the media embeddings; after a shift its positions no longer match the target's. Both only lower acceptance, so there is no gain to keep | Feeding media rows to the drafter (not built) |
| Auto by default: load rules, then a per-answer guard | Run 23: whether MTP gains depends on the model (MoE on the GPU gained on every prompt, dense lost on prose everywhere), the placement (experts in RAM or on the SSD lose) and the text (JSON gains everywhere). No fixed rule by placement is right for all; a stored measurement and a timed guard are. Run 24: auto lost 0.81-1.02x where always-on lost 0.64-0.80x, and kept 1.19-1.66x where on gained 1.21-1.72x | Off by default (no gain for anyone); on by default (up to 39% slower on prose, run 22 split); a kept-share limit (break-even was 40% kept on one setup, 85% on another) |
| Guard: 8 plain steps, then drafting timed, compared every 32 drafted tokens; slower stops guessing for the answer | Times the real thing on this machine and answer, so no per-model threshold is needed. The drafter still reads the plain steps, so plain is measured a little slow and the guard leans to keeping MTP | Probing again later in the answer (the drafter's state after a stop is not kept) |
| Auto load rules: disk reads -> off; MTP memory (1.6 blocks + 0.15 GB, run 23) over the plan's headroom -> off; stored `bench --mtp-compare` result -> on at 5% faster, else off; otherwise on with the guard | Run 23: experts from the SSD lost on every prompt (0.63-0.98x); a model that just fits must not spill because of MTP; a measurement beats any rule | Planner counting MTP memory itself (step 3, parked) |
| `p_min` 0 (always draft `n_max`); `CLUAIZ_MTP_PMIN` only for tests | llama.cpp's default (`common.h`); 0.5 and 0.75 kept more guesses (74-90%) but were slower than 0 on every prose prompt on the GPU (`bench --mtp-compare` below) | Stopping drafts at low confidence |
| Timings measured by the stream when MTP drafted (`NativeLlama::mtp_timings`) | llama.cpp counts every decode of more than one token as prompt processing (`llama-context.cpp`, `n_queued_tokens > 1`), so its counters put the checks of guesses into the prompt numbers | llama.cpp's perf counters |
| Drafted / kept counts in `ContextReport.mtp` | `bench --mtp-compare` and chat API usage read them per generation; verbose `cluaiz run` prints the percentage kept | Log line only |

### Measured

Full record with raw logs, answers and scripts: [run 22](../../test/cloud/runs/2026-11-19-run22-laptop-mtp/SUMMARY.md).

RTX 3050 Laptop 4 GB, Windows, CUDA. `unsloth/Qwen3.5-2B-MTP-GGUF` Q4_K_M (NextN inside the file),
`n_gpu_layers 99`, context 4096, temperature 0, `/v1/chat/completions`, three runs after one warm-up.
llama.cpp b12534 (official Windows CUDA 12.4 build) on the same file and settings, run in the same
hour.

| Run | Without MTP | With MTP | Guesses kept |
|---|---|---|---|
| Cluaiz, 222-token answer | 67.8 t/s | 85.2 t/s | 64% |
| llama-server b12534 | 66.6 t/s | 82.0 t/s | 145 / 228 |
| Cluaiz, thinking on (268 tokens) | 66.0 t/s | 95.3 t/s | 75% |
| Cluaiz, JSON schema answer (39 tokens) | 50.9 t/s | 76.0 t/s | 79% |

- Text: identical with and without MTP in the six comparisons of this table (byte compare). The
  `bench --mtp-compare` runs below found two prompts (`chat`, `long`) whose text changed; see "What it does".
- VRAM after load: 1.83 GB without, 1.96 GB with MTP.
- Placement changes the text: runs with 22 and 23 GPU layers gave different greedy answers, so text
  and speed comparisons pin `n_gpu_layers`. llama-server's own plain and MTP answers also differed.

### Measured with `cluaiz bench --mtp-compare` (real prompts, same placement)

`cluaiz bench <model> --mtp-compare` plans once with MTP off, then loads MTP off and on with that exact
placement and answers each prompt in `cmd/src/cli/data/bench_mtp_prompts.json` greedily with thinking
off (one warm-up, then `-r` runs; the prompt cache is cleared before every run). Prompts: `code`
(Python function and explanation, 256 tokens), `chat` (free prose, 256), `list` (ten countries as a
JSON array, 256), `long` (summary of a 690-word text, 128). Same machine and model as above, context
4096. Rerun: `CLUAIZ_MTP_DRAFT=<n> CLUAIZ_MTP_PMIN=<p> cluaiz bench qwen3.5-2b-mtp-gguf-Q4_K_M --mtp-compare -r 2`
(add `--n-gpu-layers 12` or `0` for the split and CPU rows).

Gain = generation t/s with MTP over without (kept share in brackets). Each row is one run of the
tool; MTP off and on were measured minutes apart:

| Setup | Off t/s | code | chat | list | long |
|---|---|---|---|---|---|
| Full GPU, draft 3, run A | 55 | 1.19x (63%) | 0.85x (35%) | 1.49x (90%) | 0.95x (44%) |
| Full GPU, draft 3, run B | 72 | 1.12x (63%) | 0.81x (35%) | 1.38x (90%) | 0.90x (44%) |
| Full GPU, draft 1, run A | 56 | 1.16x (82%) | 1.07x (64%) | 1.24x (98%) | 1.09x (74%) |
| Full GPU, draft 1, run B | 71 | 1.05x (82%) | 0.96x (64%) | 1.14x (98%) | 1.02x (74%) |
| Full GPU, draft 2, run A | 57 | 1.25x (76%) | 1.01x (52%) | 1.38x (95%) | 1.05x (56%) |
| Full GPU, draft 2, run B | 71 | 1.17x (76%) | 0.97x (52%) | 1.33x (95%) | 1.00x (56%) |
| Full GPU, draft 4 | 57 | 1.09x (52%) | 0.84x (28%) | 1.91x (86%) | 1.08x (39%) |
| Full GPU, draft 3, `p_min` 0.5 | 75 | 1.03x (82%) | 0.76x (74%) | 1.40x (95%) | 0.75x (76%) |
| Full GPU, draft 3, `p_min` 0.75 | 75 | 0.96x (90%) | 0.70x (90%) | 1.21x (95%) | 0.76x (88%) |
| Split (`--n-gpu-layers 12`), draft 3 | 30 | 1.10x (62%) | 0.61x (32%) | 1.51x (91%) | 0.85x (33%) |
| CPU only, draft 3 | 21 | 0.97x (66%) | 0.65x (34%) | 1.30x (90%) | 0.85x (48%) |
| CPU only, draft 3, `p_min` 0.5 | 22 | 1.04x (77%) | 0.85x (72%) | 1.33x (96%) | 0.92x (75%) |
| CPU only, draft 3, `p_min` 0.75 | 23 | 1.04x (89%) | 0.82x (89%) | 1.17x (96%) | 0.86x (92%) |

What these runs show:

- The text decides more than anything else: structured output (`list`) gained in every setup (1.14x
  to 1.91x); free prose (`chat`) lost in most.
- Fewer guesses per step lose least on prose: draft 1 never fell below 0.96x; draft 3 and 4 fell to
  0.81-0.85x on prose on the GPU.
- Split and CPU lose more on prose (0.61x, 0.65x): checking 4 tokens costs more there than on a GPU
  that holds all weights.
- `p_min` raised the kept share but not the speed on the GPU: drafts got shorter while each step still
  paid for the drafter.
- The same setup gave 55 t/s in run A and 72 t/s in run B (laptop GPU clock, other programs on the
  GPU), and the gains moved with it: the faster the plain run, the smaller the gain. A difference under
  about 10% is within this noise; rows from different runs are not compared to each other.
- Prompt t/s: full GPU, MTP on was not slower on any prompt (`long`: 3689 vs 4011 t/s, run B); CPU only
  it was up to 12% slower. In the split row the tool printed 0.0 prompt t/s with MTP on, so prompt speed
  there is unknown (Pending).
- Text: the same with and without MTP in 23 of 52 prompt pairs; draft 1 kept it in all 8, `list`
  changed in 2 (split, CPU `p_min` 0.5).

Placements, MoE and the SSD on an L4 and the laptop, with off and on alternated per round:
[run 23](../../test/cloud/runs/2026-11-20-run23-mtp-placements/SUMMARY.md). In short: the MoE 35B-A3B
gained on every prompt with all weights on the GPU (1.21-1.73x) but lost on prose with its experts
in RAM (0.70x) and on every prompt with experts read from the SSD (0.63-0.98x); the dense 2B lost on
prose in every placement; MTP cost 19-34% of the prompt speed on the GPU.

### Where MTP is expected to lose (from sources, not all measured here)

Speculative decoding speedup is `E / (v + g*c)`: `E = (1 - a^(g+1)) / (1 - a)` tokens per target run
(acceptance `a`, draft size `g`), `v` = time to check `g+1` tokens over the time for one, `c` = drafter
cost over target cost (Leviathan et al., Equation 1 and Theorem 3.8; the theorem assumes `v = 1`).
With all weights in VRAM a decode is limited by reading the weights once, so `v` stays near 1. It
grows, and MTP can lose, when:

| Setup | Reason | Source |
|---|---|---|
| MoE with experts in RAM (`n_cpu_moe`) | each checked token can route to other experts: more expert bytes read and computed on the CPU | ik_llama.cpp issue 1630; PR 22673 comment (6 GB laptop, 35B-A3B: best gain at draft 2) |
| Low VRAM | NextN weights and the MTP context push layers off the GPU or spill to shared memory | PR 22673 (27B: +2.49 GiB); our 26 t/s run above |
| Metal with K-quants at draft 1-2 | the small-batch kernel `mul_mv_ext` covers Q2_K..Q6_K only for 4-8 rows, IQ2/IQ3/IQ4_XS not at all | `llama.cpp/ggml/src/ggml-metal/ggml-metal-ops.cpp:2763-2790`; issue 23752 (M1 Max, 9B Q4_K_M: 25.3 t/s plain, 21.9 at draft 2) |
| Long prompts | the drafter reads the target's hidden rows on every prompt chunk | PR 22673: prefill 1315 -> 665 t/s, "mainly due to Device-To-Host (D2H) embedding transfers" |
| Sampling at temperature > 0 | the sampled token must equal the drafter's greedy guess | follows from sample-and-match; not measured |

### Failure and recovery

- Drafter cannot be made (no NextN layers, context creation fails): logged, the model runs without MTP.
- Drafter fails to read a batch: drafting stops for that answer; generation continues plain.
- Target cannot drop refused guesses (`llama_memory_seq_rm` false): the answer stops with `stop = error`
  and an error line. Not seen in tests.
- Generation stops with guesses unchecked: the KV of both contexts is cut back to the last kept token.

### Sources

- Leviathan, Kalman, Matias, *Fast Inference from Transformers via Speculative Decoding*, arXiv 2211.17192 (formulas read from the PDF).
- llama.cpp PR 22673 (MTP support): https://github.com/ggml-org/llama.cpp/pull/22673
- llama.cpp issue 23752 (MTP slower on an M1 Max): https://github.com/ggml-org/llama.cpp/issues/23752
- ik_llama.cpp issue 1630 (MoE offload with speculative decoding): https://github.com/ikawrakow/ik_llama.cpp/issues/1630
- Pinned llama.cpp: `common/speculative.cpp` (`common_speculative_impl_draft_mtp`, `common_speculative_init_result`), `examples/speculative-simple/speculative-simple.cpp` (the loop this follows).

### Pending

| Item | Why it matters | Needs |
|---|---|---|
| Guard cost on slow cases | auto still loses 11-19% on dense prose (the first ~40 tokens are measured before it stops) | a shorter probe, or a stored per-placement result per kind of text |
| Stored result is one average over four prompts | it turned off a placement where JSON gains 1.37x (run 24, D) | store per prompt kind, or keep the guard on when the average is near even |
| The planner counts MTP's memory | Auto only turns MTP off when its estimate does not fit | step 3 (planner parked) |
| MTP off and on measured in the same minute | GPU clock changes moved the plain speed from 55 to 72 t/s between runs | `bench --mtp-compare` alternating off and on loads per repetition |
| Prompt t/s with MTP on in a split placement | the tool printed 0.0 | find where the timing is lost |
| Memory of MTP in the load plan | a model that just fits may spill with MTP on | planner work (parked) |
| Tests: MoE `n_cpu_moe`, weights larger than RAM, temperature > 0, a 3000-token prompt, Q4_0 / Q8_0 / IQ4_XS | each can turn the gain into a loss (table above); split, CPU, draft 1-4 and `p_min` are done | an MoE MTP model (the smallest Qwen3.5/3.6-35B-A3B MTP file is 11.4 GB) and small downloads for the quant types |
| Metal and Vulkan runs | reports disagree on Metal; Vulkan had wrong output in early reports | a Mac and a Vulkan machine |
| Gemma 4 MTP sidecar file (`mtp-*.gguf`) | `pull` downloads it, the Engine does not use it | load it as a draft model (`common_speculative_init_result` takes its path) |
| Budget for models trained on one (Seed-OSS `thinking_budget`, multiples of 512) | such a model plans its thinking to the number; today it only gets the forced close | pass the budget to the template, which needs the room before the prompt is rendered |
| Auto effort on a slow CPU | the speed rule is unit-tested (5 t/s gives the 1,024 floor, 36 t/s gives 6,480) but not run on a real CPU-only machine | a CPU-only run (the planned 8 GB cloud VM) reading the `Auto thinking ceiling` log line |
| Live API contract proof for MTP usage | A model with working MTP is needed to exercise both response modes | Run the ignored `mtp_contract` test with a model path and fresh isolated home |
| Prompt cache under load | measured with one request at a time only; run 40's 4-at-once gap is not rerun with it on, and the 4 unit tests in `prompt_cache.rs` were not run (the llama suite is not run on the dev machine) | run 40's ticket test with `CLUAIZ_PROMPT_CACHE_MB` set, and the unit tests in CI |
