# Security & Authentication Architecture

> **Document Type:** System Architecture & Security Reference  
> **Status:** Active & Implemented  
> **Component:** `cluaiz_api`, `engines`, `engine_core`

---

## 1. Executive Summary & Threat Model

Cluaiz provides a high-performance local AI inference engine and agentic runtime. Because the engine executes tools, reads/writes files, and can run in both local desktop environments and cloud/server deployments, it enforces a defense-in-depth security model:

1. **Local Threat Model:** Protect developer laptops against browser drive-by attacks, malicious web scripts attempting cross-origin requests, and untrusted agent file escapes.
2. **Server/Cloud Threat Model:** Protect multi-tenant, remote, or LAN deployments with industry-standard Bearer Token authentication (`Authorization: Bearer sk-cluaiz-...`) without brittle hardcoded URL or origin restrictions.
3. **Reality Commitment:** The architecture is explicitly documented as **Workspace-Gated + Human-in-the-Loop (HITL) Approval**, eliminating hype terms like "OS Sandbox" in favor of concrete, verifiable controls.

```mermaid
flowchart TD
    Client["Client / Web UI / Agent"] -->|HTTP Request| Preflight{"OPTIONS Request?"}
    Preflight -->|Yes| CORS["CORS Layer (Allow Preflight)"]
    Preflight -->|No| AuthGate["Bearer Token Extraction"]

    AuthGate --> HasToken{"Valid Token Provided?"}
    
    HasToken -->|Yes| Allowed["Authenticated Request (Allow All Authorized APIs)"]
    
    HasToken -->|No| Sensitive{"Sensitive Route?\n(/v1/fs/* or /v1/system/cmd)"}
    Sensitive -->|Yes| Block401["401 Unauthorized\n(Token Mandatory)"]
    
    Sensitive -->|No| PublicRoute{"Public Endpoint?\n(/health, /info, Static UI)"}
    PublicRoute -->|Yes| PassPublic["Allow Public Response"]
    
    PublicRoute -->|No| GlobalAuth{"Require API Auth ON?"}
    GlobalAuth -->|Yes| BlockGlobal401["401 Unauthorized\n(API Auth Required)"]
    GlobalAuth -->|No| LocalDev["Allow Local Dev Request"]
```

---

## 2. Bearer Token Authentication Architecture

### 2.1 Dual-Token Authentication System

The engine accepts two types of cryptographic Bearer tokens via the standard `Authorization: Bearer <token>` HTTP header:

1. **User / Server API Keys (`sk-cluaiz-...`):**
   * Configured in `permission.json` under `api_auth.tokens`.
   * Generated and managed via the Developer Hub UI (Settings -> API Keys).
   * Used by external scripts, remote web apps, SDKs, and cURL clients.
2. **Local Session Token (`session.token`):**
   * Generated dynamically on engine boot (`auth::initialize_session_token`).
   * Stored in `~/.cluaiz/session.token` with restricted POSIX permissions (`0600`).
   * Injected automatically by local desktop wrappers (Tauri / Electron) for zero-configuration local security.

### 2.2 Constant-Time Comparison

To prevent timing side-channel attacks where an attacker could deduce valid token prefixes by measuring microsecond response differentials, all token checks use `constant_time_compare`:

```rust
pub fn constant_time_compare(a: &str, b: &str) -> bool {
    let a_bytes = a.as_bytes();
    let b_bytes = b.as_bytes();
    if a_bytes.len() != b_bytes.len() {
        return false;
    }
    let mut diff = 0u8;
    for (x, y) in a_bytes.iter().zip(b_bytes.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}
```

### 2.3 Route Enforcement Matrix

| Route Category | Example Endpoints | When Auth Toggle is ON | When Auth Toggle is OFF (Local Dev) |
|---|---|---|---|
| **Public Probes** | `GET /health`, `GET /info` | Public (200 OK) | Public (200 OK) |
| **DevHub Static Assets** | `/`, `/index.html`, `/assets/*` | Public (200 OK) | Public (200 OK) |
| **General Inference** | `POST /v1/chat/completions`, `GET /v1/models` | **Bearer Token Required (401)** | Open for Local Prototyping (200 OK) |
| **Sensitive Filesystem** | `POST /v1/fs/read`, `/v1/fs/write`, `/v1/fs/delete` | **Bearer Token Required (401)** | **Bearer Token Required (401)** |
| **Terminal Shell Exec** | `POST /v1/system/cmd` | **Bearer Token Required (401)** | **Bearer Token Required (401)** |

