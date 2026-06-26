# HTTP API Specifications

This directory contains specifications for the Axum-based HTTP REST endpoints hosted by the Cluaize Engine daemon (`cluaize serve`).

---

## 📡 Endpoint Index

* ### [`POST /chat`](file:///c:/Users/Aryan/my/Cluaiz-workspace/Cluaiz-Technologies/cluaize/docs/reference/api/post-chat.md)
  Streams token-by-token inference output.
* ### [`GET /hardware`](file:///c:/Users/Aryan/my/Cluaiz-workspace/Cluaiz-Technologies/cluaize/docs/reference/api/get-hardware.md)
  Fetches hardware and silicon diagnostics.
* ### [`GET /models/installed`](file:///c:/Users/Aryan/my/Cluaiz-workspace/Cluaiz-Technologies/cluaize/docs/reference/api/get-models-installed.md)
  Lists locally stored model files.
* ### [`GET /models/tags`](file:///c:/Users/Aryan/my/Cluaiz-workspace/Cluaiz-Technologies/cluaize/docs/reference/api/get-models-tags.md)
  Queries available registry options.
* ### [`POST /models/load`](file:///c:/Users/Aryan/my/Cluaiz-workspace/Cluaiz-Technologies/cluaize/docs/reference/api/post-models-load.md)
  Mounts a model family into VRAM/RAM.
* ### [`POST /models/download`](file:///c:/Users/Aryan/my/Cluaiz-workspace/Cluaiz-Technologies/cluaize/docs/reference/api/post-models-download.md)
  Initiates an async model download task.
* ### [`POST /v1/db/execute`](file:///c:/Users/Aryan/my/Cluaiz-workspace/Cluaiz-Technologies/cluaize/docs/reference/api/post-v1-db-execute.md)
  Forwards CDQL query instructions to `cluaizd`.
* ### [`GET /v1/permission`](file:///c:/Users/Aryan/my/Cluaiz-workspace/Cluaiz-Technologies/cluaize/docs/reference/api/get-v1-permission.md)
  Reads `Permission.json`.
* ### [`POST /v1/permission/update`](file:///c:/Users/Aryan/my/Cluaiz-workspace/Cluaiz-Technologies/cluaize/docs/reference/api/post-v1-permission-update.md)
  Updates single keys inside `Permission.json`.
* ### [`POST /v1/system/brain`](file:///c:/Users/Aryan/my/Cluaiz-workspace/Cluaiz-Technologies/cluaize/docs/reference/api/post-v1-system-brain.md)
  Toggles database brain connectivity.
* ### [`GET /v1/system/control`](file:///c:/Users/Aryan/my/Cluaiz-workspace/Cluaiz-Technologies/cluaize/docs/reference/api/get-v1-system-control.md)
  Reads variables inside `system_control.json`.
* ### [`GET /v1/booster/status`](file:///c:/Users/Aryan/my/Cluaiz-workspace/Cluaiz-Technologies/cluaize/docs/reference/api/get-v1-booster-status.md)
  Reads optimization values inside `system_booster.json`.
* ### [`POST /v1/booster/update`](file:///c:/Users/Aryan/my/Cluaiz-workspace/Cluaiz-Technologies/cluaize/docs/reference/api/post-v1-booster-update.md)
  Updates booster profiles.
* ### [`POST /v1/ingest/file`](file:///c:/Users/Aryan/my/Cluaiz-workspace/Cluaiz-Technologies/cluaize/docs/reference/api/post-v1-ingest-file.md)
  Ingests documents for vectorization.
* ### [`GET /health`](file:///c:/Users/Aryan/my/Cluaiz-workspace/Cluaiz-Technologies/cluaize/docs/reference/api/get-health.md)
  Checks if the web daemon is responsive.
