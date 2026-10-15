# 🏛️ Cluaiz Engine Runtime & Host (`engine/runtime`)

Single unified model host and execution supervisor. Replaces the fragmented `dispatcher`, `CoreRouter`, and `EngineRunner`.

## 📐 Architecture

```mermaid
graph TD
    A["EngineHost"] -->|Prepares| B["cluaiz-plan::ExecutionPlan"]
    A -->|Spawns| C["TokenStream Channel"]
    A -->|Tracks| D["Active Model Context & Slot"]
    A --> S["NativeSession: paired model and lazy projector"]
    A --> O["OnnxHost: serialized audio / embedding task session"]
    O --> P["Selected package, captured provider settings, typed task"]
    P --> Q["PCM transcription or text synthesis"]
    R["Ordered prepared RGB / PCM request"] --> A
    S --> T["Native media prefill then existing sampler loop"]
    E["Package input declarations and projector inventory"] --> F["media_support::resolve_support"]
    G["Wired inputs, validated model/projector pair, enabled attachment routes"] --> F
    F --> H["Available inputs with reasons and attachment formats"]
```

## 📦 Components

| Module | Responsibility |
|---|---|
| [`host.rs`](src/host.rs) | Unified engine host for model preparation and streaming dispatch. |
| [`media_support.rs`](src/media_support.rs) | Effective request input/attachment availability from declared capability, real assets and trusted backend evidence. |
| [`onnx_host.rs`](src/onnx_host.rs) | Selected ONNX audio package validation, serialized graph execution, typed chunk/result/error events and generation-scoped retention. |

## Media availability contract

- **Inputs:** `resolve_support` accepts package evidence, actually wired backend inputs, an optional trusted loaded model/projector pairing and enabled attachment routes. That pairing cannot be deserialized from a user request.
- **Planning:** `EngineHost::input_support` reads session/model state without waiting behind generation. A busy or different selected model remains unverified; another model's loaded encoder flags do not redefine this selected model's support. `media_support` adds attachment descriptors after releasing those locks, using the same resolver. The Chat planner presents one concise input contract and keeps the full internal evidence private.
- **Output:** intrinsic `inputs`, effective `available_inputs`, per-input `model_support`/`available`/`reason` and deduplicated usable attachment formats. Missing, ambiguous or incompatible assets, missing encoder, unwired dispatch and unverified pairing have distinct reasons.
- **Current route:** `CURRENT_NATIVE_CHAT_INPUTS` lists text/image/audio. `dispatch_prepared` owns decoded buffers through its blocking worker, checks the selected load fingerprint, probes package declarations and lazily loads the compatible projector before native generation. `dispatch_stream` retains the legacy text entry; API text uses `dispatch_chat_text` with selected-model identity and per-request cancellation.
- **Validation:** model and projector paths are canonicalized before comparing the trusted pairing, including Windows extended-path representation. Metadata alone never sets an image/audio input available. Unknown model capability is not promoted to support.
- **Scope:** the pure resolver reports availability; `EngineHost::media_support` supplies actual loaded encoder/pairing evidence. Decoder-library descriptors supply attachment extensions. Before lazy initialization, declared image/audio inputs can have `projector_pairing_unverified`; after initialization, reporting follows the loaded encoder. Shared media prepares document text/OCR and sampled video frames for these existing native chat routes. Sampled frames are image inputs, not intrinsic temporal-model support. General ONNX generative chat remains unwired.

## Request ownership and failure behavior

- **Model transitions:** one transition mutex serializes native load/unload allocations and publication; generation uses the paired session mutex. Requests queued after a selected-model/configuration change fail with an error instead of running on the new model. Native pointers are private to the session.

## ONNX audio ownership

