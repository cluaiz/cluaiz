//! `workspace.*`: the Engine's workspace folder, and the agent files of a project in it
//! (`cluaiz_agents::workspace`): its AGENTS.md instructions, the rules, workflows and skills of
//! its `.agents/` folders, and its memory (`.agents/memory/`).

use crate::cep::{roots, CallCtx};
use cluaiz_agents::workspace::{memory, Homes, Item, Kind, Workspace};
use cluaiz_cep::{Call, CepError, Meta, NoParams, Registry, Scope};
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::json;

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct SetParams {
    /// An existing folder: absolute, or relative to the current workspace
    path: String,
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct FolderParams {
    /// The project folder: absolute, or relative to the workspace (default: the workspace)
    path: Option<String>,
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct ItemParams {
    /// The item's name, as `workspace.index` lists it (an instruction file by its label, e.g. `.agents/AGENTS.md`)
    name: String,
    /// rule, workflow, skill or memory, when two items share a name
    kind: Option<String>,
    /// The project folder (default: the workspace)
    path: Option<String>,
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct SaveParams {
    /// A short name for the subject, e.g. `deploy-port`; it becomes the file name
    name: String,
    /// The fact, 1 to 2000 characters
    fact: String,
    /// Where it came from, e.g. `Cluaiz App, chat 12` (default: `cep`)
    source: Option<String>,
    /// The project folder (default: the workspace)
    path: Option<String>,
}

/// The project's agent files for a folder the caller may read
async fn scan(ctx: &CallCtx, call: &Call, path: Option<&str>) -> Result<Workspace, CepError> {
    let workspace = ctx.state.current_workspace_dir.read().await.clone();
    let dir = roots::resolve(path.unwrap_or("."), &workspace, false)?;
    call.check_path(&dir)?;
    if !dir.is_dir() {
        return Err(roots::invalid("path", "The project folder must be a directory"));
    }
    let cluaiz = cluaiz_core::EnvironmentManager::current().global_dir;
    let user = std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" }).map(std::path::PathBuf::from);
    Ok(Workspace::scan(&dir, Homes { cluaiz: Some(&cluaiz), user: user.as_deref() }, &[]))
}

fn item_json(item: &Item) -> serde_json::Value {
    let mut value = json!({
        "kind": item.kind.name(),
        "name": item.name,
        "description": item.description,
        "path": roots::display(&item.path),
        "found_in": item.source,
    });
    if item.kind == Kind::Rule {
        value["trigger"] = json!(item.trigger);
        value["globs"] = json!(item.globs);
    }
    if item.kind == Kind::Memory {
        value["status"] = json!(if item.superseded_by.is_some() { "superseded" } else { "confirmed" });
        value["superseded_by"] = json!(item.superseded_by.as_deref().filter(|name| !name.is_empty()));
    }
    value
}

fn index_json(workspace: &Workspace) -> serde_json::Value {
    let list = |items: &[Item]| items.iter().map(item_json).collect::<Vec<_>>();
    json!({
        "root": roots::display(&workspace.root),
        "instructions": workspace.instructions.iter().map(|file| json!({ "label": file.label, "path": roots::display(&file.path), "chars": file.total })).collect::<Vec<_>>(),
        "rules": list(&workspace.rules),
        "workflows": list(&workspace.workflows),
        "skills": list(&workspace.skills),
        "memory": list(&workspace.memory),
        "problems": workspace.problems,
    })
}

fn fact_example() -> serde_json::Value {
    json!({ "kind": "memory", "name": "test-command", "description": "The tests run with npm run check.", "path": "/project/.agents/memory/test-command.md", "found_in": ".agents/memory", "status": "confirmed", "superseded_by": null })
}

pub fn register(registry: &mut Registry<CallCtx>) {
    registry
        .add(
            "workspace.index",
            Meta::new(Scope::Read, "A project's agent files: AGENTS.md instructions, rules with their trigger, workflows, skills and memory facts, with name and description")
                .error(-32602, "invalid_params", "The path is not a folder, or is drive-relative / root-relative")
                .error(-32002, "forbidden", "The folder is outside the caller's roots (`data.needs: \"roots\"`)")
                .example(
                    json!({}),
                    json!({
                        "root": "/project",
                        "instructions": [{ "label": ".agents/AGENTS.md", "path": "/project/.agents/AGENTS.md", "chars": 1840 }],
                        "rules": [{ "kind": "rule", "name": "style", "description": "Code style for the Rust crates", "path": "/project/.agents/rules/style.md", "found_in": ".agents/rules", "trigger": "model_decision", "globs": [] }],
                        "workflows": [{ "kind": "workflow", "name": "release", "description": "Cut a release", "path": "/project/.agents/workflows/release.md", "found_in": ".agents/workflows" }],
                        "skills": [],
                        "memory": [fact_example()],
                        "problems": []
                    }),
                ),
            |ctx: CallCtx, call: Call, p: FolderParams| async move { Ok(index_json(&scan(&ctx, &call, p.path.as_deref()).await?)) },
        )
        .add(
            "workspace.item",
            Meta::new(Scope::Read, "The text of one rule, workflow, skill, memory fact or instruction file of a project, without its front matter")
                .error(-32602, "invalid_params", "`kind` is not rule, workflow, skill or memory")
                .error(-32004, "item_not_found", "Nothing of that name; `error.message` lists the names")
                .error(-32002, "forbidden", "The folder or the item's file is outside the caller's roots")
                .example(
                    json!({ "name": "release", "kind": "workflow" }),
                    json!({ "kind": "workflow", "name": "release", "description": "Cut a release", "path": "/project/.agents/workflows/release.md", "found_in": ".agents/workflows", "text": "1. Set the new version in Cargo.toml.\n2. Tag it." }),
                ),
            |ctx: CallCtx, call: Call, p: ItemParams| async move {
                let workspace = scan(&ctx, &call, p.path.as_deref()).await?;
                let kind = match p.kind.as_deref() {
                    Some(kind) => Some(Kind::parse(kind).ok_or_else(|| roots::invalid("kind", "kind is rule, workflow, skill or memory"))?),
                    None => None,
                };
                let name = p.name.trim();
                // A replaced fact is not in the index, but can still be read
                let item = workspace
                    .find(name, kind)
                    .or_else(|| workspace.memory.iter().find(|fact| kind.is_none_or(|k| k == Kind::Memory) && fact.name.eq_ignore_ascii_case(name)));
                if let Some(item) = item {
                    call.check_path(&item.path)?;
                    let text = item.body().map_err(|error| CepError::unavailable("item_unreadable", error))?;
                    let mut value = item_json(item);
                    value["text"] = json!(text);
                    return Ok(value);
                }
                if let Some(file) = workspace.instructions.iter().find(|file| kind.is_none() && file.label.eq_ignore_ascii_case(name)) {
                    call.check_path(&file.path)?;
                    let text = std::fs::read_to_string(&file.path).map_err(|error| roots::io_error("read", error))?;
                    return Ok(json!({ "kind": "instructions", "name": file.label, "path": roots::display(&file.path), "text": cluaiz_agents::workspace::split_front_matter(&text).1 }));
                }
                let names: Vec<&str> = workspace.index().map(|item| item.name.as_str()).chain(workspace.instructions.iter().map(|file| file.label.as_str())).collect();
                Err(CepError::not_found("item_not_found", format!("Nothing named {name}; names: {}", names.join(", "))))
            },
        )
        .add(
            "workspace.memory",
            Meta::new(Scope::Read, "A project's memory facts (`.agents/memory/`), live and replaced, with their text")
                .error(-32002, "forbidden", "The folder is outside the caller's roots")
                .example(
                    json!({}),
                    json!({ "facts": [{ "kind": "memory", "name": "test-command", "description": "The tests run with npm run check.", "path": "/project/.agents/memory/test-command.md", "found_in": ".agents/memory", "status": "confirmed", "superseded_by": null, "text": "The tests run with npm run check." }] }),
                ),
            |ctx: CallCtx, call: Call, p: FolderParams| async move {
                let workspace = scan(&ctx, &call, p.path.as_deref()).await?;
                let facts: Vec<_> = workspace
                    .memory
                    .iter()
                    .map(|fact| {
                        let mut value = item_json(fact);
                        value["text"] = json!(fact.body().unwrap_or_default());
                        value
                    })
                    .collect();
                Ok(json!({ "facts": facts }))
            },
        )
        .add(
            "workspace.remember",
            Meta::new(Scope::Write, "Saves a fact the user confirmed in the project's memory (`.agents/memory/<name>.md`); a live fact on the same subject or of the same name is marked superseded")
                // Memory outlives the chat: an MCP client must not write it unseen
                .no_mcp()
                .idempotent(false)
                .error(-32602, "invalid_params", "`fact` is empty or over 2000 characters")
                .error(-32002, "forbidden", "The folder is outside the caller's roots")
                .error(-32010, "memory_not_saved", "The fact file could not be written")
                .example(
                    json!({ "name": "test-command", "fact": "The tests run with npm run check.", "source": "Cluaiz App, chat 12" }),
                    json!({ "saved": "test-command", "path": "/project/.agents/memory/test-command.md", "replaced": [] }),
                ),
            |ctx: CallCtx, call: Call, p: SaveParams| async move {
                let workspace = scan(&ctx, &call, p.path.as_deref()).await?;
                memory::prepare(&p.name, &p.fact).map_err(|error| CepError::invalid_params(error, vec![json!({ "path": "fact", "message": "length" })], None))?;
                call.check_path(&memory::folder(&workspace))?;
                let saved = memory::save(&workspace, &p.name, &p.fact, p.source.as_deref().unwrap_or("cep"))
                    .map_err(|error| CepError::unavailable("memory_not_saved", format!("The fact was not saved: {error}")))?;
                Ok(json!({ "saved": saved.name, "path": roots::display(&saved.path), "replaced": saved.replaced }))
            },
        );

    registry.add(
        "workspace.get",
        Meta::new(Scope::Read, "Returns the current workspace directory")
            .example(json!({}), json!({"path": "/project"})),
        |ctx: CallCtx, call: Call, _: NoParams| async move {
            let workspace = ctx.state.current_workspace_dir.read().await.clone();
            let path = roots::resolve(".", &workspace, false)?;
            call.check_path(&path)?;
            Ok(json!({"path": roots::display(&path)}))
        },
    );
    registry.add(
        "workspace.set",
        Meta::new(
            Scope::Write,
            "Changes the workspace to an existing directory",
        )
        .no_mcp()
        .error(-32602, "invalid_params", "The path is not a folder, or is drive-relative / root-relative")
        .error(-32004, "file_not_found", "The folder does not exist")
        .error(-32002, "forbidden", "The folder is outside the caller's roots (`data.needs: \"roots\"`)")
        .example(json!({"path": "/project"}), json!({"path": "/project"})),
        |ctx: CallCtx, call: Call, params: SetParams| async move {
            let workspace = ctx.state.current_workspace_dir.read().await.clone();
            let path = roots::resolve(&params.path, &workspace, false)?;
            call.check_path(&path)?;
            if !path.is_dir() {
                return Err(roots::invalid("path", "Workspace must be a directory"));
            }
            assign(&ctx, &path).await;
            Ok(json!({"path": roots::display(&path)}))
        },
    );
}

pub(super) async fn assign(ctx: &CallCtx, path: &std::path::Path) {
    // Keep the existing App/agent workspace owner; do not create a second workspace store.
    let mut workspace = ctx.state.current_workspace_dir.write().await;
    *workspace = path.to_path_buf();
    cluaiz_agents::ToolsEngine::set_workspace_dir(path.to_path_buf());
}
