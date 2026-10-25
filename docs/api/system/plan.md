# Autonomous Model Execution Planning Engine API

`system.plan`

The Autonomous Model Execution Planning Engine deterministically evaluates real hardware constraints, probes model binary headers, and plans optimal hardware placement across the 5 canonical execution candidates. It provides ground-truth throughput (TPS), latency (TTFT), memory requirements, and model input/output capabilities with zero manual configuration.

---

## 📌 Endpoint Overview

* **CEP method:** `system.plan` (`POST /v1/cep`, JSON-RPC 2.0; see [CEP](../cep/README.md))
* **Errors:** CEP errors; an invalid value or unknown field is `-32602`, an unknown model `-32004`
* **Authentication:** Optional for local development; Bearer token required in production secured mode.
* **Content-Type:** `application/json`
* **Response Format:** `application/json`

---

## 🎯 Core Capabilities & Autonomous Geometry

Rather than relying on hardcoded heuristics or static lookup tables, the engine dynamically probes:

1. **GGUF Binary Prober (`GgufBinaryProber`):**
   Scans the GGUF binary header on disk at runtime to extract exact layer counts (`block_count`), context window limits (`context_length`), KV attention dimensions (`head_count`, `head_count_kv`, `head_dim`), MoE routing (`expert_count`, `expert_used_count`), and precise weight footprint in bytes.
2. **Dynamic Slot Taxonomy (`SlotType`):**
   Maps models into sovereign execution slots (`Chat`, `Ingest`, `Embedding`, `Tts`, `Stt`), dynamically determining supported input formats, generation outputs, and allowed attachment file types.
3. **Live Hardware Telemetry (`HardwareGovernor`):**
   Probes physical GPU VRAM capacity, NVML real-time free memory, GPU memory bandwidth, PCIe bus transfer bandwidth, host RAM capacity and read bandwidth, and CPU AVX2 SIMD peak compute.

---

## 🌐 Model Input, Output & Attachment Matrix

The engine provides full visibility into what a model can accept, process, and generate:

| Sovereign Slot | Supported Inputs (`supported_inputs`) | Supported Outputs (`supported_outputs`) | Supported Attachments (`supported_attachments`) | Example Models |
|:---|:---|:---|:---|:---|
| **Chat / Reasoning** | `["text"]` | `["text"]` | `["pdf", "txt", "code", "markdown", "csv", "json"]` | `bonsai-4b-gguf-q1_0`, `bonsai1-8b`, `qwen2.5-7b` |
| **Vision / Ingest** | `["text", "image"]` | `["text"]` | `["image", "pdf", "video", "txt"]` | `MiniCPM-V-4.6`, `GOT-OCR 2.0`, `fashion_clip` |
| **Speech-to-Text (STT)** | `["audio"]` | `["text"]` | `["audio", "wav", "mp3", "m4a", "flac"]` | `whisper-large-v3-turbo`, `moonshine` |
| **Text-to-Speech (TTS)** | `["text"]` | `["audio"]` | `["txt"]` | `piper-en-us-ryan-medium`, `kokoro-82m` |
| **Embedding** | `["text"]` | `["embedding"]` | `["txt", "pdf", "code", "csv", "json"]` | `bge_m3`, `nomic-embed` |

### Token Geometry Limits
* **`max_input_tokens`:** Ground-truth context window capacity (e.g. 32,768 for Bonsai-4B, 65,536 for Bonsai-8B, 262,144 for MiniCPM-V). Prompts within this limit fit entirely inside the model's self-attention geometry.
* **`max_output_tokens`:** Generation ceiling (typically 4,096 tokens or context limit), bounding autoregressive sequence generation.

---

## 🧠 Candidate Arbitration Engine (The 5 Candidates)

Autoregressive token generation ($N = 1$) is strictly **memory bandwidth bound**. The engine evaluates 5 discrete execution candidates:

```
[ Model Weights & KV Cache ]
             │
   ┌─────────┴─────────┐
   ▼                   ▼
Fits in VRAM?     Exceeds VRAM?
   │                   │
   ├─► Candidate A     ├─► Candidate C (Split Hybrid: Top Layers GPU, Base CPU)
   │   (Full GPU)      ├─► Candidate B (Weights GPU, KV Cache in Host RAM)
   │                   ├─► Candidate D (CPU Only: AVX2 SIMD in Host RAM)
   │                   └─► Candidate E (MoE Split: Active Experts in Host RAM)
```

