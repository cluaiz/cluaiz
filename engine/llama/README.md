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
| [`native/sampler.rs`](src/native/sampler.rs) | Llama.cpp native sampler chain (temp, top_p, min_p, penalties). |
| [`native/templater.rs`](src/native/templater.rs) | Direct C-ABI Jinja chat templating from the custom fork. |
| [`ffi/llama_cpp.rs`](src/ffi/llama_cpp.rs) | Raw `extern "C"` bindings to compiled `llama.cpp`. |
| [`ffi/mtmd.rs`](src/ffi/mtmd.rs) | Header-generated multimodal bindings, typed parts, token/position queries and evaluation helpers. |
| [`native/session.rs`](src/native/session.rs) | Paired model/projector lifetime, memory preflight, prepared media/text generation and pointer-free model telemetry used by EngineHost. |
| [`native/media.rs`](src/native/media.rs) | Private projector/bitmap/chunk owners, queried audio preparation, template association and native encoder prefill. |
| [`native/context.rs`](src/native/context.rs) | Shared native batch ownership and context shifting. |
| [`native/speech.rs`](src/native/speech.rs) | Experimental libmtmd speech generator, independent backbone/projector, vocabulary-derived language guard and optional authorized speaker PCM through the shared bitmap owner. |
| [`ffi/reasoning.rs`](src/ffi/reasoning.rs), [`ffi/reasoning_bridge.cpp`](src/ffi/reasoning_bridge.cpp) | Narrow C ABI to the pinned upstream reasoning-budget sampler; no copied budget state machine. |
| [`llama.cpp/`](llama.cpp/) | The underlying C/C++ inference engine with custom M-RoPE and chat C-ABI patches. |
| [`native/projection.rs`](src/native/projection.rs) | What a model and context will take per device, from llama.cpp's own no-alloc dry run (the one behind `--fit`), so the planner can correct its byte estimate (`project`, `ProjectedMemory`, `DeviceBytes`). Nothing is allocated. |
| [`native/load_steps.rs`](src/native/load_steps.rs) | What a chat-model load is doing, for an observer set on the loading thread (`observe`): weights read in percent (llama.cpp's `progress_callback`, set only when someone observes), a fallback llama.cpp's own load took, context creation, warmup. Used by `engine/runtime` load events. |
| [`native/tokenizer.rs`](src/native/tokenizer.rs) | One native tokenizer (`VocabHandle`) for chat and embedding contexts. |
| [`native/media_tests.rs`](src/native/media_tests.rs) | Tests of the media owners. |
| [`ffi/chat.rs`](src/ffi/chat.rs) | Narrow C ABI to `chat_bridge.cpp` for native tool calls: llama.cpp's common chat layer renders tools in the model's own format (`ChatTools::apply`), gives the lazy tool-call grammar, parses the output (`ChatParser::parse`, partial too) and turns a JSON schema into a grammar (`json_schema_grammar`). |
| [`native/speculative.rs`](src/native/speculative.rs), [`ffi/speculative_bridge.cpp`](src/ffi/speculative_bridge.cpp) | MTP speculative decoding, experimental. Settings `optimization.mtp`: `auto` (default: the runtime's load rules, then a guard in each answer that stops guessing where it is slower), `on`, `off` (load flag `--mtp`; `CLUAIZ_MTP=on` forces it for tests; `CLUAIZ_MTP_DRAFT` = guesses per step, default 3 as llama-server; `CLUAIZ_MTP_PMIN` = least drafter probability, default 0). A model with NextN layers (Qwen3.5 / 3.6, GLM-4.5+, DeepSeek-V3, ...) is loaded with `load_mtp`; llama.cpp's own `draft-mtp` drafter (`common_speculative`) guesses the next tokens on an MTP context; `stream.rs` decodes the token and its guesses in one batch and keeps a guess only when the target's sampler picks it, so the answer is the target's. The drafter sees every target decode (prompt chunks too). Media prompts do not draft; a context shift stops drafting for that answer. Its memory is not in the load plan yet (Auto turns MTP off when it would not fit next to the plan). |
| [`moe_override.rs`](src/moe_override.rs) | `--n-cpu-moe N`: the routed experts of the first N blocks stay in RAM (a CPU buffer), everything else of those blocks (attention, shared weights, KV) on the GPU; the same tensor overrides llama.cpp builds (`CpuMoeOverrides`). |

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
- **Estimate boundary:** callers supply non-zero weight/workspace estimates; weights must cover at least the file size, decoded data obeys media limits. This is a preflight estimate, not an OS reservation or a measured native peak. It does not eliminate races with other processes or promise allocation success. Encoder expansion/workspace still needs measurement with actual requests.
- **Encoder evidence:** successful `mtmd_init_from_file` performs upstream embedding-width checks. `mtmd_support_vision`, `mtmd_support_audio` and `mtmd_get_audio_sample_rate` then populate `ProjectorSupport`. These queries do not prove transcription quality, model answers or an enabled API route.
- **Cancellation/error:** a retained atomic flag is passed to the native load-progress callback. Native null results, invalid rates and changed-file checks return errors; local owners drop on Rust-side failure. Prepared generation probes free memory again for configured workspace and the additional geometry-derived native conversion peak; it no longer charges the full configured decoded ceiling. Already-resident source Arcs and weights are not counted as new allocations. Failed/cancelled prefill or a stopped consumer clears the partial model memory and returns an error. No automatic driver-failure retry is implemented here.
- **Lifecycle tests:** an ignored Rust test takes `CLUAIZ_MEDIA_MODEL` and `CLUAIZ_MEDIA_PROJECTOR`. It loads real weights on CPU and checks support against package declarations, same-session reuse, changed-thread reload, unload, missing file and paired drop. No complete native-leak/OOM assertion is made.

## Prepared media generation contract

- **Interface:** `NativeSession::stream_prepared` consumes `ChatEnvelope<Vec<PreparedPart>>`, the output token budget, the loaded model's `StructuralDNA`, request settings and a streaming consumer. It requires a successfully loaded paired projector. Source resolution stays in `cluaiz_media`; this native adapter receives decoded buffers only.
- **Audio:** common `prepare_mono` and `resample_linear` prepare finite PCM F32 at the encoder's queried rate. Channel mixing retains amplitude without automatic peak normalization. The native constructor copies samples; the pinned implementation uses a checked 32-bit sample count internally. The adapter does not duplicate audio decoding or implement the model's learned encoder.
- **Image:** validated RGB8 dimensions and byte layout go to the real copying bitmap constructor. Native mtmd owns model-specific resizing, patches and learned encoding. Unsupported loaded encoder inputs fail before evaluation.
- **Association:** collision-checked request placeholders keep ordered text/media positions through the existing chat template. Every placeholder must survive exactly once and in order. They are replaced with typed text/bitmap parts before tokenization; literal user text such as `<__media__>` is not interpreted as a bitmap delimiter.
- **Accounting:** aggregate retained media and conservative conversion/copy peaks obey the configured decoded ceiling. Native token and position queries, rather than pixel/sample counts, determine context usage. The larger native count must fit `n_ctx - (max_tokens + 16)`. On overflow, the configured context-shifting policy may remove complete oldest turns using the shared core turn selector, then count the remaining prompt again before learned encoding. The leading system message and the latest user turn/attachments (including its tool continuation) stay intact; Off refuses overflow. Removed native bitmaps are freed and retained IDs still refer to the original uploaded parts. Context reports include removed messages and the measured reduction in occupied positions. A current document/image that cannot fit is refused explicitly with active/input/output counts; no silent content truncation, context increase or multimodal KV shift is used.
- **Prefill:** the adapter orchestrates pinned native `mtmd_encode_chunk`, `mtmd_helper_decode_image_chunk` and text `mtmd_helper_eval_chunk_single` calls in the original order. Upstream still owns positioning, batching and attention behavior. Post-decode callbacks expose actual processed embedding batches; encoding counts represent native slices associated by copied bitmap IDs with the uploaded part. Last-text-chunk logits enter the existing sampler/reasoning/output loop. Raw bitmap copies are released after prefill; source Arcs needed by API/tool continuations remain owned. Sampler history receives real text tokens, without invented media token IDs.
- **Cache boundary:** media turns accept no text-only prefix cache and return no prefix token history from `NativeSession`. An existing text-only caller retains its original API and prefill path. Agent continuation and mixed-history cache reuse are not yet integrated.
- **Budget boundary:** conversion accounting retains the full original source-buffer footprint while native copies coexist, including API-held Arc clones. Rendered text/template allocations and native workspace expansion are covered only by the conservative workspace policy, not measured individually. Caller-supplied workspace estimates are not measured peaks or OS reservations. Source decode/tokenization cancellation latency and GPU/Linux/macOS behavior remain unmeasured.
- **Observed audio proof:** on Windows CPU, the ignored `test_session_media_online_audio_and_text_regression` loaded the installed Gemma E4B Q4_K_M plus BF16 companion and fetched a public [PyTorch speech demo](https://docs.pytorch.org/audio/2.5.0/tutorials/speech_recognition_pipeline_tutorial.html). Actual model output was `i had that curiosity beside me at this moment`, matching the published transcript. Native logs showed 85 audio tokens. The same test rejected an excessive output reservation and a stopped consumer, then successfully ran text chat again: `1 passed; 0 failed`, 38.86 seconds. Model/projector/demo URL and expected text come from test environment variables; the expected transcript is never supplied to the model prompt.
- **Observed image proof:** `test_session_encoded_images_change_answers_and_recover_text` used the same installed CPU model/projector pair and the same question for three encoded 128x128 fixtures: red PNG returned `Red`, blue PNG returned `Blue`, and blue JPEG returned `Blue`. Actual data URIs pass through `SourceResolver`, `prepare_chat`, the shared RGB decoder, native image encoder and the existing generation loop. Native logs show 81 image tokens per fixture. Ground-truth colors are asserted only in the test, never supplied in the prompt. The temporary resolver directory is empty after requests and plain text generation succeeds afterward: `1 passed; 0 failed`, 61.13 seconds. Image encoding, base64 and temporary-directory dependencies are test-only; no production decoder/encoder was added by this test.
- **HTTP evidence:** `api/tests/media_contract.rs` additionally exercises actual loopback JSON/SSE chat routes, including one image plus inline audio request and image-preserving continuation. The final native CPU run passed in 104.92 seconds with fixture-specific configured workspace/decoded ceilings; see the API README for scope and resource refusal evidence.
- **Proof limits:** the speech and solid-color fixtures do not certify App UI behavior, photographs, multiple images in one turn, full transcript accuracy across languages, driver failure recovery or full model/platform coverage. Upstream logs label audio experimental.

## MTP speculative decoding (experimental)

- **Status:** built and measured on CUDA (RTX 3050 Laptop 4 GB on Windows, L4 on Linux). Default
  `optimization.mtp = "auto"`: the runtime decides per load (`engine/runtime/src/native_load.rs`
  `mtp_mode`) and a guard in each answer stops guessing where it is slower (`native/stream.rs`, "MTP
  guard"); `on` and `off` force it (load flag `--mtp`; `CLUAIZ_MTP=on` forces on for tests, without
  the guard). Measuring tool `cluaiz bench <model> --mtp-compare` (off, on and auto per round).
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