> **Critical Rule:** Sensitive routes (`/v1/fs/*` and `/v1/system/cmd`) **NEVER** run without a token, regardless of the global API Authentication toggle.

---

## 3. Workspace Canonical Jail & Ancestor Traversal

Located in [`inference-engine/api/src/handlers/fs_jail.rs`](file:///c:/Users/Aryan/my/Cluaiz-workspace/Cluaiz-Technologies/cluaiz/inference-engine/api/src/handlers/fs_jail.rs).

### 3.1 Path Resolution Rules

1. **Relative Path Rooting:** Relative paths (e.g. `src/main.rs`) are always joined to `workspace_root`, never to the process working directory (CWD).
2. **Dual Canonicalization:** Both the candidate path and the workspace root are canonicalized via `std::fs::canonicalize` before comparison, ensuring Windows Extended-Length Prefixes (`\\?\`) match identically.
3. **Ancestor Walking for New Writes:** When an agent writes to a non-existent file, the resolver walks up parent directories until it finds the closest existing ancestor, canonicalizes it, and appends the uncreated child path segments.

### 3.2 Security Modes

* **Strict Mode (`strict`):** Any attempt to access a path outside the active workspace returns a hard `403 Forbidden` (`"Workspace Boundary Violation"`). No approval bypass is possible.
* **Workspace-Gated (`sandboxed`):** Safe workspace reads execute automatically. Writes, creations, and access outside the workspace register a `PendingPermissionRequest` and halt execution until explicit human approval is received.
* **Full Access (`full_access`):** Autonomous developer mode bypassing path checks.

---

## 4. Capability-Based Tool Approval & HITL Streaming

Located in [`inference-engine/engines/src/tools/registry/types.rs`](file:///c:/Users/Aryan/my/Cluaiz-workspace/Cluaiz-Technologies/cluaiz/inference-engine/engines/src/tools/registry/types.rs) and [`inference-engine/api/src/handlers/chat/agent_loop.rs`](file:///c:/Users/Aryan/my/Cluaiz-workspace/Cluaiz-Technologies/cluaiz/inference-engine/api/src/handlers/chat/agent_loop.rs).

### 4.1 Capability Declarations

Tools declare capabilities explicitly in their metadata:
* `exec`: Running system binaries, CLI commands, or scripts.
* `fs_write`: Mutating, writing, or deleting files on disk.
* `network`: Inbound or outbound network socket operations.

**Default-Deny Rule:** Any tool with undeclared capabilities (`capabilities.is_empty()`) requires explicit user approval by default under `sandboxed` and `strict` modes. All MCP (Model Context Protocol) external tools require approval under `sandboxed` and `strict` modes.

### 4.2 Transparent Payloads & Zero Post-Approval Mutation

1. Before tool execution, the engine yields an SSE chunk `permission_request` containing:
   * `request_id`: Unique UUID.
   * `tool_name`: Exact tool name.
   * `capabilities`: Declared capability list.
   * `parameters`: Fully expanded, untruncated JSON payload.
2. The agent loop pauses and awaits a signal on an in-memory `tokio::sync::oneshot` channel.
3. When the user approves via `POST /v1/system/permission/approve`, the engine resumes and executes the **exact stored parameters** received during the pause. The model cannot alter the payload between approval and execution.

---

## 5. Safe Configuration Deserialization

Located in [`inference-engine/engines/core/src/environment/config_manager.rs`](file:///c:/Users/Aryan/my/Cluaiz-workspace/Cluaiz-Technologies/cluaiz/inference-engine/engines/core/src/environment/config_manager.rs).

Configuration schemas (`permission.json`, `gguf_config.json`, `system_control.json`) prioritize human-editable JSON with `serde` defaults as the primary source of truth:
* Prevents binary layout mismatches (`rkyv` offset desync) caused by struct field additions across versions.
* Eliminates out-of-memory aborts caused by unvalidated binary zero-copy deserialization.

---

## 6. Master Security Matrix & Execution Boundary (Anthropic Parity)

Located in [`inference-engine/engines/src/tools/execution/policy.rs`](file:///c:/Users/Aryan/my/Cluaiz-workspace/Cluaiz-Technologies/cluaiz/inference-engine/engines/src/tools/execution/policy.rs) and [`inference-engine/engines/src/tools/registry/types.rs`](file:///c:/Users/Aryan/my/Cluaiz-workspace/Cluaiz-Technologies/cluaiz/inference-engine/engines/src/tools/registry/types.rs).

Cluaiz enforces a two-tier security governance model combining **Global Agent Mode** (`permission.json`) with **Tool-Level Policies** (`tools_registry.json`).

### 6.1 Architectural Decision Flow

```mermaid
flowchart TD
    Req["Tool / Terminal Request"] --> CheckSuicide{"Destructive Prohibited?\n(format c:, rm -rf /, dd if=)"}
    CheckSuicide -->|Yes| HardDeny["⛔ HARD-DENIED Across All Modes\n(OS / Hardware Destruction Prohibited)"]
    
    CheckSuicide -->|No| CheckMaster{"Master Security Mode\n(permission.json)"}
    
    CheckMaster -->|full_access| ToolFullCheck{"Tool Mode == require_approval?"}
    ToolFullCheck -->|Yes| PromptUser["🟡 Prompt User (HITL Confirmation Card)"]
    ToolFullCheck -->|No| AutoRun["✅ Direct Auto-Run (Zero Friction Automation)"]
    
    CheckMaster -->|strict| ToolStrictCheck{"Tool Mode == always_allow?"}
    ToolStrictCheck -->|Yes| AutoRun
    ToolStrictCheck -->|No| PromptUser
    
    CheckMaster -->|sandboxed| CheckJail{"Target Path in Workspace Jail?"}
    CheckJail -->|Outside Workspace| OutsideCheck{"Tool Mode == always_allow?"}
    OutsideCheck -->|Yes| PromptOutside["🟡 Prompt User (Outside Boundary Alert)"]
    OutsideCheck -->|No| PromptOutside
    
    CheckJail -->|Inside Workspace| ToolSandboxCheck{"Tool Policy"}
    ToolSandboxCheck -->|always_allow| AutoRun
    ToolSandboxCheck -->|require_approval| PromptUser
    ToolSandboxCheck -->|inherit| CapCheck{"Safe Read vs Mutative/Exec?"}
    CapCheck -->|Safe Read (cat, git diff)| AutoRun
    CapCheck -->|Mutative / Exec / Shell / MCP| PromptUser
```

### 6.2 The Complete 3 × 3 = 9 Master Security Matrix

| # | Global Mode (`permission.json`) | Tool Mode (`tools_registry.json`) | Workspace Ke Andar (Read / Edit / Build) | Workspace Ke Bahar (Host / System Path) | Harmful / Destructive Action (`rm -rf`, `format`, delete) |
| :---: | :--- | :--- | :--- | :--- | :--- |
| **1** | **`full_access`** | **`always_allow`** | ✅ **Direct Auto-Run** (Zero Prompt, Maximum Speed) | ✅ **Allowed Directly** (Read/Write Allowed for automation) | ⛔ **HARD-BLOCKED** (Suicide/OS wipe strictly denied; safe delete prompts) |
| **2** | **`full_access`** | **`inherit`** | ✅ **Direct Auto-Run** (Follows Global Full Access) | ✅ **Allowed Directly** (Automation loop smooth) | ⛔ **HARD-BLOCKED** (Hardware/OS destruction prohibited) |
| **3** | **`full_access`** | **`require_approval`** | 🟡 **Prompt User** (Tool level par user ne explicitly approval maanga hai) | 🟡 **Prompt User** (Confirmation required) | ⛔ **HARD-BLOCKED** (Destructive patterns denied; normal delete prompts) |
| **4** | **`sandboxed`** *(Default)* | **`always_allow`** | ✅ **Direct Auto-Run** (Sandbox ke andar tool ko full trust hai) | 🟡 **Prompt User** (Workspace se bahar ja raha hai, confirmation compulsory) | 🟡 **Prompt User / Block** (Harmful action par prompt aayega, suicide block) |
| **5** | **`sandboxed`** *(Default)* | **`inherit`** | ✅ **Read-only Auto-Run**<br>🟡 **Mutating/Exec prompts if undeclared** | 🟡 **Prompt User** (Outside boundary requires explicit sign-off) | ⛔ **HARD-BLOCKED** for suicide; 🟡 **Prompt User** for recursive delete |
| **6** | **`sandboxed`** *(Default)* | **`require_approval`** | 🟡 **Prompt User** (Har execution par approval card aayega) | 🟡 **Prompt User** (Bahar jaane par approval zaroori) | ⛔ **HARD-BLOCKED** for suicide; 🟡 **Prompt User** for delete |
| **7** | **`strict`** | **`always_allow`** | ✅ **Direct Auto-Run** (Tool trusted by user for workspace tasks) | 🟡 **Prompt User** (Workspace ke bahar 1% bhi sensitive hone par permission) | ⛔ **HARD-BLOCKED** for suicide; 🟡 **Prompt User** for delete |
| **8** | **`strict`** | **`inherit`** | 🟡 **Prompt User** (Strict mode forces confirmation on all exec/write) | 🚫 **Access Denied / Prompt** (Strict outside boundary lock) | ⛔ **HARD-BLOCKED** (Zero-tolerance destructive block) |
| **9** | **`strict`** | **`require_approval`** | 🟡 **Prompt User** (Double-lock: Global bhi Strict, Tool bhi Strict) | 🚫 **Access Denied / Prompt** (Outside workspace strictly gated) | ⛔ **HARD-BLOCKED** (Destructive commands completely rejected) |

### 6.3 Core Security Doctrines

1. **The Workspace Privilege Boundary:** The workspace root is the default trust zone. Crossing out of the workspace requires human verification in `sandboxed` and `strict` modes.
2. **Anthropic Safety Net (Non-Bypassable Hard-Deny):** Even when running under `--dangerously-skip-permissions` or `full_access`, catastrophic commands that wipe disks or destroy operating system files (`format c:`, `rm -rf /`, `diskutil eraseDisk`, fork bombs) are unconditionally rejected at the engine level across Windows, Linux, and macOS.
3. **Sensitive Credential Protection:** Sensitive identity paths (`~/.ssh`, `~/.aws/credentials`, `~/.kube/config`, `/etc/shadow`, `system32/config`, macOS Keychains) are actively protected from unapproved reading or modification.

### 6.4 Cross-Platform Hard-Deny Catalog

The following operations are prohibited unconditionally across all 3 major platforms:

* **POSIX / Linux / macOS**:
  * Root & Home Directory Destruction: `rm -rf /`, `rm -rf /*`, `rm -rf ~`, `rm -rf $HOME`
  * Raw Disk Wiping & Partition Formatting: `mkfs`, `mkfs.ext4`, `mkfs.xfs`, `mkfs.btrfs`, `mkfs.vfat`
  * Raw Device Zeroing & Overwrites: `dd if=/dev/zero`, `dd if=/dev/urandom`, `dd if=/dev/null`, `dd of=/dev/sd*`, `dd of=/dev/nvme*`, `> /dev/sda`
  * Process Starvation & Fork Bombs: `:(){ :|:& };:`, `:(){ :|: & };:`
  * Forced Kernel Shutdown & Halts: `shutdown -h`, `shutdown -r`, `init 0`, `init 6`, `poweroff`, `reboot`, `halt`
  * System Permission Tampering: `chmod -R 777 /`, `chmod -R 000 /`, `chown -R` on `/`
* **Windows**:
  * Volume & Disk Formatting: `format c:`, `format d:`, `format /q`
  * System Drive Wiping: `rmdir /s /q c:\`, `rmdir /s /q c:/`, `rd /s /q c:\`, `rd /s /q c:/`
  * Windows Directory Deletion: `del /f /s /q c:\windows`, `del /f /s /q c:/windows`, `del /f /s /q c:\*`
  * Scripted Storage Destruction: `diskpart`, `Clear-Disk`, `Initialize-Disk`, `Remove-Partition`, `Format-Volume`
  * Host Shutdown & Reboot: `Stop-Computer`, `Restart-Computer`, `shutdown /s`, `shutdown /r`
* **macOS**:
  * Volume & Partition Destruction: `diskutil eraseDisk`, `diskutil reformat`, `diskutil unmountDisk force`
  * Firmware & NVRAM Wiping: `nvram -c`
  * System Integrity Protection Tampering: `csrutil disable`

### 6.5 3-Stage Tool Execution Lifecycle

Tool invocations in Cluaiz stream through 3 formal stages:

```mermaid
sequenceDiagram
    participant LLM as Agent Loop (LLM)
    participant Engine as Cluaiz Engine
    participant OS as Subprocess (Host OS)
    participant UI as Desktop Client / Web UI

    LLM->>Engine: Tool Call Emitted (e.g. run_command)
    Note over Engine: Stage 1: PENDING
    alt Approval Required
        Engine->>UI: SSE permission_request (status: pending_approval)
        UI-->>Engine: User Decision (Approve / Deny)
    else Auto-Approved / Safe
        Engine->>UI: SSE tool_status (status: pending)
    end

    Note over Engine,OS: Stage 2: RUNNING
    Engine->>UI: SSE tool_status (status: running)
    Engine->>OS: Spawn Process with Sandbox Boundary

    Note over OS,Engine: Stage 3: COMPLETED / ERROR
    OS-->>Engine: Process Exit (Stdout / Stderr / Exit Code)
    Engine->>UI: SSE tool_result (status: completed | failed, latency_ms)
    Engine->>LLM: Resume with XML <tool_response>
```

