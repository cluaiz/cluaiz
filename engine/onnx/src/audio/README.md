# Component: Cluaiz ONNX Audio & TTS Subsystem (`src/audio/`)

## Technical Specification
- **Purpose:** ONNX STT model preprocessing/graphs and multi-family TTS orchestration. Input source reading, codec decoding and PCM preparation are shared through cluaiz-media.
- **Platform Support:** Linux x86_64, macOS ARM64, Windows x86_64, Android AArch64.
- **Reusability Level:** Backend-owned model processing; common audio preparation is imported directly from cluaiz-media.

---

## Architectural Flow

```mermaid
graph TD
    S["STT local file / inline audio"] --> R["cluaiz_media::source::read_local_or_inline"]
    R --> P["cluaiz_media::audio::decode_audio"]
    P --> Q["Explicit mono / peak / rate preparation"]
    Q --> STT["Whisper spectrogram and encoder / decoder graphs"]
    A["Text Prompt Input"] --> B["tts_router::route_tts_inference"]
    U["Prepared PCM / typed text from API"] --> V["media::execute_prepared_audio"]
    V -->|"Prepared PCM"| STT
    V -->|"Typed text"| T["tts_router::route_tts_text"]
    T --> R1["reading::prepare — spoken symbols, bracket content, phrase breaks"]
    B --> R1
    R1 --> C["family_adapter::FamilyAdapter::detect_family"]
    C --> D{"TtsFamily Variant"}
    
    D -->|"Kokoro"| E["families::kokoro::execute"]
    D -->|"VitsPiper"| F["families::vits_piper::execute"]
    D -->|"Supertonic"| G["families::supertonic::execute"]
    D -->|"Chatterbox"| H["families::chatterbox::execute"]
    D -->|"CosyVoice"| I["families::cosyvoice::execute"]
    D -->|"Audio8"| J["families::audio8::execute"]
    D -->|"Matcha"| K["families::matcha::execute"]
    D -->|"OmniVoice"| L["families::omnivoice::execute"]
    
    E --> PJ["reading::append_audio — guarded boundary pauses"]
    F --> PJ
    PJ --> M["sanitize_audio_pcm"]
    G --> M
    H --> M
    I --> M
    J --> M
    K --> M
    L --> M
    
    M --> N["vocoder::NativeVocoder (WAV Header Base64 Output)"]
```

---

## API Contract (Interface)

### Hindi pronunciation input boundaries

`tts/rules_g2p.rs` uses per-language `data/g2p_rules/*.json` rules. Abugida files may declare `ignored_characters` (formatting controls or elision signs) and `word_separators` (internal word boundaries). Hindi declares ZWJ/ZWNJ and avagraha as ignored, and selected hyphens, slash and apostrophes as separators. Each piece runs through the existing vowel/schwa rules independently; formatting controls do not split consonant conjuncts. Other languages retain their own policy; Hindi does not override a preferred same-script language merely because it accepts a separator.

Hindi examples include decomposed nasal vowels (`में`, `एँ`), anusvara before non-stop consonants, Om, vocalic vowels/signs and avagraha. Vocalic R/L mappings follow this Hindi rule file's existing pronunciation convention; they do not certify Vedic/Sanskrit recitation. Nasalization and vowel length remain separate phoneme symbols and must survive the selected model's symbol conversion.

`PhonemeMap::require_phonemes` rejects sequences containing only that package's pad/BOS/EOS IDs before either Kokoro or Piper inference. A nonempty token vector alone does not prove any speech content was encoded. This prevents unsupported-only input from producing a meaningless waveform. It does not detect every omitted symbol or certify pronunciation quality.

Run `cargo test -p onnx-engine --offline --lib -- audio::tts` in an isolated home. The ignored `actual_pronunciations_fit_installed_model_symbols` test additionally requires `CLUAIZ_TTS_TEST_MODEL` pointing to a staged genuine package and checks actual symbol coverage. Real waveform/listening acceptance is separate from IPA, token-count and HTTP-success tests.