### 1. Candidate A: Full GPU (VRAM-Resident)
* **Execution:** All layers and KV cache fit resident in GPU VRAM ($W_{\text{GB}} + \text{KV}_{\text{GB}} \le \text{VRAM}_{\text{usable}}$).
* **Latency Formula:**
  $$\text{Latency}_{\text{token}} \approx \frac{\text{Weights}_{\text{GB}}}{\text{VRAM Bandwidth} \times 0.70} + \frac{\text{FLOPs}_{\text{token}}}{\text{TFLOPs}_{\text{GPU}} \times 0.60} + 0.5\text{ ms}$$
* **PCIe Bus Cost:** **0.0 ms per token** (weights never cross PCIe during generation).
* **Verdict:** Optimal highest throughput tier.

### 2. Candidate B: Weights GPU, KV in Host RAM (`offload_kqv=false`)
* **Execution:** Model weights fit on GPU, but large KV cache is offloaded to Host RAM to conserve VRAM.
* **Latency:** Attention execution experiences per-layer round-trip PCIe synchronization to read KV cache entries from system memory.

### 3. Candidate C: Split Hybrid (Partial Offload)
* **Execution:** When weights exceed dedicated VRAM, the engine offloads $g$ layers to GPU while remaining $L - g$ layers execute on CPU.
* **Mechanism:** RAM layers compute on CPU directly without copying weights over PCIe on every token. Activations cross the PCIe boundary once per token forward pass.
* **Throughput:** Substantially faster than CPU-only execution without bottlenecking on PCIe bandwidth.

### 4. Candidate D: CPU Only (Host RAM SIMD)
* **Execution:** All weights and KV cache reside in Host DDR5 RAM and compute via multi-threaded AVX2 SIMD vector intrinsics.
* **Latency Formula:**
  $$\text{Latency}_{\text{token}} \approx \frac{\text{Weights}_{\text{GB}}}{\text{RAM Read Bandwidth} \times 0.70} + \frac{\text{FLOPs}_{\text{token}}}{\text{GFLOPs}_{\text{CPU}} \times 0.50} + 1.0\text{ ms}$$

### 5. Candidate E: MoE Split (Active Experts in RAM)
* **Execution:** For Mixture-of-Experts architectures, shared attention and dense norms reside in VRAM while sparse expert weights stream or compute in RAM.

---

## 📥 Request Body Schema

| Parameter | Type | Required | Default | Description |
|:---|:---|:---|:---|:---|
| `model` | `string` | Optional | Auto | Model identifier from installed registry (e.g. `bonsai-4b-gguf-q1_0`, `bonsai1-8b`). |
| `model_path` | `string` | Optional | `null` | Direct path to a `.gguf` file for zero-config binary header probing. |
| `batch_tokens` | `integer` (usize) | Optional | `1` | Batch size in tokens (1 for token generation; $\ge 64$ for prompt prefill evaluation). |
| `weights_gb` | `number` (f64) | Optional | Auto | Model weights size in GB (auto-probed from file if omitted). |
| `total_layers` | `integer` (usize) | Optional | Auto | Transformer layer count (auto-probed from GGUF binary headers). |
| `n_ctx` | `integer` (usize) | Optional | Auto | Context window size in tokens (auto-probed from binary header or registry). |
| `vram_buffer_gb`| `number` (f64) | Optional | `0.25-0.5` | VRAM buffer reserved for desktop display and OS. |
| `ram_buffer_gb` | `number` (f64) | Optional | `5%` | Host RAM safety buffer. |

```json
{
  "model": "bonsai-4b-gguf-q1_0",
  "batch_tokens": 1
}
```

---

## 📤 Response Schema Reference

