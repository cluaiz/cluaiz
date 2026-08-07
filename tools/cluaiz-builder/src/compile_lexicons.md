# Lexicon Builder (`compile_lexicons.rs`)

This Rust-based CLI tool is designed to download, clean, normalize, and export International Phonetic Alphabet (IPA) pronouncing dictionaries in raw text format for the Cluaiz TTS inference engine.

---

## 🔍 Overview & Motivation

Processing raw pronouncing dictionaries directly from Hugging Face is handled by this builder tool to pre-generate language lexicons and package them inside the `assets/ipa_dictionary/` directory.

- **Lightweight Assets:** By packaging only the raw `.txt` files in the repository assets, we avoid committing heavy binary files into Git history.
- **On-the-Fly Engine Compilation:** At runtime, the TTS engine dynamically copies the `.txt` file into the model directory and compiles it into a high-performance binary format (`lexicon.bin`) on-the-fly.

---

## 📦 Dual-Priority Datasets

The builder downloads parquet shards directly from Hugging Face and merges them using the following rules:

1. **Priority 1 (Omneity Labs `ipa-dict`):**
   - Clean, hand-curated pronunciations across ~25 languages.
2. **Priority 2 (Neurlang `ipa-lexicon-4v0-7M`):**
   - Massive 7-million word database across 350+ languages.
   - **Quality Filter:** Only entries with community vote confidence (`votes >= 1`) are imported.
   - Priority 2 entries will **never** overwrite Priority 1 entries.

---

## 🚀 Command Usage & Execution

You can run the compiler with the following CLI command structure:

```powershell
cargo run --manifest-path tools/cluaiz-builder/Cargo.toml --bin compile-lexicons -- [--lang <lang_code>] [--no-neurlang]
```

### Options
- `--lang <lang_code>`: Limit processing to a specific language code (e.g. `hi` or `en-us`).
- `--no-neurlang`: Skip downloading the massive 700MB Neurlang dataset, generating lexicons solely from the high-quality Omneity Labs dataset.

### Examples

#### 1. Generate Text Lexicons for All Languages
```powershell
cargo run --manifest-path tools/cluaiz-builder/Cargo.toml --bin compile-lexicons
```

#### 2. Generate Text Lexicon for Hindi Only (Fast Run, Skips Neurlang)
```powershell
cargo run --manifest-path tools/cluaiz-builder/Cargo.toml --bin compile-lexicons -- --lang hi --no-neurlang
```

---

## ⚙️ Automatic Integration with Engine

The TTS engine (`interface-engines/onnx`) interacts with these files automatically:
1. **Dynamic Copying:** If a Piper model folder lacks `lexicon.txt` and `lexicon.bin`, the engine automatically copies `<lang>.txt` from `assets/ipa_dictionary/` to `<model-dir>/lexicon.txt`.
2. **Dynamic Compilation:** On the first synthesized audio request, the engine automatically compiles `lexicon.txt` into `lexicon.bin` in the model folder.
3. **Auto-Recompilation:** If you manually edit a model's `lexicon.txt` to add custom pronunciations, the engine compares file timestamps at startup. If the text file is newer, the engine automatically recompiles it to `lexicon.bin` on-the-fly.
