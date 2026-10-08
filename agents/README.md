# 🤖 Cluaiz Agents Subsystem (`agents/`)

Orchestration layer for Skills, Plugins, MCP (Model Context Protocol), and Tool executions.

## 📐 Architecture

```mermaid
graph TD
    Agent[Agent Orchestrator] --> Skills[skills/: Skills Router]
    Agent --> Plugins[plugins/: WASM & Native Plugins]
    Agent --> MCP[mcp/: Model Context Protocol]
    Agent --> Execution[execution/: Process & Sandbox Execution]
```

## 📦 Components

| Module | Responsibility |
|---|---|
| [`skills/`](src/skills/) | Semantic and regex-based routing of local developer skills. |
| [`plugins/`](src/plugins/) | Dynamic plugin loading and sandboxed execution. |
| [`mcp/`](src/mcp/) | Standard Model Context Protocol client implementation. |
| [`execution/`](src/execution/) | Safe process invocation and sandboxing. |
