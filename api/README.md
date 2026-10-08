# 🌐 Cluaiz API Gateway (`api/`)

Direct Axum HTTP and Server-Sent Events (SSE) server for Cluaiz.

## 📐 Architecture

```mermaid
graph TD
    Client[Web UI / Tauri Desktop / CLI] -->|HTTP / SSE| Router[Axum Router]
    Router --> Chat[handlers/chat: Streaming Chat]
    Router --> Models[handlers/models: Vault & Registry]
    Router --> System[handlers/system: Plan & Calibration]
    Router --> Runtime[cluaiz-runtime::EngineHost]
```

## 📦 Components

| Module | Responsibility |
|---|---|
| [`handlers/`](src/handlers/) | REST endpoints for chat, models, vault, and plan inspection. |
| [`utils/`](src/utils/) | HTTP response wrappers, error encoders, and SSE streamers. |
