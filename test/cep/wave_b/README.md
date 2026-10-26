# CEP file, system, model and tool tests

Tests for the `fs.*`, `workspace.*`, `system.*`, `storage.*`, `models.*` and `tools.*` CEP methods. Two kinds:

| Kind | Files | Needs |
|---|---|---|
| Rust contracts (no CUDA build) | `roots_contract.rs`, `fs_methods_contract.rs`, `isolated_home.rs` | `cargo` only |
| Live journeys against a built Engine | `fs_http_journey.py`, `system_http_journey.py`, `models_http_journey.py`, `tools_http_journey.py`, helper `journey_home.py` | a built `cluaiz.exe`, an isolated seed home |

Python here is test tooling only; nothing in the Engine imports it.

## Rust contracts

`roots_contract.rs` imports the production `api/src/cep/roots.rs`. `fs_methods_contract.rs` imports the production file and workspace method modules, registers them with the real CEP dispatcher, and runs real file operations in temporary folders. This Cargo workspace builds the CEP core and these modules without the inference runtime.

```powershell
cargo test --offline --manifest-path test/cep/wave_b/Cargo.toml --target-dir target/cep-wave-b-contracts
```

Covered: schema rejection before any side effect, text and binary round trips, recursive copy / list / delete, scope denial, a real symbolic link that leaves the roots, drive-relative and root-relative paths, missing write parents, workspace synchronization, and discovery metadata. Windows must allow creating symbolic links; otherwise the link tests fail instead of skipping.

## Live journeys

Each journey copies the config of an isolated seed home into a fresh home (`journey_home.py`: never weights, `session.token` or live tool state), starts a copy of the executable there on the given port, calls the methods over `POST /v1/cep`, and stops it. Ports 8000 and 8011 and the owner's `~/.cluaiz` are refused.

```powershell
python test/cep/wave_b/fs_http_journey.py     --exe target/release/cluaiz.exe --seed-home <isolated test home> --home <fresh folder> --port 8012
python test/cep/wave_b/system_http_journey.py --exe target/release/cluaiz.exe --seed-home <isolated test home> --home <fresh folder> --port 8012
python test/cep/wave_b/models_http_journey.py --exe target/release/cluaiz.exe --seed-home <isolated test home> --home <fresh folder> --port 8012 [--check-load]
python test/cep/wave_b/tools_http_journey.py  --exe target/release/cluaiz.exe --seed-home <isolated test home> --home <fresh folder> --port 8012
```

`--check-load` also loads a small installed GGUF on the CPU and removes a hard-linked copy of it. Each journey writes `<home>/<group>-http-report.json` with every check.

Not covered: the folder picker dialog (only a denied non-owner call is tested, so no dialog opens unattended), third-party native plugin execution, other operating systems.
