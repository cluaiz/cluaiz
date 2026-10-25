# Master Reference Index

Complete reference index for the cluaiz engine — all specifications, APIs, CLI contracts, and configuration profiles in one place.

---

## 📚 Reference Sections

### Reference Index
Core language and system specifications:
* **[C-Pointer Specifications](../reference/c-pointer-fii)** — FFI boundary contracts and memory layout rules.
* **[System Dictionary](../reference/dictionary)** — Global type registry and term definitions.

### CLI Reference
* **[Core CLI Manual](../reference/terminal-commands)** — Full command argument reference and shell integration.

### HTTP API Reference
* **[Unified API Reference](../reference/api)** — The routes and CEP (`/v1/cep`), with links to every method.

### Configuration Profiles
* **[Engine Configurations](../reference/engine-configuration)** — `llm_optimization.json` and `system_control.json` field-level specs.

---

## 🗺️ Architecture Quick Reference

```mermaid
flowchart LR
    CLI["💻 CLI\ncluaiz-cli"]
    ENGINE["⚙️ Engine\nAxum + Tokio"]
    CEP["🔗 CEP\nJSON-RPC 2.0"]
    FFI["🔌 FFI Bridge\nC / WASM"]
    DRIVER["🖥️ Driver\nCUDA / Metal"]

    CLI -->|"HTTP"| ENGINE
    ENGINE --> CEP
    ENGINE --> FFI
    FFI --> DRIVER
```

---

## 📋 Document Status

| Document | Status | Path |
|---|---|---|
| C-Pointer FFI | ✅ Complete | `docs/reference/c-pointer-fii.md` |
| System Dictionary | ✅ Complete | `docs/reference/dictionary.md` |
| CLI Manual | ✅ Complete | `docs/reference/terminal-commands.md` |
| API Reference | ✅ Complete | `docs/reference/api.md` |
| Engine Config | ✅ Complete | `docs/reference/engine-configuration.md` |
