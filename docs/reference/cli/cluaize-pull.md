# `cluaiz pull` Command Reference

The `pull` command downloads and registers a specified model into the local model vault.

---

## 📋 Syntax & Usage

```bash
cluaiz pull <model-identifier>
```

### Examples:
```bash
cluaiz pull qwen3.5:8b
cluaiz pull unsloth/Qwen3.5-4B-GGUF
```

---

## ⚙️ Execution Flow

1. **Metadata Query:** Queries the cluaiz package registry to find the HuggingFace URL, file sizes, and expected checksum hashes.
2. **Space Check:** Validates available disk space on the workstation node partition.
3. **Chunked Download:** Performs a chunked, resume-supported HTTPS stream to download the GGUF file directly into `~/.cluaiz/models/chat/`.
4. **Validation:** Computes the SHA-256 hash of the downloaded binary and registers it to prevent corruption.
