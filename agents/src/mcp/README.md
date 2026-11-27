<!-- name: agents/src/mcp
description: The Engine as an MCP client: starts installed MCP servers, keeps each running, lists and calls their tools. -->

# MCP client (`agents/src/mcp/`)

The Engine as an MCP **client**: it connects to installed MCP servers over stdio (a long-lived process) or
Streamable HTTP (a session at a URL), lists their tools and calls them. (The Engine as an MCP **server** is `api/src/cep/mcp.rs`
and `cluaiz mcp-server`.)

## At a glance

- **Platform Support:** Windows (a job object ends the server and everything it started),
  Linux, macOS.
- **OS sandbox:** each server is started through `execution::os_sandbox::confine_here` with
  `SandboxPolicy::for_tool` (secret folders and the Cluaiz home hidden, its package folder read-only,
  the workspace writable); its network follows its declared `network` permission (`tool_network`).
  Platform backends and modes: [execution/README.md](../execution/README.md#os-sandbox-os_sandboxrs-2026-11-20).
- **Reusability Level:** Internal to `cluaiz-agents`; used by `ToolsEngine`, tool search (tool
  lists) and the API chat.

## How it works

![MCP server process: started on the first call, reused, stopped after 10 idle minutes or when more than 4 run; restarted after 1 s then 2 s; unavailable after 3 crashes in 10 minutes](../../../docs/assets/engine/mcp-server-life.png)

## Interface
- **`client.rs`:** `McpClient::{list_tools, call_tool, get_cached_tools, clear_tool_cache,
  stop_server, is_unavailable}`; `Job` (Windows job object, a no-op elsewhere).
- **`http.rs`:** `HttpSession`, the Streamable HTTP transport.
- **`manifest.rs`:** `McpManifest`, `McpDiscovery`, `McpActivation`, `McpExecution`,
  `McpManifestParser::parse_file`.
- **Export Type:** Rust, inside `cluaiz-agents`.
- **Dependencies:** `tokio` (process, timers), `serde_json`, `crate::execution::environment`
  (`for_tool`), `crate::tool_env`; `windows-sys` for the job object.

## Stdio or HTTP: the `execution` block

The package's `package.json` says how to reach the server. Which form is used depends on which field is set:

```json
{ "execution": { "command": "npx", "args": ["-y", "some-mcp-server"] } }
```

```json
{ "execution": { "url": "https://example.com/mcp",
                 "headers": { "Authorization": "Bearer ${EXAMPLE_TOKEN}" } },
  "env": { "EXAMPLE_TOKEN": { "info": "API token of the server" } } }
```

- Add one from the CLI, with no file edited: `cluaiz mcp add <name> --url <url> [--header "Name: value"]`
  (code: `installer/http_mcp.rs`). It writes exactly this `package.json` (and declares each `${NAME}` in
  `env`), refuses a name that is installed, and `cluaiz mcp list` shows the url. Why not the registry
  install: that downloads a bundle from cluaiz-tools, and a remote server has nothing to download.
- `url` set: HTTP. `command` and `args` are ignored (a warning is logged when both are set).
- `headers` are sent with every request. `${NAME}` in a value is replaced by the setting `NAME`
  (declared in the package's `env`, set with `cluaiz mcp config <id>` or the system environment), so
  a token is not stored in the package file. A name with no value fails the start and names the setting.
- Tool listing, tool calls, `expose`, the tool cache, the 4-server limit and the idle stop work the same
  for both forms (`Running` in `client.rs`).

## HTTP transport (`http.rs`), what is implemented

From the MCP spec 2025-06-18, "Transports" > "Streamable HTTP" (read 2026-11-26). The 2025-03-26 version
was not read separately; the client also reads a JSON array reply, which that version allows.

| Spec text | What the client does |
|---|---|
| "Every JSON-RPC message sent from the client MUST be a new HTTP POST request to the MCP endpoint." | one POST per message |
| "The client MUST include an Accept header, listing both application/json and text/event-stream" | sent as `application/json, text/event-stream` |
| "the server MUST either return Content-Type: text/event-stream ... or Content-Type: application/json ... The client MUST support both these cases." | both are read; for an event stream, events are read until the response with the request's id arrives; other messages in the stream are handled (below) |
| response or notification accepted: "HTTP status code 202 Accepted with no body" | any 2xx is accepted for `notifications/initialized` and for the client's answers to server requests |
| "include it in the Mcp-Session-Id header on all of their subsequent HTTP requests" | the id from the `initialize` response is sent on every later request |
| "the client MUST include the MCP-Protocol-Version ... header on all subsequent requests" and it "SHOULD be the one negotiated during initialization" | sent after `initialize`, with the version the server answered with |
| "When a client receives HTTP 404 in response to a request containing an Mcp-Session-Id, it MUST start a new session by sending a new InitializeRequest" | the session is marked ended and the call fails with a clear error; the next call opens a new session |
| clients "SHOULD send an HTTP DELETE to the MCP endpoint with the Mcp-Session-Id header" | sent when the server is stopped (removed, replaced, idle stop, more than 4 servers); not waited for; a 405 is ignored |

Messages a server puts in an event stream before the response: a `roots/list` request is answered with
the open workspace (as a separate POST), other requests get "method not found", `notifications/tools/list_changed`
clears the tool cache, other notifications are logged.

Errors: HTTP status and the first 200 characters of the body (`HTTP 500: boom`), a body that is not JSON,
an unexpected content type, a connect failure, and timeouts (60 s a request, 180 s for the first one).
Messages show the URL without its query string, and header values are never put in a message.

## Files
- `manifest.rs`: reads the server's `package.json`: how to start it (`command`, `args`) or where it is
  (`url`, `headers`), its fixed `execution.env`, discovery and activation fields.
- `http.rs`: the HTTP transport described above.
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

## Failure and recovery
- **Server exits on its own (a crash):** recorded; it is started again after 1 s, then 2 s; after
  3 crashes within 10 minutes it is not started until the window passes, and
  `is_unavailable` is true (the tool is offered as unavailable).
- **Request timeout:** the call fails with a timeout error; the process (or HTTP session) keeps running.
- **HTTP server ends the session (404):** the call fails; the session is dropped and the next call sends a new `initialize`.
  Not counted as a crash. The failed call is not retried by itself.
- **HTTP error, non-JSON reply, unreachable server:** the call fails with the status, content type or
  connect error in the message; nothing is restarted, the next call tries again.
- **Start timeout:** the start fails after 180 s; the error names the server.
- **Server not allowed a permission:** asked before each call (`registry/types.rs`). The server
  process itself is confined by the OS sandbox where it is enabled (Technical Specification above);
  with `os_sandbox: off`, or on Windows in `auto`, it runs with the user's rights.

## Decisions
| Decision | Why | Alternative not taken |
|---|---|---|
| One long-lived process per server, reused for every call | starting `npx` / `uvx` servers takes seconds (the first start may download a package) | a process per call |
| At most 4 servers running, idle stop after 10 minutes | each server holds memory while idle | keep every started server |
| Crash backoff 1 s, 2 s, then unavailable after 3 crashes in 10 minutes | a server that crashes on start would otherwise be restarted in a loop on every call | restart without limit |
| HTTP transport on the existing `reqwest` (no new dependency) | `reqwest` is already used by the installer; an event stream is read with `Response::chunk`, which needs no extra feature | a dedicated MCP SDK crate, or turning on reqwest's `stream` feature |
| `url` / `command` chosen by which field is set, in `execution` | the same package file and the same install path for both | a separate package type for HTTP servers |
| `${NAME}` in header values from the package's `env` settings | the existing settings store holds tokens already, and the package file stays free of secrets | tokens written in `headers` only |
| No redirects followed | a redirect would resend custom headers (tokens) elsewhere and turn a POST into a GET | follow redirects like a browser |
| A 404 with a session id ends the session; the next call reopens it | what the spec requires; simple to reason about | retry the failed call inside the same call |
| Windows job object around each server | ending `npx.cmd` left the real server running and holding inherited handles (a listening port) | kill the launcher only |

## Tested

| Test | What it proves |
|---|---|
| `user_env_gives_only_the_named_values_of_this_package` | a server gets only the values its manifest names |
| `expose_limits_tools_and_inputs` | with an `expose` list, only the listed tools and their listed inputs are shown to the model, and a call outside it is refused whatever sends it (`only_exposed`, `check_exposed`) |
| `test_restart_backoff_and_unavailable` | the 1 s / 2 s backoff and the unavailable state after 3 crashes |
| `header_values_expand_named_settings` | `${NAME}` in header values |
| `url_form_and_command_form_are_told_apart` (`manifest.rs`) | `execution.url` / `headers` are read; the command form is unchanged |
| `sse_events_split_on_blank_lines_and_join_data_lines`, `error_body_snippet_is_short` (`http.rs`) | event-stream parsing (LF and CRLF, several `data:` lines) and short error bodies |
| `tests/mcp_http.rs` (4 tests, own local HTTP server, no network) | through `McpClient`: `initialize` with the session id and negotiated version sent on every later request, `Accept` and custom headers, `tools/list`, `tools/call` with a JSON reply and with an event-stream reply (a notification first), a DELETE with the session id on stop, 500 with body, an HTML reply, 404 at `initialize`, connection refused, a bad scheme, and no secret in an error. Rerun: `cargo test -p cluaiz-agents --test mcp_http` |
| `tests/mcp_reference_server.rs` (ignored) | the client against a real stdio server (`@modelcontextprotocol/server-everything`); `cargo test -p cluaiz-agents --test mcp_reference_server -- --ignored --nocapture` |

`cargo test -p cluaiz-agents` on 2026-11-26: 181 unit tests passed (1 ignored), `mcp_http` 4 passed. The reference-server test was not run
that day. MCP over the Engine's own CEP server: `test/cep/mcp_journey.py`.

## Pending

| Item | Why it matters | Needs |
|---|---|---|
| `cluaiz mcp add` tested only as unit tests | the five tests in `installer/http_mcp.rs` check the written file with `McpManifestParser`, not a real `cluaiz mcp add` then a tool call; `cargo build -p cmd` was not run when it was written | a run in an isolated `CLUAIZ_HOME`: `mcp add`, `mcp list`, `mcp config`, then one tool call against a real HTTP server, saved under `test/cloud/runs/` |
| A recorded run of `mcp_reference_server` | the real-server path has no checked-in result | one run, saved under `test/cloud/runs/` |
| HTTP transport tested only against the in-file test server | no run against a real HTTP MCP server yet | one run against a public Streamable HTTP server, saved under `test/cloud/runs/` |
| Old HTTP+SSE transport (2024-11-05) not implemented | servers that only speak it (a GET that returns an `endpoint` event) cannot be used | the fallback in the spec's "Backwards Compatibility" section; skipped because it is a second transport |
| No standalone GET stream | server messages outside a POST reply (for example a `list_changed` between calls) are not received; the tool cache is only cleared when the message comes inside a reply | `GET` with `Last-Event-ID` resume |
| No OAuth | servers that need the MCP authorization flow cannot be used; a fixed token in `headers` works | the authorization flow of the spec |
| No `CancelledNotification` on timeout | after a timeout the server may keep working on the call | send it on timeout |
| An HTTP server is not under the OS sandbox or the `network` permission | it is a remote service, not a process of ours; each call is still asked per `registry/types.rs` | decide whether `network` should gate HTTP packages |
| `url` is read only from the package's own `execution` block | the parser's `versions` fallback does not read a `url` | read it there too if a registry package needs it |
