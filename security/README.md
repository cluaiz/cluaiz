# Permission settings and audit log (`security/`)

This crate holds data, not enforcement. The checks run where a call is made: CEP scopes and approvals in
[`cep/`](../cep/), the bearer token in [`api/src/auth.rs`](../api/src/auth.rs), the agent's rules in
[`cmd/src/agent/rules.rs`](../cmd/src/agent/rules.rs).

| Module | Responsibility |
|---|---|
| [`permission_schema.rs`](src/permission_schema.rs) | `permission.json`: its fields, defaults, reading and saving |
| [`audit.rs`](src/audit.rs) | The log of security-relevant calls |

The settings are described in [docs/engine/permission.md](../docs/engine/permission.md).
