# 🏛️ Cluaiz Engine Runtime & Host (`engine/runtime`)

Single unified model host and execution supervisor. Replaces the fragmented `dispatcher`, `CoreRouter`, and `EngineRunner`.

## 📐 Architecture

```mermaid
graph TD
    A[EngineHost] -->|Prepares| B[cluaiz-plan::ExecutionPlan]
    A -->|Spawns| C[TokenStream Channel]
    A -->|Tracks| D[Active Model Context & Slot]
```

## 📦 Components

| Module | Responsibility |
|---|---|
| [`host.rs`](src/host.rs) | Unified engine host for model preparation and streaming dispatch. |
