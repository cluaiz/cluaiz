# Security Policy

## Supported Versions

Cluaiz is in pre-release. Security fixes go to the `main` branch and into the next release.

| Version | Supported          |
| ------- | ------------------ |
| `main`  | :white_check_mark: |
| < 0.1.0 | :x:                |

---

## How the Engine protects you

What the Engine does today. The details are in [Security & Authentication Architecture](docs/architecture/security-and-auth-architecture.md).

### 1. Dual-Token Bearer Authentication
All incoming requests are evaluated through our standard HTTP Bearer token middleware (`auth::auth_middleware`):
* **Configured API keys (`sk-cluaiz-...`):** saved in `permission.json` and managed with `cluaiz token create`, `list` and `remove`.
* **Session token:** created when the Engine starts, in `~/.cluaiz/session.token`, readable only by your user (`0600` on Linux and macOS), so local apps can connect without setup.
* **Constant-time comparison:** tokens are compared with `constant_time_compare()`, so the time a check takes does not reveal how much of a token was right.
* **One rule for every route:** only `/health` is open; every other route, CEP (`/v1/cep`, `/mcp`) included, follows the owner's `api_auth.required` setting. What a caller may then do is CEP's scope and path check, and `system.exec` asks the owner on every call.

### 2. Workspace Canonical Jail & Ancestor Walking
Filesystem interactions are strictly bound to the active workspace directory:
* **Canonical paths:** every path is resolved with `std::fs::canonicalize` before it is checked, so symlinks, `../` sequences and Windows extended-length prefixes (`\\?\`) are resolved first.
* **Closest existing parent:** before a new file is created, the closest folder that already exists is checked, so a new path outside the workspace is refused.
* `fs.delete` and `fs.rename` act on a link itself, never on its target. See [Security & Auth Architecture](docs/architecture/security-and-auth-architecture.md) and the [CEP reference](docs/api/cep/README.md).

### 3. Capability-Based Human-in-the-Loop (HITL) Gate
Tools declare granular capabilities (`exec`, `fs_write`, `network`). Under `sandboxed` and `strict` modes:
* Any tool invocation declaring sensitive capabilities or undeclared MCP operations automatically pauses the token generation stream.
* A transparent `permission_request` SSE chunk is delivered to the client with the exact, unmutated parameters.
* Execution resumes only when the owner answers with CEP `approvals.approve` (or `approvals.reject`).

---

## Reporting a Vulnerability

If you find a vulnerability or a way around a protection, **please do not open a public issue.** A public report puts everyone running the Engine at risk.

Instead, report it privately:
1. Use GitHub's **Report a vulnerability** button on the [Security tab](https://github.com/cluaiz/cluaiz/security).
2. Include clear, step-by-step instructions to reproduce the vulnerability.
3. Detail the impact (e.g., directory jail escape, unauthorized command execution, or timing side-channel).

Every report is read. During the pre-release there is no fixed response time; when a report is confirmed, the fix and the time it is made public are agreed with you.