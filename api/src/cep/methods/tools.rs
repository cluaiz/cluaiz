//! `tools.*`: skills, plugins and MCP servers, each named by `kind` + `name`.

mod catalog;
mod common;
mod files;
mod lifecycle;

use crate::cep::CallCtx;
use cluaiz_cep::{Meta, Registry, Scope};
use serde_json::json;

pub fn register(registry: &mut Registry<CallCtx>) {
    registry.add(
        "tools.list",
        Meta::new(
            Scope::Read,
            "Installed tools with settings and known function schemas",
        )
        .example(json!({"kind": "mcp"}), json!([{
            "id": "filesystem", "category": "mcp", "description": "Official Model Context Protocol secure file system bridge",
            "enabled": true, "execution_mode": "manual", "security_mode": "inherit", "default_turns": 3,
            "functions": [{"name": "read_text_file", "description": "Read the complete contents of a file from the file system as text.",
                           "inputSchema": {"type": "object", "properties": {"path": {"type": "string"}}, "required": ["path"]},
                           "annotations": {"readOnlyHint": true, "openWorldHint": false}}]
        }])),
        catalog::list,
    );
    registry.add(
        "tools.discover",
        Meta::new(Scope::Read, "Dry-run the existing installed-tool matcher").example(
            json!({"query":"read notes.txt from my project"}),
            json!({"query":"read notes.txt from my project","actionable":true,"installed":10,
                   "candidates":[{"id":"filesystem","category":"mcp","level":"card","score":9.56,"matched":["read","project","file"],
                                  "load_cost_tokens":1650,"summary":"Official Model Context Protocol secure file system bridge"},
                                 {"id":"pynotes","category":"mcp","level":"title","score":3.58,"matched":["read"],
                                  "load_cost_tokens":55,"summary":"Test notes server"}]}),
        ),
        catalog::discover,
    );
    registry.add(
        "tools.install",
        Meta::new(
            Scope::Write,
            "Install a registry tool; task result reports the actual outcome",
        )
        .task()
        .error(-32602, "invalid_params", "`name` is not 1–64 lowercase letters, digits, dots, underscores or hyphens")
        .error(-32002, "forbidden", "The tool folder is outside the caller's roots (`data.needs: \"roots\"`)")
        .error(-32010, "tool_operation_failed", "The installer, registry or tool itself failed (its message is passed on)")
        .error(-32008, "cancelled", "`tasks.cancel` stopped it before or after the download (from the task's result)")
        .example(
            json!({"kind":"skill","name":"registry-tool"}),
            json!({"task":{"task_id":"t_example","status":"working"}}),
        ),
        lifecycle::install,
    );
    registry.add(
        "tools.remove",
        Meta::new(
            Scope::Write,
            "Remove an installed tool and update its registry",
        )
        .destructive()
        .error(-32004, "tool_not_found", "No installed tool of this kind and name")
        .error(-32602, "invalid_params", "`name` is not 1–64 lowercase letters, digits, dots, underscores or hyphens")
        .error(-32002, "forbidden", "The tool folder is outside the caller's roots (`data.needs: \"roots\"`)")
        .error(-32010, "tool_operation_failed", "The installer, registry or tool itself failed (its message is passed on)")
        .example(
            json!({"kind":"skill","name":"installed-tool"}),
            json!({"kind":"skill","name":"installed-tool"}),
        ),
        lifecycle::remove,
    );
    registry.add(
        "tools.cache",
        Meta::new(
            Scope::Read,
            "Actual cache entries and byte counts for installed tools",
        )
        .error(-32004, "tool_not_found", "No installed tool of this kind and name")
        .error(-32602, "invalid_params", "`name` is not 1–64 lowercase letters, digits, dots, underscores or hyphens")
        .error(-32002, "forbidden", "The tool folder is outside the caller's roots (`data.needs: \"roots\"`)")
        .example(json!({"kind":"skill"}), json!([{"kind":"skill","name":"code-interpreter","all_bytes":0,"temp_bytes":0,"entries":[]}])),
        lifecycle::cache,
    );
    registry.add(
        "tools.clear_cache",
        Meta::new(
            Scope::Write,
            "Remove cached files without uninstalling tools",
        )
        .destructive()
        .error(-32004, "tool_not_found", "No installed tool of this kind and name")
        .error(-32602, "invalid_params", "`name` is not 1–64 lowercase letters, digits, dots, underscores or hyphens")
        .error(-32002, "forbidden", "The tool folder is outside the caller's roots (`data.needs: \"roots\"`)")
        .error(-32010, "filesystem_error", "A cached file could not be removed")
        .example(
            json!({"kind":"skill"}),
            json!({"removed":0,"freed_bytes":0}),
        ),
        lifecycle::clear_cache,
    );
    registry.add(
        "tools.get_settings",
        Meta::new(Scope::Read, "Read installed tool settings")
            .no_mcp()
            .error(-32004, "tool_not_found", "No installed tool of this kind and name")
            .error(-32602, "invalid_params", "`name` is not 1–64 lowercase letters, digits, dots, underscores or hyphens")
            .example(
                json!({"kind":"mcp","name":"filesystem"}),
                json!({"schema":{},"values":{"enabled":true,"execution_mode":"manual","security_mode":"inherit","trust_annotations":false}}),
            ),
        catalog::get,
    );
    registry.add(
        "tools.set_settings",
        Meta::new(Scope::Write, "Validate and save a tool settings patch")
            .no_mcp()
            .error(-32004, "tool_not_found", "No installed tool of this kind and name")
            .error(-32602, "invalid_params", "An unknown `security_mode` / `execution_mode`, or a bad `name`")
            .error(-32010, "tool_operation_failed", "The tool registry could not be saved")
            .example(
                json!({"kind":"skill","name":"installed-tool","patch":{"enabled":true}}),
                json!({"schema":{},"values":{"enabled":true}}),
            ),
        catalog::set,
    );
    registry.add(
        "tools.files",
        Meta::new(
            Scope::Read,
            "Files inside a tool package, with cache counts",
        )
        .error(-32004, "tool_not_found", "No installed tool of this kind and name")
        .error(-32602, "invalid_params", "`name` is not 1–64 lowercase letters, digits, dots, underscores or hyphens")
        .error(-32002, "forbidden", "The tool folder is outside the caller's roots (`data.needs: \"roots\"`)")
        .example(
            json!({"kind":"skill","name":"installed-tool"}),
            json!({"files":[{"path":"SKILL.md","name":"SKILL.md","is_dir":false},{"path":"assets","name":"assets","is_dir":true}],
                   "cache":{"all_bytes":0,"temp_bytes":0,"entries":[]},"truncated":false,"skipped":0}),
        ),
        files::files,
    );
    registry.add(
        "tools.read_file",
        Meta::new(
            Scope::Read,
            "Read a UTF-8 file inside an installed tool package",
        )
        .error(-32004, "tool_not_found", "No installed tool of this kind and name")
        .error(-32602, "invalid_params", "`path` leaves the tool folder")
        .error(-32004, "file_not_found", "No such file in the tool folder")
        .example(
            json!({"kind":"skill","name":"installed-tool","path":"SKILL.md"}),
            json!({"path":"SKILL.md","text":"Tool instructions"}),
        ),
        files::read,
    );
    registry.add(
        "tools.write_file",
        Meta::new(
            Scope::Write,
            "Write SKILL.md or package.json in an installed tool package",
        )
        .no_mcp()
        .error(-32004, "tool_not_found", "No installed tool of this kind and name")
        .error(-32602, "invalid_params", "`name` is not 1–64 lowercase letters, digits, dots, underscores or hyphens")
        .error(-32002, "forbidden", "The tool folder is outside the caller's roots (`data.needs: \"roots\"`)")
        .example(
            json!({"kind":"skill","name":"installed-tool","text":"Updated instructions"}),
            json!({"path":"SKILL.md","bytes":20}),
        ),
        files::write,
    );
    registry.add(
        "tools.open_file",
        Meta::new(
            Scope::Read,
            "Ask the local file editor to open a tool file (owner only)",
        )
        .owner_only()
        .error(-32004, "tool_not_found", "No installed tool of this kind and name")
        .error(-32602, "invalid_params", "`path` leaves the tool folder or is not a file")
        .example(
            json!({"kind":"skill","name":"installed-tool"}),
            json!({"opened":true}),
        ),
        files::open,
    );
    registry.add(
        "tools.call",
        Meta::new(
            Scope::Exec,
            "Execute an installed tool using the existing ToolsEngine",
        )
        .error(-32004, "tool_not_found", "No installed tool of this kind and name")
        .error(-32602, "invalid_params", "A bad `name`, or `fn` looks like a path")
        .error(-32002, "forbidden", "The tool folder is outside the caller's roots (`data.needs: \"roots\"`)")
        .error(-32010, "tool_operation_failed", "The installer, registry or tool itself failed (its message is passed on)")
        .example(
            json!({"kind":"plugin","name":"installed-tool","fn":"run","input":{}}),
            json!({"result":"tool output"}),
        ),
        lifecycle::execute,
    );
}
