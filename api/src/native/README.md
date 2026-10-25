# Local native API transport

`NativeListener` binds one caller-provided Windows named pipe or Unix socket.
It carries HTTP/1 request, response and streaming frames through Hyper and then
calls the existing Axum router. It never opens a TCP port, maps another route
table, or owns a second model runtime.

```mermaid
flowchart LR
    A[App native client] --> B[Explicit local OS endpoint]
    B --> C[Mandatory session token check]
    C --> D[Existing Axum router and body limits]
    D --> E[Existing domain handlers and EngineHost]
```

The CLI selects this mode with `cluaiz serve --native-endpoint <endpoint>`.
Plain `cluaiz serve` keeps its HTTP behavior. The client must supply the Engine's
current session Bearer token even for `/health`, MCP, OPTIONS, or profiles with
ordinary HTTP authentication disabled. This is host-level transport authority;
it is not a per-extension credential or sandbox grant.

Windows endpoints must be explicit local `\\.\pipe\<identifier>` names. The
first server claims exclusive initial ownership, remote clients are rejected,
and the next pipe instance exists before serving a connected client. Unix
endpoints require an absolute path under an owned private directory; socket
permissions are 0600. Existing paths are never unlinked during bind. Shutdown
unlinks only the Unix socket whose device/inode still matches this listener.

At most 32 connection tasks are admitted. Header parsing has a ten-second
deadline and bounded buffer. Shutdown asks active HTTP connections to finish,
then stops them within a bounded grace period. Dropping the HTTP body follows
the existing handler cancellation semantics; native inference is not forcibly
terminated by this transport. Streaming and binary handling preserve existing
endpoint contracts; transport support does not add raw-audio routes or models.

`api/tests/native_contract.rs` exercises OS streams without loading models.
Windows tests execute on Windows; Unix permission/cleanup cases require a Unix
run. No platform is declared tested solely from the other platform's results.
