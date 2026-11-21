# Developer tools (`tools/`)

Programs and scripts for the people who build the Engine. None of them ships in a release or runs
inside the Engine.

| Folder | What it is | Run |
|---|---|---|
| [`cluaiz-builder/`](cluaiz-builder/README.md) | Builds the `cluaiz` program and the GPU driver libraries without compiling unused parts; also a local CI runner (`local_ci.rs`) and the packaging of native media assets (`media_assets.rs`) | see its README |
| `cldr/make_numbers.py` | Writes the spoken-number rules for text to speech, `engine/onnx/src/audio/tts/data/numbers/<lang>.json`, from Unicode CLDR (cldr-json, pinned commit `91c26740`, the same as `reading_rules.json`) | `python -I tools/cldr/make_numbers.py engine/onnx/src/audio/tts/data/numbers es fr ...` from the repository root |
| `training-scripts/distill_atma.py` | An old script (2026-06-29) that asked Gemini 1.5 Pro for training pairs for an "Atma" routing model. Nothing in the Engine uses it | not used |

## `cldr/make_numbers.py`

- The generated JSON files are what ships; the script runs only when a language is added or the
  CLDR commit changes. It needs network access to `raw.githubusercontent.com`.
- `en.json` and `hi.json` are written by hand, not generated. Slovenian is not generated: CLDR's
  spell-out reads like Croatian.
- A few editorial overrides change CLDR's written form to what is said aloud (Japanese, Chinese,
  Russian, Vietnamese); each has a comment in the script.
- Why CLDR: the Engine keeps no language word lists in Rust code; number words come from the
  published Unicode data and its licence (`tts/data/CLDR-LICENSE.txt`).
- Used by `engine/onnx/src/audio/tts/numbers.rs` and `rbnf.rs` (ICU rule-based number format).

## Pending

| Item | Why it matters | Needs |
|---|---|---|
| `training-scripts/distill_atma.py` is unused and names files that do not exist (`docs/master_landscape.md`, `NEURAL_PATHWAYS.yaml`) | dead code confuses readers | the owner's decision: delete it or move it out |
| `make_numbers.py` has no test | a CLDR update could change the spoken numbers silently | compare the generated files with the committed ones in CI |
