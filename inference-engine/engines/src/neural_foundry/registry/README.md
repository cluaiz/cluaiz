# Component: Plugin, Skill & MCP Registry

## Technical Specification
- **Purpose:** Centralized index and loader for plugins, skills, and MCP servers, utilizing O(1) semantic indexing via `registry.yaml` and compiled `.bin` caching for zero-latency boots.
- **Platform Support:** Cross-platform (Windows, Linux, macOS)
- **Reusability Level:** High (Global Engine Registry)

## API Contract (Interface)
- **Props/Struct/Trait:** `MasterRegistry`, `PluginManager`, `SkillRegistry`, `McpGateway`
- **Export Type:** Public Module (`registry`)
- **Dependencies:** `inference-cel` (Manifest schema parser), `bincode`, `serde_yaml`

## Failure & Recovery Logic
- **Potential Failure Point:** The `registry.yaml` or a component's binary path goes missing from the OS disk while still marked enabled.
- **Recovery Logic:** The Engine detects `LoadStrategy::Lazy` failures at runtime, logs the missing binary path, disables the plugin dynamically in the `MasterRegistry`, and safely aborts execution without crashing the active inference loop.