```json
{
  "decision": {
    "model_name": "bonsai-4b-gguf-q1_0",
    "architecture": "qwen3",
    "weights_gb": 0.53,
    "batch_tokens": 1,
    "total_layers": 36,
    "n_ctx": 32768,
    "max_input_tokens": 32768,
    "max_output_tokens": 4096,
    "supported_inputs": ["text"],
    "supported_outputs": ["text"],
    "supported_attachments": ["pdf", "txt", "code", "markdown", "csv", "json"],
    "supported_modalities": ["text"],
    "kv_cache_gb": 0.12,
    "compute_buffer_gb": 0.08,
    "vram_safety_buffer_gb": 0.25,
    "ram_safety_buffer_gb": 1.19,
    "vram_free_needed_gb": 0.95,
    "ram_free_needed_gb": 1.19,
    "gpu_layers": 36,
    "cpu_layers": 0,
    "model_fits_in_vram": true,
    "faster_target": "GPU_VRAM (Candidate A: Full GPU)",
    "speedup_ratio": 6.25,
    "cpu_latency_ms": 42.15,
    "gpu_vram_latency_ms": 6.74,
    "gpu_pcie_latency_ms": 115.20,
    "winner_candidate_id": "candidate_a_full_gpu",
    "winner_generation_tps": 148.4,
    "winner_ttft_ms": 14.2,
    "decision_reason": "Candidate A: Full GPU (VRAM-Resident) wins with 148.4 tokens/sec (6.25x faster than CPU 23.7 TPS, TTFT 14.2 ms). The 0.53 GB model fits entirely inside 4.00 GB VRAM (usable 2.35 GB), incurring 0 PCIe transfer penalties during token generation.",
    "breakdown": "Model: 0.53 GB (36 layers, ctx: 32768) | Winner: Candidate A @ 148.4 TPS (TTFT: 14.2 ms, 6.25x vs CPU) | VRAM: 1.40/4.00 GB, RAM: 15.80/23.70 GB",
    "candidates": [
      {
        "id": "candidate_a_full_gpu",
        "name": "Candidate A: Full GPU (VRAM-Resident)",
        "feasible": true,
        "gpu_layers": 36,
        "cpu_layers": 0,
        "vram_required_gb": 0.85,
        "generation_tps": 148.4,
        "ttft_ms": 14.2,
        "is_winner": true
      },
      {
        "id": "candidate_b_weights_gpu_kv_ram",
        "name": "Candidate B: Weights GPU, KV in Host RAM (offload_kqv=false)",
        "feasible": true,
        "generation_tps": 42.1,
        "ttft_ms": 24.5,
        "is_winner": false
      },
      {
        "id": "candidate_c_split_hybrid",
        "name": "Candidate C: Split Hybrid (Partial Offload)",
        "feasible": true,
        "generation_tps": 112.0,
        "ttft_ms": 19.3,
        "is_winner": false
      },
      {
        "id": "candidate_d_cpu_only",
        "name": "Candidate D: CPU Only (Host RAM SIMD)",
        "feasible": true,
        "generation_tps": 23.7,
        "ttft_ms": 42.2,
        "is_winner": false
      },
      {
        "id": "candidate_e_moe_split",
        "name": "Candidate E: MoE Split (Active Experts in RAM)",
        "feasible": false,
        "generation_tps": 0.0,
        "ttft_ms": 0.0,
        "is_winner": false
      }
    ]
  }
}
```

---

## 💻 Multi-Language Code Snippets

`system.plan` is a CEP method: `POST /v1/cep` with a JSON-RPC body; the plan is the `result`.

### 1. cURL
```bash
curl -X POST http://localhost:8000/v1/cep \
  -H "Authorization: Bearer $CLUAIZ_TOKEN" -H "Content-Type: application/json" \
  -d '{"jsonrpc":"2.0","id":1,"method":"system.plan","params":{"model":"bonsai-4b-gguf-q1_0"}}'
```

### 2. Python
```python
import os, requests

reply = requests.post("http://localhost:8000/v1/cep",
    headers={"Authorization": "Bearer " + os.environ["CLUAIZ_TOKEN"]},
    json={"jsonrpc": "2.0", "id": 1, "method": "system.plan", "params": {"model": "bonsai-4b-gguf-q1_0"}}).json()
plan = reply["result"]
print(plan["model_info"]["context_window"], plan["execution_plan"])
```

### 3. Node.js
```javascript
const reply = await fetch('http://localhost:8000/v1/cep', {
  method: 'POST',
  headers: { 'Content-Type': 'application/json', Authorization: `Bearer ${process.env.CLUAIZ_TOKEN}` },
  body: JSON.stringify({ jsonrpc: '2.0', id: 1, method: 'system.plan', params: { model: 'bonsai-4b-gguf-q1_0' } })
}).then((r) => r.json());
console.log('Execution Plan:', reply.result.execution_plan);
```
