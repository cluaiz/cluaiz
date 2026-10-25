//! `tools.*: skills, plugins and MCP servers (kind + name)` (03-method-catalog Part B). Owner: Wave B (05-chatgpt-plan.md §2).

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
        .example(json!({}), json!([])),
        catalog::list,
    );
    registry.add(
        "tools.discover",
        Meta::new(Scope::Read, "Dry-run the existing installed-tool matcher").example(
            json!({"query":"Find a tool for this task"}),
            json!({"candidates":[],"installed":0}),
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
        .example(json!({"kind":"skill"}), json!([])),
        lifecycle::cache,
    );
    registry.add(
        "tools.clear_cache",
        Meta::new(
            Scope::Write,
            "Remove cached files without uninstalling tools",
        )
        .destructive()
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
            .example(
                json!({"kind":"skill","name":"installed-tool"}),
                json!({"schema":{},"values":{"enabled":true}}),
            ),
        catalog::get,
    );
    registry.add(
        "tools.set_settings",
        Meta::new(Scope::Write, "Validate and save a tool settings patch")
            .no_mcp()
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
        .example(
            json!({"kind":"skill","name":"installed-tool"}),
            json!({"files":[],"cache":{},"truncated":false,"skipped":0}),
        ),
        files::files,
    );
    registry.add(
        "tools.read_file",
        Meta::new(
            Scope::Read,
            "Read a UTF-8 file inside an installed tool package",
        )
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
        .example(
            json!({"kind":"plugin","name":"installed-tool","fn":"run","input":{}}),
            json!({"result":"tool output"}),
        ),
        lifecycle::execute,
    );
}
