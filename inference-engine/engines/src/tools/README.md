# 🏛️ Tools Modular Subsystem (`engines/src/tools/`)

## 1. Architectural Mandate: Single Domain Sovereignty

Just like Model architecture is self-contained in `engines/src/models/` and Hardware Expert Offloading is self-contained in `cluaiz-shared/src/hardware/expert_offloading/`, **all Tool logic** (Skills, Plugins, MCP, Registry, Turn Lifecycle, and Telemetry) is unified within this modular domain:

$$\mathbf{engines/src/tools/}$$

---

## 2. Directory Layout & Subsystems

| Subsystem | Folder | Responsibility |
| :--- | :--- | :--- |
| **Facade** | [`mod.rs`](./mod.rs) | `ToolsEngine` public facade providing a clean API for Handlers and Engine. |
| **Registry** | [`registry/`](./registry/) | Manages `tools_registry.json` and fast binary cache `tools_registry.bin`. Auto-probes `~/.cluaiz/tools/`. |
| **Installer** | [`installer/`](./installer/) | Downloads, caches, installs, and uninstalls packages from Cluaiz Hub. |
| **Skills** | [`skills/`](./skills/) | Parses `SKILL.md` frontmatter and provides O(1) keyword and semantic routing. |
| **Plugins** | [`plugins/`](./plugins/) | Parses `manifest-plugin.yaml` and executes WASM binaries and native libraries. |
| **MCP** | [`mcp/`](./mcp/) | Parses `manifest-mcp.yaml` and handles JSON-RPC 2.0 subprocess communication over stdio. |
| **Lifecycle** | [`lifecycle/`](./lifecycle/) | Tracks session-bound active tools, turn countdowns (`-1`, `0`, `N`), and auto-unloading. |
| **Telemetry** | [`telemetry/`](./telemetry/) | Calculates active token breakdowns, lazy loading savings, and KV-cache VRAM allocation. |

---

## 3. Standardized Filesystem Root (`~/.cluaiz/`)

```
~/.cluaiz/
├── engine/config/
│   ├── model_registry.json              <── Master record for GGUF & ONNX models
│   ├── tools_registry.json              <── Master record for all installed tools
│   └── tools_registry.bin               <── Fast-boot binary Bincode cache
└── tools/                               <── Master Tools domain folder
    ├── skills/                          <── Installed Cognitive Skills
    │   └── frontend-dev/SKILL.md
    ├── plugins/                         <── Installed WASM / Native Tool Binaries
    │   └── cluaiz-search/plugin.wasm & manifest-plugin.yaml
    └── mcp/                             <── Installed MCP Servers
        └── sqlite-bridge/manifest-mcp.yaml
```
