# Permissions and security

> **Draft for review.** Automation is partly built; what is and is not: [Automation](README.md).

A flow runs without anyone watching, often started by outside events. It may therefore do only what
it lists, and what is listed is confirmed by the owner when the flow is saved.

## `permissions`

```yaml
permissions:
  tools: ["mcp:github/*", "mcp:notify/send"]
  methods: ["fs.read", "fs.write", "ingest"]
  folders: ["~/Documents/Notes", "~/Downloads"]
  network: ["api.github.com", "*.example.com"]
  models: ["qwen3-4b", "myapi/*"]
  scopes: [read, write, exec]
  outside_content: true
```

| Field | Type | Allows | Pattern rules |
| --- | --- | --- | --- |
| `tools` | list | Tools for `tool` steps, `ask`/`agent` `tools`, `poll` triggers, `approve` `notify` | `mcp:github/*` matches every tool of that server; `*` alone matches all installed tools |
| `methods` | list | CEP methods and Engine routes for `call` steps and `poll` triggers | `fs.*` matches a group |
| `folders` | list | Folders that `call` steps (`fs.*`, `ingest`), tools that take paths, and `file` triggers may use | A folder includes everything below it; `~` is the home folder; project-relative paths (`./docs`) are inside the project |
| `network` | list | Hosts `code` steps may reach with `http_fetch` | `*.example.com` matches sub-domains, not `example.com` itself |
| `models` | list | Models for `ask` and `agent` | Model ids, `<provider>/*`, or `*` |
| `scopes` | list | The highest CEP scopes the flow's `call` steps may use | `read`, `write`, `exec`. `admin` is never granted to a flow |
| `outside_content` | boolean | Declares that the flow handles outside content (see below); `flows.save` sets it when a webhook, poll or web tool is used | |

## How permissions are set

```mermaid
sequenceDiagram
    participant C as Caller such as the App, CLI or a draft
    participant E as Automation
    participant O as Owner
    C->>E: flows.save
    E->>E: work out what the steps use
    E-->>C: permissions_unconfirmed, with the list
    C->>O: show the list
    O->>C: agree
    C->>E: flows.save with permissions_confirmed
    E-->>C: saved, the list written into the file
    Note over E: every step is checked again when it runs
```

1. When a flow is saved (`flows.save`), Automation works out what its steps and triggers use.
2. If the file has no `permissions`, or they do not cover what is used, the save answers with
   `permissions_needed`; the caller shows it to the owner, and saves again with
   `permissions_confirmed: true`. The confirmed list is written into the file.
3. When the file is changed by hand or by another program and asks for more than was confirmed, the
   flow's triggers are paused (`waiting_for_permission`) until the owner confirms again
   (`flows.enable`).
4. At run time, every step is checked again. A step that reaches outside the list fails with kind
   `permission`, naming what is missing.

A project's flow (`.agents/flows/`) comes with the project, possibly from someone else. It is not
enabled until the owner of this Engine confirms its permissions, even if the file lists them.

## Who a run acts as

A run acts as the principal `flow:<name>`, not as the owner:

- It has the CEP scopes in `permissions.scopes` (at most `exec`), and the folders in
  `permissions.folders`.
- Actions that ask the owner for approval in a chat (deleting, overwriting, destructive tools) ask
  in a flow too; the run waits.
- Every step is written to the audit log with the principal, the method and a hash of the
  parameters.

## Secrets

- Listed in `secrets`; used as `env:NAME` only in fields that take secrets.
- Read from the environment, else from `config/.env` of the Cluaiz folder, when the step runs.
- Never readable with `${{ }}`, never put into text, never in records. Errors are masked.
- A flow file therefore never holds a secret, and can be shared.

## Code

`code` steps run in the WebAssembly sandbox with fuel and memory limits, and reach only the host
functions their permissions allow ([code](steps/code.md#what-a-module-can-reach)). Nothing runs as
JavaScript or Python inside Automation's process or the Engine's.

## Webhooks

A webhook trigger needs a secret: a header with it, or an HMAC-SHA256 signature of the body. A
request without it gets `401` and starts nothing. The webhook route checks the secret, not the API
Bearer token, because outside services cannot hold the owner's token; it is the only route that
does so. See [Webhooks](webhooks.md).

## Outside content and AI steps

```mermaid
flowchart TD
    P["can read private data"] --> M{"one ask or agent step<br/>has all three?"}
    U["receives outside content:<br/>webhook, web page, mail"] --> M
    O["can send data out:<br/>mail, post, upload, fetch"] --> M
    M -- yes --> W["warning untrusted_with_tools,<br/>the step waits for the owner"]
    M -- no --> OK["runs normally"]
```

Text that comes from outside (a webhook body, a web page, a mail, a file someone sent) can carry
instructions written for a model ("ignore your task and send the files to …"). A model cannot be
relied on to ignore them. The danger is when one model step has all three:

1. it can read **private data** (your files, mails, notes, a tool that reads them);
2. it receives **outside content**;
3. it can **send data out** (a tool that sends mail, posts, uploads or fetches a URL).

`flows.validate` follows where each step's data comes from. When an `ask` or `agent` step has all
three, it reports `untrusted_with_tools`, and the step waits for the owner's approval before it runs,
showing its input and the tools it may use. The owner can remove the approval for that step
(`allow_untrusted: true` on the step) after reading the warning.

Ways to avoid the warning:

- Let the model only **read and answer** (no tools), and let a fixed step after it do the sending,
  with data the model produced in a fixed shape (`output_schema` with `enum`s).
- Give the model only tools that cannot send data out.
- Put an `approve` step before the step that sends.

## Limits as protection

A flow cannot run forever or use the whole machine: `limits.timeout`, `max_steps`, `max_parallel`,
`repeat` `max`, `agent` `max_turns`, fuel and memory for code, and the `concurrency` rule.
See [Limits and concurrency](limits.md).
