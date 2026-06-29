# <img src="../bonsai-logo.svg" width="48" height="48" style="vertical-align:middle">  1-bit Bonsai | v1.1

> **The world's first commercially viable 1-bit LLM family, delivering elite cognitive reasoning at the extreme edge.**

---

## ⚔️ Independent Execution
To pull and run this model natively on your silicon:
```bash
cluaiz run bonsai:1.1
```

---

## 📊 Model Matrix (Silicon Specs)
| Attribute          | Specification             |
| :----------------- | :------------------------ |
| **Version**        | v1.1 (Industrial Release) |
| **Parameters**     | 8.2 Billion               |
| **Architecture**   | True 1-bit Neural Kernel  |
| **Memory (VRAM)**  | 1.15 GB (Ultra-Lite)      |
| **Download Size**  | 1.02 GB                   |
| **Context Window** | 256K Tokens               |

---

## 🎨 Visual Identity & Benchmarks

<!-- ![Independent Banner](../../poster.webp) -->

### Intelligence Density Analysis
Bonsai 8B redefines the Pareto frontier by focusing on **Intelligence per GB**, ensuring high-accuracy reasoning on mobile and edge devices.

![Intelligence Density Comparison](./assets/Intelligence%20density%20(per%20GB)%20of%201-bit%20Bonsai%208B%20compared%20to%20other%20models%20in%20the%20same%20parameter%20class.%20.png)
*Fig I: Intelligence density comparison vs standard full-precision 8B models.*

![Benchmark Scores](./assets/The%20benchmark%20scores%20of%201-bit%20Bonsai%208B%20compared%20to%20other%20models%20in%20the%20same%20parameter%20class..png)
*Fig II: Benchmark accuracy across MMLU, GSM8K, and HumanEval.*

---

## 🧠 The Independent Narrative: Concentrating Intelligence
In an era of bloated models, **1-bit Bonsai** represents a paradigm shift. Developed at the intersection of Caltech research and PrismML's Independent hardware principles, this model is built for "Intelligence Density."

### Beyond Quantization
Unlike standard models that lose reasoning capability when squeezed, Bonsai is trained from the ground up as a **True 1-bit Model**. Every layer—from embeddings to attention mechanisms—operates on binary logic. This results in a **14x reduction in size** without the typical performance degradation of low-bit quantization.

### Silicon Excellence (5/10 TPS Rule)
Bonsai is engineered to maintain a minimum of **5 tokens per second (TPS)** on entry-level hardware and over **10 TPS** on professional silicon. Its extreme energy efficiency (0.068 mWh/tok) makes it the ideal choice for persistent on-device agents and real-time robotics.

---

## ⚖️ Independent Usage Rules
- **Privacy First**: Local-only inference. No data exfiltration allowed.
- **Hardware Aware**: Best suited for devices with NPU/GPU acceleration.
- **Licensing**: Apache 2.0 (Weights) | Independent Metadata License (Assets).

---

**Join the Independent Revolution.**
Published by **PrismML** & **Cluaiz-OS**. Supported by Khosla Ventures and Google DeepMind.
Full Technical Whitepaper: [Bonsai-Whitepaper.pdf](./assets/bonsai-whitepaper.pdf)