- **Boundary:** `EngineHost.onnx.dispatch` receives `OnnxModel`, `AudioTask`, typed `AudioInput`, STT options, captured settings and keep-alive minutes. It uses the selected ONNX graph and never invokes llama text generation. Audio and chat have separate native owners.
- **Package identity:** declared asset paths must resolve inside the selected package. An explicit primary ONNX graph and its known required helper/external files must be present. The existing `PackageCapabilities` inventory is reused before a lease/native load and again when a queued request begins. Registry metadata, listed asset stamps, package snapshots, task and provider settings form the reuse key. This is cache invalidation, not cryptographic integrity; snapshot traversal has a depth ceiling and does not follow directory symlinks.
- **Concurrency:** one blocking worker holds the audio mutex through load and execution. Old sessions are dropped before replacement. Decoded input is retained by the queued request; it is not fetched again inside the ONNX backend. Model switches do not publish partially loaded sessions.
- **Retention:** zero unloads the audio owner after execution; null/-1 retain it until replacement. Positive minutes schedule expiry after completion. A key and monotonically increasing request generation must still match when the timer runs; old timers cannot unload a newer request's resident session. Execution errors discard the failed resident owner. An incomplete package rejected before the lease does not unload the existing valid resident.
- **Output:** `AudioEvent::Chunk` carries real incremental STT text. `Completed` carries a transcript or the existing TTS WAV data URI. Native/model failures are errors. TTS currently completes a whole WAV rather than streaming PCM chunks.
- **Limits:** a dropped STT output consumer stops at the callback boundary. Deep encoder interruption, TTS mid-synthesis cancellation, process peak bounds, concurrent chat/audio resource coordination, positive-GPU/other-OS execution and comprehensive multi-family quality remain unverified.

## Native request continuation and resource policy

- **Media history:** each media turn re-evaluates its full typed input; text-only prefix history is cleared and no synthetic prefix cache is returned. The existing text sampler/prefill remains the single generation implementation. Oversized media context fails explicitly; whole-message auto fitting remains pending.
- **Cancellation:** API chat callbacks use their own retained atomic flag. The legacy CLI entry retains its global cancellation source. A cancelled queued API request fails before encoding; an active request stops at callback/evaluation boundaries. Long native encode/decode operations have no newly certified immediate-abort latency.
- **Budget:** projector file size is the weight-estimate lower bound. `media.projector_workspace_bytes` supplies a non-zero workspace estimate and the configured decoded ceiling is charged separately. These are checked against fresh free memory after the text model is resident. They are estimates, not an OS reservation or measured process peak; resource failure is explicit. Native driver allocation failure has no automatic CPU retry here.
- **Delivery:** the worker sends its existing error token on failure. The API interprets it as an SSE error object or a non-success JSON response, clears stale generation telemetry and retains decoded media across tool continuation requests.

## ONNX task lease

- **Owner:** `OnnxModel` validates the selected package/primary/assets. `OnnxHost::with_engine` is the single serialized load/reuse/replace/error/unload implementation shared by audio dispatch and embedding batches.
- **Identity:** resident reuse includes declared asset/config stamps, task and captured provider settings. The existing generation guard protects newer requests from old positive keep-alive timers. This is cache identity, not cryptographic file integrity.
- **Embedding:** `embed` sends typed text/RGB to a blocking worker and returns actual vector/token results. Its byte and graph errors propagate to the API. Audio and embedding replace each other's ONNX resident task; neither calls chat unload. Simultaneously loaded chat/ONNX memory scheduling remains separate work.

## ONNX readiness ownership

- **Evidence:** the resident records successful native task, standalone input, package identity and captured provider settings. `readiness` returns only matching evidence and never accepts a client's readiness claim.
- **Resolver:** `media_support::resolve_onnx_support` combines it with graph declarations, known helper presence and common decoder formats. Auxiliary text is not a standalone image-graph text route; unknown/unimplemented task routes stay unavailable.
- **Invalidation:** failure/replacement/expiry/zero retention removes the resident; changed helpers/settings invalidate old readiness. Inspection never loads a session to manufacture readiness. First valid tasks may lazily initialize the owner.
