<!-- name: agents/src/mcp
description: The Engine as an MCP client: starts installed MCP servers, keeps each running, lists and calls their tools. -->

# MCP client (`agents/src/mcp/`)

The Engine as an MCP **client**: it connects to installed MCP servers over stdio (a long-lived process) or
Streamable HTTP (a URL; stateless on spec 2026-07-28 servers, a session on older ones), lists their tools and calls them. (The Engine as an MCP **server** is `api/src/cep/mcp.rs`
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

## Two eras of the HTTP transport

Read on 2026-11-27: spec 2026-07-28 changelog, "Streamable HTTP" (including "Custom Headers from Tool
Parameters"), "Tools" (`x-mcp-header`), "Versioning and Compatibility" and "Discovery" (modelcontextprotocol.io/specification/2026-07-28/...). The spec calls versions from 2026-07-28
"modern" and 2025-11-25 and earlier "legacy".

| Spec text (2026-07-28) | What the client does |
|---|---|
| "Remove protocol-level sessions and the `Mcp-Session-Id` header from the Streamable HTTP transport" | in modern mode no session id is read, sent or deleted; in legacy mode a session is kept only when the server returns one |
| "Make MCP stateless: remove the `initialize`/`notifications/initialized` handshake. Every request now carries its protocol version and client capabilities in `_meta`" | every modern request gets `io.modelcontextprotocol/protocolVersion` `2026-07-28`, `clientInfo` and `clientCapabilities` (`{}`: no roots, sampling or elicitation offered) in `params._meta` |
| "Every POST request to the MCP endpoint MUST include an `MCP-Protocol-Version` header" matching the `_meta` value | sent on every modern request |
| "Standard Request Headers": `Mcp-Method` on all requests, `Mcp-Name` for `tools/call`, `resources/read`, `prompts/get` | both sent; a value that is not plain ASCII (or is shaped like the sentinel) goes as `=?base64?...?=` |
| "A client ... MAY detect which era the server implements by attempting a modern request first. On 400 Bad Request, the client SHOULD inspect the response body before falling back" | the first request is `server/discover`; see the decision table below |
| `UnsupportedProtocolVersionError` code `-32022` with `data.supported` | a list with only older versions: legacy path; only newer versions: clear error, no fallback |
| "All results now carry a required `resultType` field ... Clients MUST treat results from earlier-protocol servers that omit the field as complete" | a missing field is complete; `input_required` (multi round-trip) is refused with a clear error because no input is offered |
| "While the use of `x-mcp-header` is optional for servers, clients **MUST** support this feature. When a server's tool definition includes `x-mcp-header` annotations, conforming clients **MUST** mirror the designated parameter values into HTTP headers." (Streamable HTTP, "Custom Headers from Tool Parameters") | `tools/call` on a modern server gets `Mcp-Param-{name}` for each annotated parameter; the annotations are read from `tools/list`, and a `tools/call` in a session that has not listed yet lists first (`learn_tools`) |
| "If no value is present at that path in the call arguments, the header is omitted." and, in the scenario table, "Parameter value is `null`: Client MUST omit the header" | the value is read at the exact `properties` path (nested objects work); missing or `null` gives no header |
| "Clients **MUST** encode parameter values ... `integer`: Convert to decimal string representation ... `boolean`: Convert to lowercase "true" or "false"" and non-ASCII, control characters or leading/trailing whitespace "**MUST** use Base64 encoding" as `=?base64?...?=` | same `header_value` as `Mcp-Name`; integers as decimal text, booleans `true`/`false`; test value `héllo` goes as `=?base64?aMOpbGxv?=` |
| "Clients using the Streamable HTTP transport **MUST** reject tool definitions where any `x-mcp-header` value violates these constraints. Rejection means the client **MUST** exclude the invalid tool from the result of `tools/list`. Clients **SHOULD** log a warning ..." | an invalid tool is removed from the `tools/list` result and a warning names the tool and the reason (`param_annotations`); the other tools stay |
| Constraints: not empty; `1*tchar` token; no control characters; case-insensitively unique; only `integer`, `string`, `boolean` (`number` not permitted); only "statically reachable ... via a chain consisting solely of `properties` keys" (not through `items`, `oneOf`/`anyOf`/`allOf`/`not`, `if`/`then`/`else`, `$ref`) | each is checked; an annotation anywhere else (found by counting every `x-mcp-header` keyword against the reachable ones) makes the tool invalid. The safe-integer range is not checked on send (see Pending) |
| "Remove SSE stream resumability" and the GET stream; server requests on a stream replaced by multi round-trip | nothing to do: the client never used GET or `Last-Event-ID`; legacy server requests on a stream are still answered as before |

How the first request (`server/discover`) decides (`probe_modern` in `http.rs`):

| Answer | Result |
|---|---|
| result whose `supportedVersions` has `2026-07-28` | modern; no `initialize` is sent |
| 4xx or a JSON-RPC error that is not `-32020`..`-32022` (an old server does not know the method) | legacy: `initialize` as before |
| `-32020`..`-32022` listing only versions older than 2026-07-28 | legacy |
| `-32020`..`-32022` listing only newer versions, or `supportedVersions` only newer | error, no fallback |
| other `-32020`..`-32022` | error with the server's message |
| connect failure, timeout, 5xx | error (the era is unknown) |

## HTTP transport, legacy era (`http.rs`), what is implemented

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
| `names_are_plain_or_base64_as_the_spec_shows` (`http.rs`) | `Mcp-Name` encoding gives the spec's own table values |
| `x_mcp_header_valid_schema_gives_paths_including_nested`, `x_mcp_header_violations_make_the_tool_invalid` (`http.rs`) | every constraint above: empty, bad token, CR/LF, `number`, non-primitive, no type, not a string, case-insensitive duplicate, under `items`, `anyOf`, `$defs`; a property merely named `x-mcp-header` is not an annotation |
| `tests/mcp_http.rs` (7 tests, serialised by a lock because the client keeps at most 4 servers, own local HTTP server, no network) | through `McpClient`: `initialize` with the session id and negotiated version sent on every later request, `Accept` and custom headers, `tools/list`, `tools/call` with a JSON reply and with an event-stream reply (a notification first), a DELETE with the session id on stop, 500 with body, an HTML reply, 404 at `initialize`, connection refused, a bad scheme, and no secret in an error; `x_mcp_header_values_are_mirrored_and_invalid_tools_are_left_out`: a call before any listing lists first, then sends `Mcp-Param-Region/Count/Flag/Text/Level` (string, integer, boolean, base64 non-ASCII, nested), no header for a `null` value, and `list_tools` leaves out the duplicate-name and `number` tools; a stateless 2026-07-28 server (`server/discover`, `_meta` and `Mcp-Method`/`Mcp-Name` on every request, no `initialize`, no session, no DELETE); the fallback to legacy when the server's modern error lists only older versions; no fallback when it lists only newer ones. Rerun: `cargo test -p cluaiz-agents --test mcp_http` |
| `tests/mcp_reference_server.rs` (ignored) | the client against a real stdio server (`@modelcontextprotocol/server-everything`); `cargo test -p cluaiz-agents --test mcp_reference_server -- --ignored --nocapture` |

`cargo test -p cluaiz-agents` on 2026-11-27: 189 unit tests passed (1 ignored), `mcp_http` 7 passed. The reference-server test was not run
that day. MCP over the Engine's own CEP server: `test/cep/mcp_journey.py`.

## The Engine's own MCP server against 2026-07-28 (checked, not changed, 2026-11-27)

`api/src/cep/mcp.rs` speaks the legacy era only. Gaps against the 2026-07-28 text: `VERSIONS` lists
2025-11-25 at newest (`mcp.rs:22`); no `server/discover`, which "servers MUST implement" (`answer`,
`mcp.rs:82-113`, falls to method not found at line 112); `_meta` is dropped (`mcp.rs:137`) so the version in it
and `MCP-Protocol-Version` are never checked and `UnsupportedProtocolVersionError` is never returned; no
`Mcp-Method`/`Mcp-Name` validation; no `resultType`, `ttlMs`, `cacheScope`; an unknown method answers 200
with a JSON-RPC error (`mcp.rs:139-148`) where the spec wants 404. No `Origin` check ("Servers MUST validate
the `Origin` header ... respond with HTTP 403"): the only Origin handling is CORS `allow_origin(Any)`
(`api/src/routes.rs:29`), and `allow_headers` (`routes.rs:22-26`) lacks the `MCP-*` headers, so a browser
client could not send them. Already fine: only POST is routed (`routes.rs:70`, so GET/DELETE get 405), no
session id is minted, notifications get 202. Auth and bind defaults are untouched.

## Pending

| Item | Why it matters | Needs |
|---|---|---|
| `cluaiz mcp add` tested only as unit tests | the five tests in `installer/http_mcp.rs` check the written file with `McpManifestParser`, not a real `cluaiz mcp add` then a tool call; `cargo build -p cmd` was not run when it was written | a run in an isolated `CLUAIZ_HOME`: `mcp add`, `mcp list`, `mcp config`, then one tool call against a real HTTP server, saved under `test/cloud/runs/` |
| A recorded run of `mcp_reference_server` | the real-server path has no checked-in result | one run, saved under `test/cloud/runs/` |
| HTTP transport tested only against the in-file test server | no run against a real HTTP MCP server yet | one run against a public Streamable HTTP server, saved under `test/cloud/runs/` |
| `x-mcp-header`: no safe-integer check (the `HeaderMismatch` retry is done: `request` in `http.rs` re-lists tools and retries the call once, test `header_mismatch_relists_tools_and_retries_the_call_once`) | spec: "the client **SHOULD** call `tools/list` to check for changes to the tool's `inputSchema`, then retry the original request"; integers "**MUST** be within the safe range" (-2^53+1 to 2^53-1). A call after the server changed a schema fails once with `-32020`; an integer outside the range is sent as written | refuse an out-of-range integer |
| `x-mcp-header` tested only against the in-file server | the test server checks the headers itself | one run against a real 2026-07-28 server that uses the annotation, saved under `test/cloud/runs/` |
| No `subscriptions/listen` | modern servers send change notifications only on that stream, so `tools/list_changed` is not received; the tool cache is cleared only by `clear_tool_cache` | open the stream and call `on_list_changed` |
| Modern mode untested against a real server | only the in-file test server; the legacy fallback was also tested only locally | one run against a public 2026-07-28 server and an older one, saved under `test/cloud/runs/` |
| `ttlMs` / `cacheScope` ignored | the tool list is cached by the Engine's own rules | honour the freshness hint |
| Legacy `initialize` still offers 2025-06-18 | 2025-11-25 was read on 2026-11-27 (changelog and its Streamable HTTP page). Nothing in its changelog needs a client change for what this client sends (no elicitation, sampling or tasks offered; the `Origin` 403 rule is for servers), except SEP-1699: a 2025-11-25 server "**MAY** close the *connection* ... at any time" after an event id, and "The client **SHOULD** then "poll" the SSE stream by attempting to reconnect", "**MUST** respect the `retry` field", resuming "via HTTP GET with `Last-Event-ID`". This client reads a POST reply stream once and fails with "closed the event stream without a response", so offering 2025-11-25 would let such a server break a call that works today with 2025-06-18 | GET with `Last-Event-ID` after a closed stream, waiting the `retry` milliseconds; then offer 2025-11-25 first and keep 2025-06-18 accepted |
| Old HTTP+SSE transport (2024-11-05) not implemented | servers that only speak it (a GET that returns an `endpoint` event) cannot be used | the fallback in the spec's "Backwards Compatibility" section; skipped because it is a second transport |
| No standalone GET stream | server messages outside a POST reply (for example a `list_changed` between calls) are not received; the tool cache is only cleared when the message comes inside a reply | `GET` with `Last-Event-ID` resume |
| No OAuth | servers that need the MCP authorization flow cannot be used; a fixed token in `headers` works | the authorization flow of the spec |
| No `CancelledNotification` on timeout | after a timeout the server may keep working on the call | send it on timeout |
| An HTTP server is not under the OS sandbox or the `network` permission | it is a remote service, not a process of ours; each call is still asked per `registry/types.rs` | decide whether `network` should gate HTTP packages |
| `url` is read only from the package's own `execution` block | the parser's `versions` fallback does not read a `url` | read it there too if a registry package needs it |
