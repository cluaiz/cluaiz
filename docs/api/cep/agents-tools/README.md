# Agents, MCP & Tools

<!-- Generated from the CEP registry by api/tests/cep_docs.rs; do not edit. -->

| Method | Scope | Runs as | Summary |
|---|---|---|---|
| [`approvals.approve`](./approvals/approvals.approve.md) | admin | sync | Allows the waiting call; with remember chat, the tool also runs without asking in that chat for a while |
| [`approvals.list`](./approvals/approvals.list.md) | admin | sync | Approval requests waiting for the owner |
| [`approvals.reject`](./approvals/approvals.reject.md) | admin | sync | Refuses the waiting call |
| [`tools.cache`](./tools/tools.cache.md) | read | sync | Actual cache entries and byte counts for installed tools |
| [`tools.call`](./tools/tools.call.md) | exec | sync | Execute an installed tool using the existing ToolsEngine |
| [`tools.clear_cache`](./tools/tools.clear_cache.md) | write | sync | Remove cached files without uninstalling tools |
| [`tools.discover`](./tools/tools.discover.md) | read | sync | Dry-run the existing installed-tool matcher |
| [`tools.files`](./tools/tools.files.md) | read | sync | Files inside a tool package, with cache counts |
| [`tools.get_settings`](./tools/tools.get_settings.md) | read | sync | Read installed tool settings |
| [`tools.install`](./tools/tools.install.md) | write | task | Install a registry tool; task result reports the actual outcome |
| [`tools.list`](./tools/tools.list.md) | read | sync | Installed tools with settings and known function schemas |
| [`tools.open_file`](./tools/tools.open_file.md) | read | sync | Ask the local file editor to open a tool file (owner only) |
| [`tools.read_file`](./tools/tools.read_file.md) | read | sync | Read a UTF-8 file inside an installed tool package |
| [`tools.remove`](./tools/tools.remove.md) | write | sync | Remove an installed tool and update its registry |
| [`tools.set_settings`](./tools/tools.set_settings.md) | write | sync | Validate and save a tool settings patch |
| [`tools.write_file`](./tools/tools.write_file.md) | write | sync | Write SKILL.md or package.json in an installed tool package |

All categories: [CEP reference](../README.md). Folder: `docs/api/cep/agents-tools/`.
