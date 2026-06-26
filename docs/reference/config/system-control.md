# `system_control.json` Parameter Details

The `system_control.json` file dictates workspace environments and hardware threads usage.

---

## ⚙️ Parameters Schema

| Parameter | Type | Options | Description |
|:---|:---|:---|:---|
| **`node_id`** | String | Alphanumeric | Workspace identifier for cluster synchronization. |
| **`active_model`** | String | Valid Model ID | Active model loaded in the server session. |
| **`user_identity`** | Object | JSON Object | Maps user parameters (`name`, `purpose`). |
| **`hardware_governance`** | Object | JSON Object | Controls limits (`vram_limit_gb`, `cpu_thread_limit`). |
| **`network`** | Object | JSON Object | Configures daemon ports and bindings. |