### Dutch context and Vietnamese syllable data

Alphabet JSON rules may use `before_sequence`/`after_sequence` for consecutive neighboring classes without consuming them. Dutch uses this for the open-syllable vowel approximation and a limited unstressed ending; doubled written consonants emit one consonant. This does not replace stress/morphology or dictionary lookup. Sources: [official open-syllable spelling](https://woordenlijst.org/zoeken/leidraad/2/1.html), [consonant doubling](https://woordenlijst.org/zoeken/leidraad/2/2.html).

Vietnamese OOV syllables use canonical decomposition to extract tone marks, reconstruct the remaining vowel-quality letters, then append the configured tone after the segmental sequence. `suffix_marks`, `default_suffix`, `checked_endings` and `checked_suffixes` are data-owned; existing languages keep empty defaults. The Northern/Hanoi-oriented data follows [Kirby2011](https://doi.org/10.1017/S0025100311000181), with documented segmental approximations. Model-specific missing-symbol fallbacks keep longer tone sequences ahead of their shorter prefixes and retain glottal stops. A model's tone arrows approximate pitch contours and do not certify Vietnamese phonation, intelligibility or all dialects. Existing dictionary lookup remains first; native WAV/transcription acceptance is separate. When no Vietnamese voice exists, the language data prefers an available Chinese voice; this remains a fallback, not a native Vietnamese voice or listening certification.

### Reading punctuation, parentheses and operators

`tts/reading.rs` is the shared text preparation owner. `data/reading_rules.json` stores spoken operator labels, Unicode aliases and phrase-break policy. Brackets become parenthetical boundaries; their contents are retained. The typed text path never extracts voice commands from user text. The legacy adapter unwraps nested text envelopes and accepts a voice only in the envelope prefix; typed API voice transport remains separate work.

Operator labels cover English and the existing22 rule-language codes. They come from [Unicode CLDR](https://github.com/unicode-org/cldr-json) commit `91c267402229a59e3ef2774544f001bf959e8809`; Hindi/English use documented shorter editorial forms. The Unicode-3.0 license is in `tts/data/CLDR-LICENSE.txt` and the CLI's bundled third-party notices. Rust contains no language-word replacement list. Same-script model preferences use the existing G2P rule data, so Marathi/Nepali are not automatically relabeled Hindi.

Piper/Kokoro use configured minimum boundary gaps: comma120ms, parenthesis100ms, sentence280ms and paragraph360ms. Existing quiet leading/trailing padding counts toward the target; excessive padding is capped at twice that minimum while preserving20ms guards next to speech. The detector uses0.5% of the incoming chunk's peak gain; samples above this threshold are never eligible for trimming. Quiet consonant edges remain a listening-review risk. Byte-budget splits keep whole words and use the previous crossfade without inventing a sentence pause. Extra native calls at phrase boundaries can increase synthesis time. Other model families receive normalized text, but their native timing is not certified by these two handlers' PCM join.

Decimal/grouping/time separators between digits, configured abbreviations, internal word hyphens/apostrophes and URI/email structure are preserved. Full number-to-word grammar, general math/Markdown/URL reading, arbitrary abbreviation expansion, emotional emphasis and all-language voice quality are not implemented or certified by this policy. Unsupported language/operator combinations fail explicitly; punctuation-only input is not sent to the model as speech. Regression inputs live in test-only `data/reading_tests.json`.

- **Core Router Entry:** `tts_router::route_tts_inference(engine, session, prompt, tokenizer) -> Result<String>`
- **Typed entries:** `route_tts_text(engine, session, text)` preserves text at the transport boundary; `OnnxEngine::execute_stt_prepared` accepts shared PCM without source fetching. Legacy tagged prompts adapt to these entries.
- **Audio Sanitizer:** `tts_router::sanitize_audio_pcm(pcm: Vec<f32>) -> Result<Vec<f32>>`
- **Family Inspector:** `family_adapter::FamilyAdapter::detect_family(model_dir, sessions) -> TtsFamily`
- **Asset Gate:** `family_adapter::FamilyAdapter::validate_package_inventory(family, model_dir) -> Result<()>`
- **Export Type:** Public Rust modules through `onnx_engine`; native inference is delegated to ONNX Runtime through `ort`.

---

## Deep File Breakdown

### A. Core Audio Subsystem (`src/audio/`)
- [`config.rs`](./config.rs):
  - **Logic:** Dynamic JSON config parser (`config.json`, `preprocessor_config.json`, `generation_config.json`).
  - **Why:** Resolves model parameters and special token IDs across Whisper and TTS pipelines.
- **Shared input preparation:** `cluaiz_media::source`, `cluaiz_media::audio`, `cluaiz_media::resample` own bounded local/data URI reads, codec decoding, channel conversion and rate conversion. `stt/loader.rs` and its re-exports are removed. Remote audio must first be resolved by the asynchronous shared source resolver; this synchronous legacy STT entry does not download URLs.
- [`spectrogram.rs`](./stt/spectrogram.rs):
  - **Logic:** Computes log-mel features using the validated processor configuration. Numerical parity with every upstream processor has not been measured.
- [`mel_bank.rs`](./stt/mel_bank.rs):
  - **Logic:** Builds dynamic Slaney mel filterbank matrices (80 and 128 channels).
- [`decoder.rs`](./stt/decoder.rs):
  - **Logic:** Autoregressive ONNX inference loop executing encoder-decoder transformer layers.
  - **Input policy:** Typed calls retain shared PCM and choose mono plus `LimitToUnit` peak policy before resampling at the processor rate. Legacy calls decode through shared utilities first. Oversized recording windows and native errors fail explicitly. Cached decoder topologies are rejected until their cache contract is implemented.

### B. Registered ONNX TTS Engine (`src/audio/tts/`)
- [`tts_router.rs`](./tts/tts_router.rs):
  - **Logic:** High-level router adapts legacy/typed inputs, uses shared reading preparation, detects family variant, validates inventory, executes model family pipelines, sanitizes PCM output and encodes base64 WAV URIs. Whole words stay intact; a single oversized word remains subject to the model's native token guard.
  - **Pronunciation normalization:** Abugida files can opt into canonical Unicode normalization with `normalize_unicode`; Hindi enables it so canonical-equivalent nukta spellings reach the same pronunciation keys. Other rule files keep their existing behavior. Regression inputs live in test-only `data/pronunciation_tests.json`.
- [`family_adapter.rs`](./tts/family_adapter.rs):
  - **Logic:** Pre-boot asset inventory validator and priority family detector checking `model_registry.json`, `model_manifest.json`, and ONNX session signature inputs.
- [`manifest_loader.rs`](./tts/manifest_loader.rs):
  - **Logic:** Parses manifest JSON configs to dynamically determine model sample rates (22.05kHz, 24kHz, 44.1kHz), mel channels, and token offsets.
- [`phoneme_map.rs`](./tts/phoneme_map.rs):
  - **Logic:** IPA & G2P phoneme-to-ID lookup mapper for VITS/Piper models.
- [`g2p.rs`](./tts/g2p.rs):
  - **Logic:** Rule-based and lexicon-assisted Grapheme-to-Phoneme converter.
- [`reading.rs`](./tts/reading.rs):
  - **Logic:** Data-owned spoken symbols, content-preserving parenthetical boundaries, word-preserving chunks and a shared PCM join that bounds quiet boundary padding relative to chunk gain and adds missing quiet samples.
- [`vocoder.rs`](./tts/vocoder.rs):
  - **Logic:** Native WAV encoder constructing standard PCM RIFF headers from float32 audio buffers.
- [`neural_vocoder.rs`](./tts/neural_vocoder.rs):
  - **Logic:** Executes HiFi-GAN / Vocos neural vocoder ONNX graphs converting mel-spectrogram matrices into PCM audio waveforms.

### C. 8 Model Family Handlers (`src/audio/tts/families/`)
- [`kokoro.rs`](./tts/families/kokoro.rs):
  - **Logic:** Family 2 (Kokoro-82M). Loads voice style vector `.bin` matrices (`[1, 256]`) and passes phoneme IDs through `model_uint8.onnx`.
- [`vits_piper.rs`](./tts/families/vits_piper.rs):
  - **Logic:** Family 1 (Piper/VITS). Inserts inter-phoneme zero-padding (`token 0`) for acoustic alignment and executes single-stage end-to-end variational inference.
- [`supertonic.rs`](./tts/families/supertonic.rs):
  - **Logic:** Family 3 (Supertonic-3). Runs 4-stage pipeline (Text Encoder -> Duration Predictor -> 8-step Euler ODE Denoising Loop -> Vocoder). Uses linear duration interpolation across token boundaries to eliminate 2-3x word repetition.
- [`chatterbox.rs`](./tts/families/chatterbox.rs):
  - **Logic:** Family 7 (Chatterbox Turbo). Runs 4-stage multi-codec pipeline. Uses clean reference speech embedding extraction and top-p temperature-scaled logit decoding (0.7) with repetition penalty (1.2) to eliminate static burning noise.
- [`cosyvoice.rs`](./tts/families/cosyvoice.rs):
  - **Logic:** Family 6 (CosyVoice2). Extracts speaker embeddings via `campplus.onnx` and dynamically maps all 6 required flow estimator input tensors (`prompt_token`, `prompt_speech`, `embedding`, `len`, `token_ids`) before feeding `hift.onnx` vocoder.
- [`audio8.rs`](./tts/families/audio8.rs):
  - **Runtime:** The typed package owner loads the declared Slow AR, Fast AR and codec sessions once, requesting `ORT_ENABLE_ALL` optimization like the reference runtime (ORT Level3 alone is not equivalent). `audio8/` validates the FP16 valid-prefix manifest, uses the actual tokenizer and processor prompt protocol, samples logits and updates only the evaluated cache positions. Codebooks are generated by the model, never fabricated. No-reference synthesis and package-local `meta.json`/uint16 or int64 `codes.npy` references have separate prompt paths. Guarded rolling codec windows feed the shared tempo processor and WAV parts, followed by their whole utterance. Missing assets, incompatible layouts, invalid references, failed/cancelled output and exhausted context error. Broader listening quality and other exported topologies remain unverified; see the speech API's package requirements.
- [`matcha.rs`](./tts/families/matcha.rs):
  - **Logic:** Family 5 (Matcha/LuxTTS). Executes 2-stage flow matching pipeline (`text_encoder_int8` -> `fm_decoder_int8` ODE loop -> `vocos.bin` / neural vocoder). Uses rank-1 unit slice tensor mapping for ORT scalar inputs.
- [`omnivoice.rs`](./tts/families/omnivoice.rs):
  - **Logic:** Family 8 (OmniVoice GenAI). Executes multi-head audio decoder (`audio_embeddings_encoder` -> 28-layer `llm_decoder` KV-cache -> `audio_heads_decoder` -> 8-codebook argmax decoding).

---

## Failure & Recovery Logic
- **Invalid Output PCM Buffer:** `sanitize_audio_pcm` checks generated float32 samples for `NaN` or `Infinite` values and aborts synthesis to prevent speaker buzzing/clipping damage.
- **Low Signal Noise Amplification:** If max amplitude is below `0.01`, peak scaling is skipped to avoid multiplying background neural floating-point noise into blowing air sound ("fhoor fhoor").
- **Asset Inventory Failure:** `validate_package_inventory` checks required ONNX sub-components before allocating GPU/RAM session memory and returns `PackageContractException`.

