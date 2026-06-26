# `cluaize pull` Command Reference

The `pull` command downloads and registers a specified model into the local model vault.

---

## 📋 Syntax & Usage

```bash
cluaize pull <model-identifier>
```

### Examples:
```bash
cluaize pull qwen3.5:8b
cluaize pull unsloth/Qwen3.5-4B-GGUF
```

---

## ⚙️ Execution Flow

1. **Metadata Query:** Queries the Cluaize package registry to find the HuggingFace URL, file sizes, and expected checksum hashes.
2. **Space Check:** Validates available disk space on the workstation node partition.
3. **Chunked Download:** Performs a chunked, resume-supported HTTPS stream to download the GGUF file directly into `~/.cluaize/models/chat/`.
4. **Validation:** Computes the SHA-256 hash of the downloaded binary and registers it to prevent corruption.
