# GGUF Engine Configuration API

`GET /v1/system/gguf_config` & `POST /v1/system/gguf_config`

Inspects and dynamically updates hardware execution flags, thread sizing, context limits, and sampler defaults for the GGUF compute engine.

---

## 📌 Endpoint Information

* **HTTP Methods:** `GET` (read configuration), `POST` (save configuration)
* **Path:** `/v1/system/gguf_config`
* **Content-Type:** `application/json`

---

## 💻 Code Examples

### 1. cURL (Get Active Configuration)

```bash
curl -X GET http://localhost:8000/v1/system/gguf_config
```

### 2. Python (Update Samplers & Think Mode)

```python
import requests

# Fetch current config
res = requests.get("http://localhost:8000/v1/system/gguf_config")
config = res.json()

# Modify configuration values
config["samplers"]["temp"] = 0.7
config["samplers"]["top_p"] = 0.95
config["user_moved_flags"]["think_mode"] = "Auto"

# Save updated config
save_res = requests.post("http://localhost:8000/v1/system/gguf_config", json=config)
print("Save result:", save_res.json())
```

---

## 📥 Configuration Schema

```json
{
  "hardware_and_execution": {
    "n_gpu_layers": 0,
    "n_ctx": 0,
    "no_mmap": false,
    "override_tensor": "",
    "batch_size": 512,
    "ubatch_size": 512,
    "parallel": 1,
    "spec_type": "",
    "spec_draft_n_max": 0
  },
  "templating_flags": {
    "chat_template_file": "",
    "chat_template_kwargs": "",
    "jinja": false,
    "fit": "off"
  },
  "samplers": {
    "temp": 0.8,
    "top_p": 0.95,
    "top_k": 40,
    "min_p": 0.05,
    "presence_penalty": 0.0,
    "repeat_penalty": 1.1
  },
  "user_moved_flags": {
    "think_mode": "Auto"
  }
}
```
