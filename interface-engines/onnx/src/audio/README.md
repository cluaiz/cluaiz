# Cluaiz ONNX Audio Subsystem (`src/audio/`)

```
   [Audio Input (URL / Local / Base64)]
                    │
                    ▼
          [audio::loader::load_audio_to_pcm]
                    │
                    ▼
   [audio::spectrogram::compute_log_mel_spectrogram]
                    │
                    ▼
          [audio::mel_bank::build_mel_filterbank]
                    │
                    ▼
     [audio::decoder::execute_audio_graph]
                    │
                    ▼
  [Autoregressive Decoder & Tokenizer Output]
```

## Module Overview

| Module | File | Purpose |
| :--- | :--- | :--- |
| **Config** | [`config.rs`](./config.rs) | Dynamic JSON config parser (`config.json`, `preprocessor_config.json`, `generation_config.json`) and special token resolver. |
| **Loader** | [`loader.rs`](./loader.rs) | `symphonia` audio decoder supporting paths, URLs, and Base64 WebM/WAV/MP3 data URIs. |
| **Spectrogram** | [`spectrogram.rs`](./spectrogram.rs) | Bit-perfect 30s reflect-padded STFT computation matching PyTorch Whisper (`global_max - 8.0`). |
| **Mel Bank** | [`mel_bank.rs`](./mel_bank.rs) | Dynamic Slaney mel filterbank builder (128 & 80 mel). |
| **Decoder** | [`decoder.rs`](./decoder.rs) | Autoregressive ONNX inference loop executing encoder hidden states and decoder logits. |
| **API Entry** | [`mod.rs`](./mod.rs) | Exposes clean, decoupled audio module interface to `OnnxEngine`. |
