# `system_booster.json` Parameter Details

The `system_booster.json` file configures execution constraints for the local inference pipeline.

---

## ⚙️ Parameters Schema

| Parameter | Type | Options | Description |
|:---|:---|:---|:---|
| **`mode_run`** | String | `edge / multitasking / balance / max_boost / ultra_max_boost / hyper_cluster` | Chooses baseline computing optimizations. |
| **`n_gpu_layers`** | Integer | `-1 to 128` | Number of attention layers offloaded to GPU VRAM (`-1` = full offload, `0` = CPU only). |
| **`kv_quant`** | String | `auto / kv16 / kv8 / kv4` | Compression level for intermediate Key-Value (KV) cache layers. |
| **`context_shift`** | String | `auto / off / minimal / standard / aggressive / extreme` | Context window compression/pruning strategy. |
| **`flash_attention`** | String | `on / off / auto` | Toggles Flash Attention to optimize attention cache latency. |
| **`speculative_decoding`** | String | `on / off / auto` | Toggles Draft-model speculative decoding. |
