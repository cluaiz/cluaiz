# Axum API

The Engine's HTTP server is built on Axum and Tokio (`api/src/routes.rs`). It serves:

- the market-standard routes existing clients already speak: `/v1/chat/completions`, `/api/chat`, `/v1/embeddings`, `/v1/audio/speech`, `/v1/audio/transcriptions`, `/v1/models`, `/api/tags`, `/api/pull`, `/health`;
- CEP, one JSON-RPC 2.0 endpoint for everything Cluaiz-specific: `POST /v1/cep`, and the same methods for MCP clients at `/mcp`.

The route handlers are in `api/src/handlers/`, the CEP methods in `api/src/cep/methods/`. A function used by both a route and a method lives in one place.

References: [API overview](../api/README.md), [CEP](../api/cep/README.md), [method reference](../api/cep/methods.md).
