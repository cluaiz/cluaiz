# Security Policy

## Supported Versions

Currently, Cluaiz is in an Active Validation Phase. We provide security updates for the `main` branch and the most recent pre-release tags.

| Version | Supported          |
| ------- | ------------------ |
| `main`  | :white_check_mark: |
| < 0.1.0 | :x:                |

---

## Security Architecture & Core Commitments

Cluaiz enforces defense-in-depth principles across local, LAN, and remote cloud deployments. For an exhaustive breakdown of threat models and cryptographic protocols, refer to our [Security & Authentication Architecture](docs/architecture/security-and-auth-architecture.md).

### 1. Dual-Token Bearer Authentication
All incoming requests are evaluated through our standard HTTP Bearer token middleware (`auth::auth_middleware`):
* **Configured API Keys (`sk-cluaiz-...`):** Saved securely in `permission.json` and configurable via the Developer Hub UI.
* **Boot Session Token:** Generated dynamically on engine startup into `~/.cluaiz/session.token` with restricted (`0600`) POSIX permissions for zero-configuration local desktop security.
* **Constant-Time Validation:** All token evaluations use `constant_time_compare()` to eliminate microsecond side-channel timing attacks.
* **Sensitive Route Protection:** Operations against the host filesystem (`/v1/fs/*`) and operating system shell (`/v1/system/cmd`) **permanently enforce valid Bearer authentication**, even when general inference endpoints are opened for local dev prototyping.

### 2. Workspace Canonical Jail & Ancestor Walking
Filesystem interactions are strictly bound to the active workspace directory:
* **Canonical Path Normalization:** Every path is resolved and validated using `std::fs::canonicalize` to eliminate symlink attacks, directory traversal sequences (`../../`), and Windows extended-length prefix variations (`\\?\`).
* **Ancestor Walking:** New write operations verify the closest existing parent directory before creating files, ensuring agents cannot escape outside the workspace perimeter.
* For complete endpoint documentation, see the [Filesystem API Reference](docs/api/fs/filesystem.md).

### 3. Capability-Based Human-in-the-Loop (HITL) Gate
Tools declare granular capabilities (`exec`, `fs_write`, `network`). Under `sandboxed` and `strict` modes:
* Any tool invocation declaring sensitive capabilities or undeclared MCP operations automatically pauses the token generation stream.
* A transparent `permission_request` SSE chunk is delivered to the client with the exact, unmutated parameters.
* Execution resumes only upon receiving an explicit approval signal via `POST /v1/system/permission/approve`.

---

## Reporting a Vulnerability

We take the security of the Cluaiz Engine very seriously. If you discover a vulnerability or security bypass, **please do not open a public issue.** Public disclosure puts early adopters and production instances at risk.

Instead, report it privately:
1. Email your findings to the core maintainer team at `security@cluaiz.com` (or submit a private GitHub Security Advisory).
2. Include clear, step-by-step instructions to reproduce the vulnerability.
3. Detail the impact (e.g., directory jail escape, unauthorized command execution, or timing side-channel).

We will acknowledge receipt within 48 hours and work with you on a coordinated fix and disclosure timeline.