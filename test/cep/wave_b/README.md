# Wave B: old-route baselines (B0)

- **Purpose:** Record actual old HTTP responses before moving Wave B handlers into CEP. These snapshots preserve old defects; they do not certify the old APIs as correct.
- **Scope:** File/workspace, system/storage, model management, tools, audio and ingest routes from the approved CEP method catalog.
- **Runtime:** Python 3.11+ and the existing `test/context-runtime/harness.py` Engine owner. Process cleanup currently uses that harness's Windows implementation.
- **Isolation:** A fresh explicit home, a copied executable and a separate caller-supplied port. No new build, branch or checkout. Never ports 8000/8011, the personal home, an existing destination home, or someone else's running server.

```mermaid
flowchart LR
    C["Approved method catalog"] --> F["Safe local request fixtures"]
    B["Old executable + isolated config seed"] --> H["Fresh test home + executable copy"]
    H --> E["Existing Engine harness: owned process"]
    F --> E
    E --> G["Actual route, request, HTTP status, body"]
    G --> S["Redact secrets and machine paths"]
    S --> J["One golden JSON per group + summary"]
    E --> X["Stop only the owned process"]
```

## Run

From the Engine repository, supply real paths and a dedicated test port:

```powershell
python test/cep/wave_b/golden_capture.py `
  --exe <old-cluaiz.exe> `
  --seed-home <existing-isolated-test-home> `
  --home <fresh-isolated-test-home> `
  --port <wave-b-test-port> `
  --output <fresh-output-directory>
```

The seed's selected configuration files are copied and checked for changes afterwards. Weights stay external; B0 does not load or remove them. Tool file mutations use a locally created skill fixture. Invalid install IDs are rejected before a network installation; no real package install is requested. Filesystem writes, rename, copy, delete and storage cleanup affect only test fixtures. The interactive folder picker is skipped.

## Reading the evidence

- `golden/<group>.json`: actual requests and responses. Runtime values such as memory usage, timestamps and hardware measurements are observations, not stable equality assertions.
- `golden/summary.json`: executable/catalog SHA-256, route coverage, omitted successful cases, unexpected outcomes, source-config integrity and process cleanup.
- `<test_home>`, `<workspace>`, `<model:ID>` and `<missing_id>` replace machine-specific paths/IDs. Credentials and hostname are redacted.
- `observations` records known old error mappings, including invalid install IDs reported as HTTP 500 and model inspection errors returned with HTTP 200. CEP must use its approved error contract rather than copy these defects.
- The executable is frozen by SHA-256. Its source revision is explicitly **unverified** because B0 does not rebuild Claude's shared working tree.

## What B0 does not prove

It does not exercise successful model loading/removal, real tool-hub installations, speech inference or the native folder picker. Speech success requires a suitable installed model in the isolated registry. It does not compare CEP results yet; the skeleton and each migrated group must exist first. Standard OpenAI/Ollama routes and App behavior are outside this capture.

New baselines require a fresh output directory; the script refuses to overwrite existing golden JSON files. Logs and the fresh home remain available for inspection. Do not commit executable copies, logs, test-home credentials or Python bytecode.
