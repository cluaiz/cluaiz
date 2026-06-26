# `Permission.json` Parameter Details

The `Permission.json` file configures data access rules, local node telemetry, and default model keys.

---

## ⚙️ Parameters Schema

| Parameter | Type | Options | Description |
|:---|:---|:---|:---|
| **`firewall_mode`** | String | `auto / strict / off` | Sandboxed WASM execution block level. |
| **`enable_telemetry`** | Boolean | `true / false` | Toggles anonymous diagnostics logs sending. |
| **`vectorize_user_input`** | Boolean | `true / false` | Auto-vectorizes prompts for local context databases. |
| **`vectorize_ai_response`** | Boolean | `true / false` | Auto-vectorizes outputs for retrieval layers. |
| **`chat_ttl_hours`** | Integer | `12 to 8760` | Chat session duration limit. |
| **`default_chat_model`** | String | Valid Model ID | Selected default model for chat loops. |
| **`default_vector_model`** | String | Valid Model ID | Selected default embedding model. |
