# Native High-Speed Filesystem & Workspace Jail API Reference

> **Base Route:** `/v1/fs/*` & `/v1/workspace`  
> **Security Protocol:** Permanent Bearer Token Authentication + Canonical Workspace Jail  
> **Component:** `cluaiz_api::handlers::fs`, `cluaiz_api::handlers::fs_jail`

---

## 1. Security & Architecture Doctrine

The Cluaiz Filesystem API provides direct, high-speed NVMe/SSD filesystem operations tailored for local agent execution, code exploration, and workspace file manipulation. 

Because filesystem access interacts with local physical storage, it enforces an uncompromising, dual-layer defense-in-depth model:

```mermaid
flowchart TD
    Req["Filesystem Request (/v1/fs/*)"] --> AuthCheck{"Bearer Token Present\n& Cryptographically Valid?"}
    AuthCheck -->|No / Invalid| Block401["401 Unauthorized\n(Token Mandatory)"]
    
    AuthCheck -->|Valid Token| PathResolver["Path Resolver\n(resolve_workspace_path)"]
    PathResolver --> CanonCheck{"Canonicalize Path\n& Workspace Root"}
    
    CanonCheck --> Boundary{"Inside Active Workspace?"}
    
    Boundary -->|Yes| SecModeCheck{"Security Mode"}
    SecModeCheck -->|Strict / Sandboxed Reads| AllowAction["Execute FS Syscall (200 OK)"]
    SecModeCheck -->|Sandboxed Writes| PermGate{"Pre-Approved or Approval ID?"}
    PermGate -->|No| EmitPending["Queue PendingPermissionRequest\nPause Stream (SSE Event)"]
    PermGate -->|Yes| AllowAction
    
    Boundary -->|No (Escape Attempt)| ModeEscape{"Security Mode"}
    ModeEscape -->|Strict| Block403["403 Forbidden\n(Workspace Boundary Violation)"]
    ModeEscape -->|Sandboxed| QueueApproval["Queue Approval Request to User"]
    ModeEscape -->|Full Access| AllowAction
```

### 🔒 Core Security Rules

1. **Permanent Bearer Token Enforcement:**
   * Unlike general inference endpoints (`/v1/chat/completions`) which can be opened for local prototyping, all `/v1/fs/*` routes **permanently require a valid Bearer token** (`Authorization: Bearer <token>`).
   * Supported tokens: Local Engine Session Token (`~/.cluaiz/session.token`) or configured API Key (`sk-cluaiz-...`).
   * Calls without a valid token immediately return `401 Unauthorized`.
