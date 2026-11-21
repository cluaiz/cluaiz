# Component: `agents/src/mcp/`

## Technical Specification

- **Purpose:** The Engine as an MCP **client**: it starts installed MCP servers (stdio), keeps
  each one running as one long-lived process, lists its tools and calls them. (The Engine as an
  MCP **server** is `api/src/cep/mcp.rs` and `cluaiz mcp-server`.)
- **Platform Support:** Windows (a job object ends the server and everything it started),
  Linux, macOS.
- **OS sandbox:** each server is started through `execution::os_sandbox::confine_here` with
  `SandboxPolicy::for_tool` (secret folders and the Cluaiz home hidden, its package folder read-only,
  the workspace writable); its network follows its declared `network` permission (`tool_network`).
  Platform backends and modes: [execution/README.md](../execution/README.md#os-sandbox-os_sandboxrs-2026-11-20).
- **Reusability Level:** Internal to `cluaiz-agents`; used by `ToolsEngine`, tool search (tool
  lists) and the API chat.

## Architectural Flow

![MCP server process: started on the first call, reused, stopped after 10 idle minutes or when more than 4 run; restarted after 1 s then 2 s; unavailable after 3 crashes in 10 minutes](../../../docs/assets/engine/mcp-server-life.png)


```mermaid
flowchart LR
    CALL["call_tool / list_tools<br/>(server folder)"]
    MAN["manifest.rs<br/>McpManifestParser: command, args, execution.env"]
    SESS["SESSIONS<br/>one running process per server"]
    START["start: cleared env + tool values<br/>initialize once"]
    RPC["JSON-RPC over stdio<br/>tools/list (paged) · tools/call"]
    IDLE["idle 10 min → stop<br/>more than 4 → stop least recent"]
    CRASH["CRASHES<br/>backoff 1 s, 2 s; 3 in 10 min → unavailable"]

    CALL --> SESS
    SESS -->|"not running"| START --> RPC
    MAN --> START
    SESS -->|"running"| RPC
    SESS --> IDLE
    RPC -->|"process exited"| CRASH --> SESS
```

## API Contract (Interface)

- **`client.rs`:** `McpClient::{list_tools, call_tool, get_cached_tools, clear_tool_cache,
  stop_server, is_unavailable}`; `Job` (Windows job object, a no-op elsewhere).
- **`manifest.rs`:** `McpManifest`, `McpDiscovery`, `McpActivation`, `McpExecution`,
  `McpManifestParser::parse_file`.
- **Export Type:** Rust, inside `cluaiz-agents`.
- **Dependencies:** `tokio` (process, timers), `serde_json`, `crate::execution::environment`
  (`for_tool`), `crate::tool_env`; `windows-sys` for the job object.

## Deep File Breakdown

- `manifest.rs`: reads the server's `package.json`: how to start it (`command`, `args`), its
  fixed `execution.env`, discovery and activation fields.
- `client.rs`:
  - **Start:** the process starts from a cleared environment (system basics, the server's
    `execution.env`, the values named in its `env`), is initialized once, then serves any number
    of requests on the same connection.
  - **Timeouts:** a request waits 60 s; the first call of a new process waits up to 180 s,
    since a first start can include a package download (`npx -y`, `uvx`).
  - **Running servers:** a server with no request for 10 minutes is stopped and started again on
    its next call; at most 4 run at once, and starting one more stops the least recently used.
  - **Tool lists:** `tools/list` is read page by page (at most 50 pages) and cached per server;
    `clear_tool_cache` drops the cache.
  - **Windows:** ending the launcher (`npx.cmd`, `uvx`) would leave the real server running and
    holding handles it inherited (such as a listening port); the job object ends all of them
    when the server stops and when the Engine exits.
  - **Why:** Reusing one process per server keeps calls fast; the limits keep idle servers from
    holding memory.

## Failure & Recovery Logic

- **Server exits on its own (a crash):** recorded; it is started again after 1 s, then 2 s; after
  3 crashes within 10 minutes it is not started until the window passes, and
  `is_unavailable` is true (the tool is offered as unavailable).
- **Request timeout:** the call fails with a timeout error; the process keeps running.
- **Start timeout:** the start fails after 180 s; the error names the server.
- **Server not allowed a permission:** asked before each call (`registry/types.rs`). The server
  process itself is confined by the OS sandbox where it is enabled (Technical Specification above);
  with `os_sandbox: off`, or on Windows in `auto`, it runs with the user's rights.

## Decisions and why

| Decision | Why | Alternative not taken |
|---|---|---|
| One long-lived process per server, reused for every call | starting `npx` / `uvx` servers takes seconds (the first start may download a package) | a process per call |
| At most 4 servers running, idle stop after 10 minutes | each server holds memory while idle | keep every started server |
| Crash backoff 1 s, 2 s, then unavailable after 3 crashes in 10 minutes | a server that crashes on start would otherwise be restarted in a loop on every call | restart without limit |
| Windows job object around each server | ending `npx.cmd` left the real server running and holding inherited handles (a listening port) | kill the launcher only |

## Tested

| Test | What it proves |
|---|---|
| `user_env_gives_only_the_named_values_of_this_package` | a server gets only the values its manifest names |
| `expose_limits_tools_and_inputs` | with an `expose` list, only the listed tools and their listed inputs are shown to the model, and a call outside it is refused whatever sends it (`only_exposed`, `check_exposed`) |
| `test_restart_backoff_and_unavailable` | the 1 s / 2 s backoff and the unavailable state after 3 crashes |
| `tests/mcp_reference_server.rs` (ignored) | the client against a real stdio server (`@modelcontextprotocol/server-everything`); `cargo test -p cluaiz-agents --test mcp_reference_server -- --ignored --nocapture` |

Part of `cargo test -p cluaiz-agents`: 161 passed on 2026-11-21. The reference-server test was not run
that day. MCP over the Engine's own CEP server: `test/cep/mcp_journey.py`.

## Pending

| Item | Why it matters | Needs |
|---|---|---|
| A recorded run of `mcp_reference_server` | the real-server path has no checked-in result | one run, saved under `test/cloud/runs/` |
| Only stdio servers | MCP servers that speak HTTP (streamable HTTP) cannot be installed | an HTTP transport in `client.rs` |
