# Component: Plugin & Extension Registry

## Technical Specification
- **Purpose:** Centralized index and loader for extensions, plugins, and MCP servers, utilizing O(1) semantic indexing via `registry.yaml` and compiled `.bin` caching for zero-latency boots.
- **Platform Support:** Cross-platform (Windows, Linux, macOS)
- **Reusability Level:** High (Global Engine Registry)

## API Contract (Interface)
- **Props/Struct/Trait:** `MasterRegistry`, `ExtensionManager`, `PluginManager`, `McpManager`
- **Export Type:** Public Module (`registry`)
- **Dependencies:** `inference-cel` (Manifest schema parser), `bincode`, `serde_yaml`

## Failure & Recovery Logic
- **Potential Failure Point:** The `registry.yaml` or a component's binary path goes missing from the OS disk while still marked enabled.
- **Recovery Logic:** The Engine detects `LoadStrategy::Lazy` failures at runtime, logs the missing binary path, disables the extension dynamically in the `MasterRegistry`, and safely aborts execution without crashing the active inference loop.
