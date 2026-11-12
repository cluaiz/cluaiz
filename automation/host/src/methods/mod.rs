//! The `flows.*` method group, served at `/v1/cep` next to the Engine's methods
//! (docs/automation/methods/): schema, validate, save, get, list, remove, enable, disable, run,
//! rerun_from, runs, run_get, nodes, explain, draft and fix.

mod common;
mod drafting;
mod ease;
mod files;
mod runs;
mod switch;

use cluaiz_api::cep::CallCtx;
use cluaiz_cep::{Meta, Registry, Scope};
use serde_json::json;

const NOT_FOUND: i32 = -32004;
const INVALID: i32 = -32602;
const CONFLICT: i32 = -32005;
const APPROVAL: i32 = -32003;
const TOO_LARGE: i32 = -32006;
/// A model gave no usable answer
const MODEL_FAILED: i32 = -32010;

pub fn register(registry: &mut Registry<CallCtx>) {
    let summary = json!({ "name": "weekly-issues", "scope": "user", "path": "~/.cluaiz/flows/weekly-issues.flow.yaml", "format": "yaml", "enabled": true,
        "triggers": [{ "id": "manual_0", "kind": "manual", "status": "manual" }], "hash": "sha256:4b1e…", "last_run": null, "warnings": 0, "tags": [] });
    registry.add(
        "flows.schema",
        Meta::new(Scope::Read, "The JSON Schema of the flow file, for editors, applications and models")
            .error(INVALID, "unknown_version", "`version` is not one this release of Automation knows")
            .example(json!({}), json!({ "version": "cluaiz.flow/1", "versions": ["cluaiz.flow/1"], "formats": ["json", "yaml"], "schema": { "title": "Cluaiz flow" } })),
        files::schema,
    );
    registry.add(
        "flows.validate",
        Meta::new(Scope::Read, "Checks a flow without saving it: problems with their place, the permissions it needs, the flow with defaults")
            .error(TOO_LARGE, "too_large", "The text is over 1 MB")
            .error(NOT_FOUND, "flow_not_found", "`name` names no flow")
            .example(
                json!({ "flow": { "flow": "cluaiz.flow/1", "name": "hello", "steps": [{ "set": { "greeting": "hi" } }] } }),
                json!({ "valid": true, "problems": [], "permissions_needed": {}, "permissions_missing": null, "outside_content": false, "normalized": { "name": "hello" }, "hash": "sha256:…" }),
            ),
        files::validate,
    );
    registry.add(
        "flows.save",
        Meta::new(Scope::Write, "Saves a new flow, or replaces one; the owner confirms the permissions it needs")
            .error(INVALID, "invalid_flow", "The flow has errors; `data.problems` lists them")
            .error(CONFLICT, "permissions_unconfirmed", "The flow needs permissions that are not confirmed; `data.permissions_needed`")
            .error(CONFLICT, "flow_exists", "A flow of this name exists in the scope")
            .error(CONFLICT, "no_workspace", "`scope: project` without a current workspace")
            .error(APPROVAL, "approval_required", "A caller other than the owner replaces a file or confirms permissions")
            .error(TOO_LARGE, "too_large", "The text is over 1 MB")
            .example(json!({ "text": "flow: cluaiz.flow/1\nname: hello\nsteps:\n  - set: { greeting: hi }\n", "format": "yaml" }), json!({ "flow": summary, "permissions": {}, "problems": [] })),
        files::save,
    );
    registry.add(
        "flows.get",
        Meta::new(Scope::Read, "One flow: its summary, its file as text and as JSON, and its triggers' state")
            .error(NOT_FOUND, "flow_not_found", "No flow with this name in the scope (or in either scope)")
            .example(json!({ "name": "weekly-issues" }), json!({ "flow": summary, "format": "yaml", "text": "flow: cluaiz.flow/1\n…", "definition": { "flow": "cluaiz.flow/1" } })),
        files::get,
    );
    registry.add(
        "flows.list",
        Meta::new(Scope::Read, "The flows of the project and the user, with their state and last run").example(json!({}), json!({ "flows": [summary] })),
        files::list,
    );
    registry.add(
        "flows.remove",
        Meta::new(Scope::Write, "Removes a flow file, and its runs unless kept")
            .destructive()
            .error(NOT_FOUND, "flow_not_found", "No flow with this name in the scope (or in either scope)")
            .error(CONFLICT, "flow_running", "Runs are going; `data.runs` lists them")
            .error(APPROVAL, "approval_required", "A caller other than the owner deletes a flow file")
            .example(json!({ "name": "water-reminder" }), json!({ "removed": "water-reminder", "scope": "user", "runs_removed": 12, "runs_cancelled": 0 })),
        files::remove,
    );
    registry.add(
        "flows.enable",
        Meta::new(Scope::Write, "Turns a flow's triggers on; confirms new permissions of a changed file")
            .owner_only()
            .no_mcp()
            .error(NOT_FOUND, "flow_not_found", "No flow with this name in the scope (or in either scope)")
            .error(INVALID, "invalid_flow", "The flow has errors; `data.problems` lists them")
            .error(CONFLICT, "permissions_unconfirmed", "The file asks for permissions not confirmed; `data.permissions_missing`")
            .example(json!({ "name": "weekly-issues" }), json!({ "flow": summary, "triggers": [{ "id": "schedule_0", "kind": "schedule", "status": "active", "next_at": "2026-11-13T12:30:00.000Z" }] })),
        switch::enable,
    );
    registry.add(
        "flows.disable",
        Meta::new(Scope::Write, "Turns a flow's triggers off; runs in progress go on unless cancelled")
            .owner_only()
            .no_mcp()
            .error(NOT_FOUND, "flow_not_found", "No flow with this name in the scope (or in either scope)")
            .example(json!({ "name": "weekly-issues" }), json!({ "flow": summary, "runs_cancelled": 0 })),
        switch::disable,
    );
    registry.add(
        "flows.run",
        Meta::new(Scope::Exec, "Starts a run of a flow (a task): by hand, as a test with sample data, or without side effects")
            .task()
            .no_mcp()
            .error(NOT_FOUND, "flow_not_found", "No flow with this name in the scope (or in either scope)")
            .error(INVALID, "invalid_flow", "The flow has errors; `data.problems` lists them")
            .error(INVALID, "invalid_input", "The inputs do not match the flow's `inputs`; `data.errors`")
            .error(CONFLICT, "skipped", "`concurrency: skip` and a run is going, or the queue is full")
            .error(CONFLICT, "permissions_unconfirmed", "The flow's permissions are not confirmed")
            .example(json!({ "name": "weekly-issues", "scope": "user" }), json!({ "task": { "task_id": "t_…", "status": "working" }, "run_id": "run_8f2c51d0e1a4", "status": "running" })),
        runs::run,
    );
    registry.add(
        "flows.rerun_from",
        Meta::new(Scope::Exec, "Starts a new run that reuses the outputs before one step of an earlier run and runs from that step")
            .task()
            .no_mcp()
            .error(NOT_FOUND, "run_not_found", "No run with this id (it may have been removed by retention)")
            .error(NOT_FOUND, "step_not_found", "`from` is not a step of the flow, or the earlier run never reached it")
            .error(CONFLICT, "run_not_finished", "The earlier run is still queued, running or waiting")
            .error(INVALID, "invalid_flow", "The flow has errors; `data.problems` lists them")
            .error(CONFLICT, "permissions_unconfirmed", "The flow's permissions are not confirmed")
            .example(json!({ "run_id": "run_8f2c51d0e1a4", "from": "report" }), json!({ "task": { "task_id": "t_…", "status": "working" }, "run_id": "run_c0a91e3f77b2", "rerun_of": "run_8f2c51d0e1a4", "from": "report" })),
        runs::rerun_from,
    );
    registry.add(
        "flows.runs",
        Meta::new(Scope::Read, "Runs of one flow or of all flows, newest first, page by page")
            .error(NOT_FOUND, "flow_not_found", "`name` names no flow and no runs")
            .example(json!({ "name": "weekly-issues", "limit": 20 }), json!({ "runs": [], "next": null })),
        runs::runs,
    );
    registry.add(
        "flows.run_get",
        Meta::new(Scope::Read, "One run with its trigger data and every step's record: input, output, attempts, tokens")
            .error(NOT_FOUND, "run_not_found", "No run with this id (it may have been removed by retention)")
            .error(NOT_FOUND, "step_not_found", "`step` is not a step of this run's flow")
            .example(json!({ "run_id": "run_8f2c51d0e1a4" }), json!({ "run": { "run_id": "run_8f2c51d0e1a4", "status": "succeeded" }, "trigger": { "kind": "manual" }, "steps": [], "next": null })),
        runs::run_get,
    );
    registry.add(
        "flows.nodes",
        Meta::new(Scope::Read, "Everything a step can run here: installed tools and CEP methods with their input and output schemas; plus step kinds, triggers and operators")
            .example(
                json!({ "kind": "tool", "query": "github", "catalog": false }),
                json!({ "nodes": [{ "ref": "mcp:github/create_issue", "step": "tool", "source": "mcp", "title": "create_issue", "description": "Creates an issue in a repository", "input_schema": { "type": "object" }, "output_schema": null, "sends": true, "destructive": false, "needs_approval": false }] }),
            ),
        ease::nodes,
    );
    registry.add(
        "flows.explain",
        Meta::new(Scope::Read, "A flow in plain words: what starts it, what each step does, what it may touch")
            .error(NOT_FOUND, "flow_not_found", "No flow with this name in the scope (or in either scope)")
            .error(INVALID, "invalid_flow", "The flow has errors; `data.problems` lists them")
            .error(NOT_FOUND, "model_not_found", "`mode: model` and no such model, or no default chat model")
            .error(MODEL_FAILED, "model_unavailable", "`mode: model` and the model did not answer")
            .example(
                json!({ "name": "weekly-issues" }),
                json!({ "summary": "Every Friday at 18:00 → run mcp:github/list_issues with repo → a model answers “Write a short weekly report.”", "triggers": ["Every Friday at 18:00"], "steps": [{ "id": "closed", "text": "Run mcp:github/list_issues with repo", "depth": 0, "list": "steps" }], "touches": ["Runs tools: mcp:github/list_issues"], "warnings": [] }),
            ),
        ease::explain,
    );
    registry.add(
        "flows.draft",
        Meta::new(Scope::Exec, "Writes a flow from a description, using only what is installed; checks it and can test it; does not save")
            .task()
            .no_mcp()
            .error(NOT_FOUND, "model_not_found", "No such model, or no default chat model")
            .error(MODEL_FAILED, "draft_failed", "The model gave no valid flow after `rounds`; `data.problems` and `data.text` hold the last attempt")
            .error(MODEL_FAILED, "model_unavailable", "The model did not answer")
            .example(
                json!({ "request": "When a PDF arrives in Downloads, put a five-line summary in my Notes folder" }),
                json!({ "task": { "task_id": "t_…", "method": "flows.draft", "status": "working" } }),
            ),
        drafting::draft,
    );
    registry.add(
        "flows.fix",
        Meta::new(Scope::Exec, "Proposes a change to a flow from a failed run or from validation problems; does not save")
            .task()
            .no_mcp()
            .error(NOT_FOUND, "run_not_found", "No run with this id (it may have been removed by retention)")
            .error(CONFLICT, "run_not_failed", "The run did not fail")
            .error(NOT_FOUND, "model_not_found", "No model to write the fix")
            .error(MODEL_FAILED, "fix_failed", "The model gave no corrected flow; `data.text` holds its last answer")
            .error(MODEL_FAILED, "model_unavailable", "The model did not answer")
            .example(json!({ "run_id": "run_5b07e2c9" }), json!({ "task": { "task_id": "t_…", "method": "flows.fix", "status": "working" } })),
        drafting::fix,
    );
}
