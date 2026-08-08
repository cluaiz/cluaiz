# Security Policy

## Supported Versions

Currently, Cluaiz is in a Pre-Alpha Validation Phase. We only provide security updates for the `main` branch and the most recent pre-release tags. Once we reach a stable Alpha/Beta release, a strict version support matrix will be established.

| Version | Supported          |
| ------- | ------------------ |
| `main`  | :white_check_mark: |
| < 0.1.0 | :x:                |

## Reporting a Vulnerability

We take the security of the Cluaiz Engine very seriously. Since Cluaiz runs locally on user hardware and processes sensitive data locally, ensuring a secure boundary is our top priority.

If you discover a vulnerability, **please do not open a public issue.** Public disclosure could put our early adopters at risk.

Instead, please report it privately:
1. Email your findings to the core maintainer team (or the designated security email for Cluaiz).
2. Include clear steps to reproduce the vulnerability.
3. Provide details about the potential impact (e.g., local privilege escalation, arbitrary code execution, etc.).

We will acknowledge receipt of your vulnerability report within 48 hours and strive to send you regular updates about our progress.

## Our Security Commitment

* **Local Execution**: Cluaiz runs locally on user hardware. We do not transmit your prompts, documents, or data to external servers without explicit, user-configured network rules.
* **Dependency Auditing**: We regularly audit our Rust crates and C/C++ dependencies (like `llama.cpp` and ONNX Runtime) for known CVEs.
* **Sandboxed Execution**: Developer tools and plugins running on the local port are designed to operate within controlled boundaries, though users should always exercise caution when installing untrusted weights or plugins.