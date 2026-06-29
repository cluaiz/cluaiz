# 🏛️ Cluaiz Model Library:  Rules & Architecture

This repository holds the JSON schemas for the Cluaiz Universal Model Library. Cluaiz is built to run natively anywhere—from Mobile and Laptops (Edge) to Enterprise Servers (Cloud). 

To ensure a "makkhan" (smooth) download and execution experience without authentication crashes or RAM allocation failures, **every model added to this library MUST follow these ironclad rules.**

---

## 🛑 1. The Gated Repo Rule (Auth Bypass)
**RULE:** Never use official gated repositories directly in the JSON files.
- **Why:** Official repos (like `google/gemma-2-27b-it` or `meta-llama/Meta-Llama-3-8B`) require the user to accept a license and provide a HuggingFace Access Token. This will cause a `403 Access Denied` error for normal users.
- **Action:** Always use trusted, **ungated community mirrors** for the JSON links. 
  - *Trusted Uploaders:* `unsloth`, `bartowski`, `MaziyarPanahi`, `mbley`, `shuyuej`, etc.

---

## 🔗 2. The URL Structure Rule (GGUF vs AWQ)
This is the most critical rule for the Cluaiz Downloader Backend. 

### A. GGUF Models (Edge / CPU / Mac)
- **Format:** MUST use a **Direct Single File URL**.
- **Example:** `"download_url": "https://huggingface.co/bartowski/gemma-2-9b-it-GGUF/resolve/main/gemma-2-9b-it-Q4_K_M.gguf"`
- **Why:** A single GGUF repo contains all quantizations (100+ GB of data). If the backend triggers a folder download, it will download everything and waste storage. We must pinpoint the exact `.gguf` file.

### B. AWQ and F16 (Servers / GPUs)
- **Format:** MUST keep the official structure ending in `/resolve/main/model.safetensors` (or directly use a `repo_id` key depending on the backend parser). 
- **Example:** `"download_url": "https://huggingface.co/mbley/google-gemma-2-27b-it-AWQ/resolve/main/model.safetensors"`
- **CRITICAL:** **NEVER put `.index.json` at the end of the URL.**
- **Why:** Large models (like 27B or 35B) exceed HuggingFace's 50GB file limit and are split into 4-5 "shards" (e.g., `model-00001-of-00004.safetensors`). If you give `.index.json`, the app downloads a 40KB text file instead of weights and crashes. The Cluaiz backend should parse the repository name from the URL and use HuggingFace Hub logic to download the entire folder (all shards + tiny config files) together.

---

## 🧠 3. Supported Architecture & Engine Routing (What & Why)
Cluaiz supports specific formats tailored to backend execution engines:

1. **GGUF (Q4, Q8):**
   - *Backend Engine:* Ollama, LocalAI (powered by `llama.cpp`).
   - *Target Hardware:* Laptops, PCs, Macs, Mobile (Edge devices).
   - *Why:* Highly optimized for CPU and unified memory architectures (like Apple Silicon M-series).

2. **AWQ (4-bit / 8-bit):**
   - *Backend Engine:* vLLM, SGLang, LMDeploy.
   - *Target Hardware:* Consumer GPUs (RTX 4080/4090) and Enterprise Servers.
   - *Why:* AWQ (Activation-aware Weight Quantization) is the modern industry standard for GPU serving. Providing both 4-bit (for consumer GPUs) and 8-bit (for enterprise servers with extra VRAM) gives perfect flexibility.
   - *Note on GPTQ:* **We do NOT support GPTQ.** AWQ and GPTQ serve the identical purpose of VRAM reduction, but AWQ is newer, faster on modern GPUs, and retains higher accuracy. Supporting both is unnecessary library bloat.

3. **BitNet / 1-bit / 1.58-bit:**
   - *Target Hardware:* Ultra-edge devices (Raspberry Pi, Low-end Phones).
   - *Why:* The next generation of neural networks, requiring minimal hardware power for inference.

---

## 🗑️ 4. The Lean Library Protocol (No F16/Base Models in JSON)
**RULE:** Do NOT include F16 (FP16/BF16) uncompressed Base models in the JSON library UI.
- *Why:* 99% of normal users will accidentally click a 60GB F16 model, wasting bandwidth and crashing their edge devices due to lack of VRAM. We must keep the curated library clean ("kachra nahi failayenge").
- *How Whales use F16:* The Cluaiz engine fully supports F16/Base models (for fine-tuning or high-end multi-A100 server usage). However, advanced users must manually input the HuggingFace Repo ID via an "Advanced/Custom Download" input field. We provide the engine support, but do not advertise heavy base models in the default curated UI.

---

## 📏 5. The Ultimate Size Matrix (Hardware vs Quantization)
**RULE:** We map Quantization formats based on Model Size and target hardware logic.

1. **Tiny Models (< 8B Parameters) - e.g., Gemma 2B, Qwen 1.5B, Llama 3.2 1B:**
   - **Allowed Formats:** ONLY `GGUF` (Q4_K_M, Q8_0) and `BitNet`.
   - **Banned Formats:** `AWQ`.
   - *Logic:* Providing GPU-heavy AWQ for 2B models is completely illogical. Users with 24GB VRAM GPUs (RTX 4090) do not run 2B models; they run 27B models. Tiny models are exclusively for Edge devices, which run `llama.cpp` (GGUF). 

2. **Medium/Large Models (8B to 35B Parameters) - e.g., Gemma 9B, Gemma 27B, Qwen 35B:**
   - **Allowed Formats:** `GGUF` (Q4, Q8 for Edge) + `AWQ` (4-bit, 8-bit for GPU/Servers).
   - *Logic:* Servers and high-end GPUs have the VRAM headroom to run 8-bit AWQ, providing near-uncompressed accuracy while still saving significant memory compared to F16. Therefore, both 4-bit and 8-bit AWQ are fully supported for medium/large models.

---

## 🗑️ 4. The Lean Library Protocol (No F16/Base Models in JSON)
**RULE:** Do NOT include F16 or uncompressed Base models in the JSON library.
- **Why:** 99% of normal users will accidentally click a 60GB F16 model, wasting bandwidth and crashing their devices because they lack VRAM. We will not clutter the curated library ("kachra nahi failayenge").
- **How Whales use F16:** The engine fully supports F16/Base models. However, advanced users/whales must manually paste the HuggingFace Repo ID into an "Advanced Download" field in the Cluaiz App. We provide the engine support, but we do not advertise heavy models in the curated UI.

---


*Any deviation from these rules will break the Cluaiz Downloader Pipeline.*