2. **Workspace Canonical Jail:**
   * Relative paths (e.g. `src/main.rs`) are always anchored to the active `workspace_root`.
   * Candidate paths and workspace paths are normalized using `std::fs::canonicalize` to eliminate symlink attacks, directory traversal sequences (`../../`), and Windows extended-length prefix variations (`\\?\`).
   * For new write operations to non-existent files, the resolver walks up the parent chain until the nearest existing ancestor is located, canonicalizes it, and verifies workspace containment.
3. **Security Modes (`security_mode`):**
   * **`strict`**: Any operation targeting a path outside the workspace boundary is immediately rejected with `403 Forbidden` (`"Workspace Boundary Violation"`).
   * **`sandboxed` (Default)**: Workspace reads execute automatically. Writes, creations, deletions, and cross-boundary paths generate a `PendingPermissionRequest` and await human confirmation via the Approval Gate.
   * **`full_access`**: Unrestricted developer mode bypassing the jail check.

---

## 2. API Endpoints

### 2.1 Read File (`POST /v1/fs/read`)

Reads the contents of a file inside the active workspace. Automatically supports UTF-8 text strings and Base64-encoded binary payloads.

* **Path:** `/v1/fs/read`
* **Method:** `POST`
* **Headers:**
  * `Authorization: Bearer <token>` (**Required**)
  * `Content-Type: application/json`
  * `x-caller: <agent_id>` (*Optional*)

#### Request Payload
```json
{
  "path": "src/main.rs",
  "is_binary": false,
  "caller": "code_editor_agent",
  "approval_id": null
}
```

| Parameter | Type | Required | Description |
|---|---|---|---|
| `path` | `string` | **Yes** | Relative to workspace or absolute path within workspace. |
| `is_binary` | `boolean` | No | If `true`, returns Base64-encoded bytes. If `false` and file is valid UTF-8, returns text string. Defaults to `false`. |
| `caller` | `string` | No | Identifier of the calling subagent or tool. |
| `approval_id` | `string` | No | Pre-approved permission UUID (if resuming from HITL pause). |

#### Response (`200 OK`)
```json
{
  "status": "success",
  "content": "fn main() {\n    println!(\"Hello Cluaiz!\");\n}\n",
  "is_binary": false
}
```

---

### 2.2 Write File (`POST /v1/fs/write`)

Writes or overwrites file contents. Missing parent directories are created automatically.

* **Path:** `/v1/fs/write`
* **Method:** `POST`

#### Request Payload
```json
{
  "path": "src/utils/logger.rs",
  "content": "pub fn init_logger() { tracing_subscriber::fmt::init(); }",
  "is_binary": false,
  "caller": "code_editor_agent"
}
```

| Parameter | Type | Required | Description |
|---|---|---|---|
| `path` | `string` | **Yes** | Target file path. |
| `content` | `string` | **Yes** | Plain UTF-8 text or Base64 string if `is_binary: true`. |
| `is_binary` | `boolean` | No | Specify `true` when transmitting Base64 binary data. |

#### Response (`200 OK`)
```json
{
  "status": "success"
}
```

---

### 2.3 List Directory (`POST /v1/fs/list`)

Enumerates directory contents up to 12 levels deep. Automatically ignores high-noise directories (`node_modules`, `.git`, `target`, `.venv`, `.next`, `dist`, `build`).

* **Path:** `/v1/fs/list`
* **Method:** `POST`

#### Request Payload
```json
{
  "dir_path": "src",
  "recursive": true
}
```

#### Response (`200 OK`)
```json
{
  "status": "success",
  "items": [
    {
      "path": "src/main.rs",
      "name": "main.rs",
      "isFolder": false,
      "length": 1024,
      "modified": "SystemTime { tv_sec: 1735700000, tv_nsec: 0 }"
    },
    {
      "path": "src/utils",
      "name": "utils",
      "isFolder": true,
      "length": 0,
      "modified": "SystemTime { tv_sec: 1735700000, tv_nsec: 0 }"
    }
  ]
}
```

---

### 2.4 Delete Path (`POST /v1/fs/delete`)

Deletes a file or directory recursively. Under `sandboxed` mode, file deletion triggers an immediate Human-in-the-Loop approval gate.

* **Path:** `/v1/fs/delete`
* **Method:** `POST`

#### Request Payload
```json
{
  "path": "temp/obsolete.log",
  "caller": "cleaner_tool"
}
```

#### Response (`200 OK`)
```json
{
  "status": "success"
}
```

---

### 2.5 Rename / Move Path (`POST /v1/fs/rename`)

Atomically renames or moves a file or directory from `old_path` to `new_path`. Both source and destination paths are validated against the workspace boundary.

* **Path:** `/v1/fs/rename`
* **Method:** `POST`

#### Request Payload
```json
{
  "old_path": "src/old_name.rs",
  "new_path": "src/new_name.rs"
}
```

#### Response (`200 OK`)
```json
{
  "status": "success"
}
```

---

### 2.6 Copy Path (`POST /v1/fs/copy`)

Copies a file or entire directory structure recursively to a destination inside the workspace.

* **Path:** `/v1/fs/copy`
* **Method:** `POST`

#### Request Payload
```json
{
  "src_path": "assets/template.json",
  "dest_path": "config/settings.json"
}
```

#### Response (`200 OK`)
```json
{
  "status": "success"
}
```

---

### 2.7 Create Directory (`POST /v1/fs/mkdir`)

Creates a new directory including all necessary parent folders.

* **Path:** `/v1/fs/mkdir`
* **Method:** `POST`

#### Request Payload
```json
{
  "path": "nested/output/data"
}
```

#### Response (`200 OK`)
```json
{
  "status": "success"
}
```

---

### 2.8 Workspace Management (`GET/POST /v1/workspace`)

Retrieves or dynamically updates the active workspace directory at runtime.

#### Get Active Workspace
* **Method:** `GET`
* **Path:** `/v1/workspace`

```bash
curl -X GET http://localhost:8080/v1/workspace \
  -H "Authorization: Bearer sk-cluaiz-admin-key"
```

**Response (`200 OK`):**
```json
{
  "status": "success",
  "workspace": "C:\\Users\\Developer\\Projects\\MyWorkspace"
}
```

#### Set Active Workspace
* **Method:** `POST`
* **Path:** `/v1/workspace`

**Request Payload:**
```json
{
  "workspace": "C:\\Users\\Developer\\Projects\\AnotherWorkspace"
}
```

**Response (`200 OK`):**
```json
{
  "status": "success",
  "workspace": "\\\\?\\C:\\Users\\Developer\\Projects\\AnotherWorkspace"
}
```

---

## 3. Code Examples

### 3.1 Python (Using `requests`)

```python
import os
import requests

API_URL = "http://localhost:8080"
# Load local session token or configured API key
SESSION_TOKEN = os.getenv("CLUAIZ_API_KEY", "your-session-or-api-key")

headers = {
    "Authorization": f"Bearer {SESSION_TOKEN}",
    "Content-Type": "application/json"
}

# 1. Read File
res = requests.post(f"{API_URL}/v1/fs/read", headers=headers, json={"path": "Cargo.toml"})
print("Cargo.toml content:", res.json().get("content"))

# 2. Write File
write_payload = {
    "path": "notes/todo.txt",
    "content": "1. Run integration benchmarks\n2. Verify memory limits"
}
res_write = requests.post(f"{API_URL}/v1/fs/write", headers=headers, json=write_payload)
print("Write status:", res_write.json())
```

### 3.2 cURL

```bash
# List directory contents
curl -X POST http://localhost:8080/v1/fs/list \
  -H "Authorization: Bearer sk-cluaiz-xxxxxxxxxxxx" \
  -H "Content-Type: application/json" \
  -d '{"dir_path": "src", "recursive": false}'
```

---

## 4. Standard Error Codes

| Status Code | Error Response Message | Root Cause |
|---|---|---|
| `401 Unauthorized` | `"Missing or invalid Authorization token..."` | Request lacked `Authorization: Bearer <token>` or token failed constant-time validation. |
| `403 Forbidden` | `"Workspace Boundary Violation: Path lies outside active workspace"` | Attempted path traversal or access outside `workspace_root` under `strict` mode. |
| `403 Forbidden` | `"Permission Required: Operation requires user approval"` | Write or delete action paused awaiting Human-in-the-Loop approval under `sandboxed` mode. |
| `404 Not Found` | `"File not found: <path>"` | Target file does not exist on disk. |
| `500 Internal Error` | `"Write error: <os_error>"` | Disk I/O or OS filesystem permission failure. |
