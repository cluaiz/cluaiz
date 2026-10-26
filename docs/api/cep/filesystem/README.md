# Filesystem & Security Jail

<!-- Generated from the CEP registry by api/tests/cep_docs.rs; do not edit. -->

| Method | Scope | Runs as | Summary |
|---|---|---|---|
| [`fs.copy`](./fs/fs.copy.md) | write | sync | Copies a file or directory between authorized paths |
| [`fs.delete`](./fs/fs.delete.md) | write | sync | Deletes a file or directory; a link is removed itself, never its target |
| [`fs.list`](./fs/fs.list.md) | read | sync | Lists directory entries, optionally recursively |
| [`fs.mkdir`](./fs/fs.mkdir.md) | write | sync | Creates a directory and missing parents |
| [`fs.pick_folder`](./fs/fs.pick_folder.md) | read | sync | Opens the host folder picker and selects the workspace |
| [`fs.read`](./fs/fs.read.md) | read | sync | Reads a UTF-8 file, or returns base64 for binary data |
| [`fs.rename`](./fs/fs.rename.md) | write | sync | Moves a file or directory between authorized paths; a link is moved itself |
| [`fs.write`](./fs/fs.write.md) | write | sync | Writes exactly one of text or data_base64 to a file |
| [`workspace.get`](./workspace/workspace.get.md) | read | sync | Returns the current workspace directory |
| [`workspace.set`](./workspace/workspace.set.md) | write | sync | Changes the workspace to an existing directory |

All categories: [CEP reference](../README.md). Folder: `docs/api/cep/filesystem/`.
