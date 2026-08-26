# Live Stream Steering & Control API

`POST /engine/skip_think` & `POST /engine/cancel`

Real-time programmatic steering controls for active inference streams, allowing clients to skip thinking chains or cancel generation mid-stream.

---

## 📌 Endpoint Information

* **HTTP Method:** `POST`
* **Paths:**
  * `/engine/skip_think` — Signals the active inference engine to close its `<think>` chain-of-thought block immediately and stream the direct final answer.
  * `/engine/cancel` — Triggers a global cancellation signal to immediately abort active token generation.

---

## 💻 Code Examples

### 1. Skip Thinking (cURL)

```bash
curl -X POST http://localhost:8000/engine/skip_think
```

### 2. Cancel Generation (cURL)

```bash
curl -X POST http://localhost:8000/engine/cancel
```

### 3. Python (Requests)

```python
import requests

# Skip thinking during an active streaming completion
res_skip = requests.post("http://localhost:8000/engine/skip_think")
print("Skip Think Response:", res_skip.json())

# Abort active token generation
res_cancel = requests.post("http://localhost:8000/engine/cancel")
print("Cancel Response:", res_cancel.json())
```

---

## 📤 Response Format (`/engine/skip_think`)

```json
{
  "status": "success",
  "message": "Brain skip signal injected. Neural graph will pivot."
}
```

---

## 📤 Response Format (`/engine/cancel`)

```json
{
  "status": "success",
  "message": "Global cancel signal triggered. Active inference stopped."
}
```
