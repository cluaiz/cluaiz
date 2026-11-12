//! The flow format, described once (docs/automation/file.md, steps/, triggers.md). The checker,
//! the JSON Schema of `flows.schema` and the step-kind list of `flows.nodes` all read these tables.

pub const VERSION: &str = "cluaiz.flow/1";
pub const VERSIONS: [&str; 1] = [VERSION];

#[derive(Debug, Clone, Copy)]
pub enum Ty {
    Text,
    Bool,
    Int,
    Number,
    Duration,
    Size,
    Time,
    /// A decision or a value expression: `true`, `false` or an operator object
    Expr,
    Any,
    Enum(&'static [&'static str]),
    List(&'static Ty),
    Object(&'static [Field]),
    /// Any names → values of one type
    Map(&'static Ty),
    Steps,
    Triggers,
    /// A JSON Schema (an object)
    Schema,
    /// Data for others, never read (`ui`, `meta`)
    Open,
    OneOf(&'static [Ty]),
}

#[derive(Debug, Clone, Copy)]
pub struct Field {
    pub name: &'static str,
    pub ty: Ty,
    pub required: bool,
    pub desc: &'static str,
}

const fn f(name: &'static str, ty: Ty, desc: &'static str) -> Field {
    Field { name, ty, required: false, desc }
}

const fn req(name: &'static str, ty: Ty, desc: &'static str) -> Field {
    Field { name, ty, required: true, desc }
}

pub const SCOPES: &[&str] = &["read", "write", "exec"];
pub const ERROR_KINDS_TEXT: &str = "network, rate_limit, timeout, provider, model, tool, invalid_input, invalid_output, expression, permission, approval_rejected, not_found, conflict, unavailable, code, limit, cancelled, not_callable, failed";

pub const RETRY: &[Field] = &[
    req("max", Ty::Int, "Attempts after the first, 1–20"),
    f("delay", Ty::Duration, "Wait before the first retry (default 5s)"),
    f("backoff", Ty::Enum(&["fixed", "exponential"]), "exponential doubles the wait each time (default)"),
    f("max_delay", Ty::Duration, "The longest wait (default 5m)"),
    f("jitter", Ty::Bool, "Adds up to 20% at random (default true)"),
    f("on", Ty::List(&Ty::Text), "Error kinds that are retried (default network, rate_limit, timeout); [any] retries every kind but cancelled, permission and approval_rejected"),
    f("respect_retry_after", Ty::Bool, "Wait as long as a provider's Retry-After asks (default true)"),
];

pub const PERMISSIONS: &[Field] = &[
    f("tools", Ty::List(&Ty::Text), "Tools for tool steps, ask and agent tools, poll triggers, approve notify (`mcp:github/*`, `*`)"),
    f("methods", Ty::List(&Ty::Text), "CEP methods and Engine routes for call steps and poll triggers (`fs.*`)"),
    f("folders", Ty::List(&Ty::Text), "Folders that steps and file triggers may use"),
    f("network", Ty::List(&Ty::Text), "Hosts code steps may reach (`*.example.com`)"),
    f("models", Ty::List(&Ty::Text), "Models for ask and agent: model ids, `<provider>/*`, or `*`"),
    f("scopes", Ty::List(&Ty::Enum(SCOPES)), "The highest CEP scopes call steps may use; admin is never granted"),
    f("outside_content", Ty::Bool, "The flow handles outside content (webhooks, web pages, mail)"),
];

pub const LIMITS: &[Field] = &[
    f("timeout", Ty::Duration, "Running time of the whole run (default 1h, at most 7d)"),
    f("max_wait", Ty::Duration, "Time a run may spend waiting (default 7d, at most 90d)"),
    f("max_steps", Ty::Int, "Steps run, each item and round counted (default 10000)"),
    f("max_parallel", Ty::Int, "Steps of this run at the same time (default 4, at most 64)"),
    f("result_in_memory", Ty::Size, "A larger value is kept as a file reference (default 1MB, at most 16MB)"),
];

pub const CONCURRENCY_MODES: &[&str] = &["queue", "skip", "replace", "parallel"];

pub const CONCURRENCY_OBJECT: &[Field] = &[
    req("mode", Ty::Enum(CONCURRENCY_MODES), "As the short form"),
    f("key", Ty::Text, "The rule applies per value of this reference"),
    f("max", Ty::Int, "How many may run together for the same key"),
];

pub const RETENTION: &[Field] = &[f("days", Ty::Int, "Days runs are kept"), f("runs", Ty::Int, "Runs kept")];

pub const FLOW: &[Field] = &[
    req("flow", Ty::Text, "The format version: cluaiz.flow/1"),
    req("name", Ty::Text, "1–64 characters: a-z, 0-9, -, _; starts with a letter or digit. Matches the file name"),
    f("description", Ty::Text, "What the flow does"),
    f("version", Ty::Text, "The author's own version text"),
    f("timezone", Ty::Text, "IANA name (Asia/Kolkata); default the computer's"),
    f("inputs", Ty::Map(&Ty::Schema), "Input name → JSON Schema"),
    f("outputs", Ty::Map(&Ty::Any), "Output name → value (references allowed)"),
    f("on", Ty::Triggers, "Triggers"),
    f("permissions", Ty::Object(PERMISSIONS), "What the flow may use"),
    f("secrets", Ty::List(&Ty::Text), "Names of the secrets the flow reads as env:NAME"),
    f("limits", Ty::Object(LIMITS), "Run limits"),
    f("concurrency", Ty::OneOf(&[Ty::Enum(CONCURRENCY_MODES), Ty::Object(CONCURRENCY_OBJECT)]), "When a trigger fires while a run of this flow is going: queue (default), skip, replace, parallel"),
    f("retention", Ty::Object(RETENTION), "How long this flow's runs are kept"),
    req("steps", Ty::Steps, "At least one step"),
    f("on_error", Ty::Steps, "Steps run when the run fails, with `error` set"),
    f("ui", Ty::Open, "The application's own data; never read by Automation"),
    f("meta", Ty::Open, "Tags, author, links"),
];

/// Fields every step may have
pub const COMMON: &[Field] = &[
    f("id", Ty::Text, "1–64 characters: letters, digits, _; starts with a letter. Unique in the flow"),
    f("name", Ty::Text, "Text for people"),
    f("when", Ty::Expr, "Checked just before the step; false skips it"),
    f("needs", Ty::List(&Ty::Text), "Ids of steps this one waits for (default the step above)"),
    f("with", Ty::Any, "The step's input; references are resolved before it starts"),
    f("output", Ty::Any, "What to keep from the result (`result` is the raw result)"),
    f("retry", Ty::Object(RETRY), "Retries of a failed attempt"),
    f("timeout", Ty::Duration, "The step fails with kind timeout after this long"),
    f("on_error", Ty::OneOf(&[Ty::Enum(&["stop", "continue"]), Ty::Steps]), "stop (default), continue, or steps run with `error` set"),
    f("approve_before", Ty::Text, "A question; the step waits for a person's yes before it starts"),
    f("idempotency_key", Ty::Text, "A step with this key that already succeeded is not run again"),
    f("on_unknown", Ty::Enum(&["ask", "repeat"]), "A step with an idempotency_key that was mid-call when Automation stopped: ask the owner (default) or repeat it"),
    f("secret_output", Ty::Bool, "Mask the output in records"),
];

pub struct KindDef {
    pub key: &'static str,
    pub ty: Ty,
    pub fields: &'static [Field],
    pub desc: &'static str,
    /// Built in this release; other kinds are read and checked, and a run reports them as not built
    pub runs: bool,
}

const ASK_FIELDS: &[Field] = &[
    f("model", Ty::Text, "An installed model id, or <provider>/<model>; default the default chat model"),
    f("system", Ty::Text, "Extra system text, before the instruction"),
    f("output_schema", Ty::Schema, "The answer must be JSON of this shape"),
    f("tools", Ty::List(&Ty::Text), "Tools the model may call while answering"),
    f("attachments", Ty::List(&Ty::Any), "Files, images or sounds for the model"),
    f("think", Ty::Enum(&["on", "off", "auto"]), "Thinking, as in chat (default auto)"),
    f("effort", Ty::Enum(&["low", "medium", "high"]), "Thinking effort"),
    f("max_tokens", Ty::Int, "Most tokens in the answer"),
    f("temperature", Ty::Number, "As in chat"),
    f("top_p", Ty::Number, "As in chat"),
    f("seed", Ty::Int, "As in chat"),
    f("session", Ty::Text, "A chat id whose memory the model gets"),
    f("schema_retries", Ty::Int, "Times to ask again when the answer does not match output_schema (default 1)"),
];

const AGENT_FIELDS: &[Field] = &[
    f("model", Ty::Text, "As in ask"),
    f("system", Ty::Text, "As in ask"),
    f("output_schema", Ty::Schema, "As in ask"),
    req("tools", Ty::List(&Ty::Text), "Tools the agent may call"),
    f("max_turns", Ty::Int, "Most rounds of tool calls (default 10)"),
    f("tool_search", Ty::Bool, "Let the agent find other allowed tools"),
    f("stop_when", Ty::Expr, "Checked after each round; true stops the agent"),
    f("think", Ty::Enum(&["on", "off", "auto"]), "As in ask"),
    f("max_tokens", Ty::Int, "As in ask"),
    f("session", Ty::Text, "As in ask"),
    f("allow_untrusted", Ty::Bool, "The owner read the untrusted_with_tools warning: no approval before the step"),
];

const SWITCH_CASE: &[Field] = &[
    f("name", Ty::Text, "A name for the case"),
    f("when", Ty::Expr, "The case holds when this is true"),
    f("is", Ty::Any, "With `match`: a value or a list of values"),
    req("do", Ty::Steps, "The case's steps"),
];

const WAIT_FORM: &[Field] = &[
    f("for", Ty::Duration, "Wait this long"),
    f("until", Ty::Text, "Wait until this time"),
    f("event", Ty::Text, "Wait for this event"),
    f("filter", Ty::Expr, "With event: only a matching event"),
    f("webhook", Ty::Bool, "Wait for a request to the run's own address"),
];

pub const KINDS: &[KindDef] = &[
    KindDef { key: "tool", ty: Ty::Text, fields: &[f("settings", Ty::Map(&Ty::Any), "Settings for this call only; values may be env:NAME")], desc: "Runs an installed MCP tool, plugin function or skill", runs: true },
    KindDef { key: "call", ty: Ty::Text, fields: &[], desc: "Runs a CEP method or an Engine route", runs: true },
    KindDef { key: "ask", ty: Ty::Text, fields: ASK_FIELDS, desc: "One answer from a model", runs: true },
    KindDef { key: "agent", ty: Ty::Text, fields: AGENT_FIELDS, desc: "A model with several rounds of tool calls", runs: true },
    KindDef {
        key: "code",
        ty: Ty::Text,
        fields: &[f("fn", Ty::Text, "The exported function (default main)"), f("memory", Ty::Size, "Memory the module may use (default and most 256MB)"), f("fuel", Ty::Int, "Instructions it may run")],
        desc: "Your WebAssembly module",
        runs: true,
    },
    KindDef { key: "if", ty: Ty::Expr, fields: &[req("then", Ty::Steps, "Runs when true"), f("else", Ty::Steps, "Runs when false")], desc: "Runs then or else", runs: true },
    KindDef {
        key: "switch",
        ty: Ty::List(&Ty::Object(SWITCH_CASE)),
        fields: &[f("default", Ty::Steps, "Runs when no case holds"), f("match", Ty::Any, "Short form: each case has `is` instead of `when`")],
        desc: "Runs the first case that holds",
        runs: true,
    },
    KindDef {
        key: "each",
        ty: Ty::Any,
        fields: &[
            f("as", Ty::Text, "The name of the current item (default item)"),
            req("do", Ty::Steps, "The steps for each item"),
            f("parallel", Ty::Int, "Items at the same time (default 1)"),
            f("batch", Ty::Int, "Groups of this many items"),
            f("stop_on_error", Ty::Bool, "The first failed item stops the rest (default true)"),
            f("delay", Ty::Duration, "Wait between starting items"),
        ],
        desc: "Runs steps for every item of a list",
        runs: true,
    },
    KindDef {
        key: "parallel",
        ty: Ty::Map(&Ty::Steps),
        fields: &[f("wait", Ty::Enum(&["all", "any"]), "all (default) or any"), f("stop_on_error", Ty::Bool, "A failed branch cancels the others (default true)")],
        desc: "Runs named branches at the same time",
        runs: true,
    },
    KindDef {
        key: "repeat",
        ty: Ty::Steps,
        fields: &[
            req("until", Ty::Expr, "Checked after each round; true ends the step"),
            req("max", Ty::Int, "Most rounds, 1–10000"),
            f("delay", Ty::Duration, "Wait between rounds"),
            f("on_max", Ty::Enum(&["fail", "continue"]), "When max rounds ran: fail (default) or go on"),
        ],
        desc: "Runs steps again until a condition holds",
        runs: true,
    },
    KindDef {
        key: "wait",
        ty: Ty::Object(WAIT_FORM),
        fields: &[f("on_timeout", Ty::Enum(&["fail", "continue"]), "fail (default) or continue")],
        desc: "Waits for a time, a duration, an event or a webhook",
        runs: true,
    },
    KindDef {
        key: "approve",
        ty: Ty::Text,
        fields: &[
            f("on_timeout", Ty::Enum(&["reject", "approve", "fail"]), "What a timeout counts as (default reject)"),
            f("on_reject", Ty::Enum(&["fail", "continue"]), "fail (default) or continue"),
            f("notify", Ty::Text, "A tool called when the question is asked"),
        ],
        desc: "Asks a person yes or no",
        runs: true,
    },
    KindDef { key: "set", ty: Ty::Map(&Ty::Any), fields: &[], desc: "Keeps values in vars", runs: true },
    KindDef {
        key: "run",
        ty: Ty::Text,
        fields: &[f("scope", Ty::Enum(&["project", "user"]), "Which flow of that name"), f("wait", Ty::Bool, "Wait for its outputs (default true)")],
        desc: "Runs another flow",
        runs: true,
    },
    KindDef { key: "emit", ty: Ty::Text, fields: &[], desc: "Sends an event", runs: true },
    KindDef { key: "return", ty: Ty::Any, fields: &[], desc: "Ends the run with these outputs", runs: true },
    KindDef {
        key: "fail",
        ty: Ty::Text,
        fields: &[f("kind", Ty::Text, "The error kind recorded (default failed)"), f("details", Ty::Any, "Kept in error.details")],
        desc: "Ends the run with an error",
        runs: true,
    },
];

pub fn kind(key: &str) -> Option<&'static KindDef> {
    KINDS.iter().find(|k| k.key == key)
}

/// Fields of a kind that are also common fields keep the common meaning (`with`, `timeout`)
pub fn kind_field(kind: &KindDef, name: &str) -> Option<&'static Field> {
    kind.fields.iter().find(|f| f.name == name)
}

/// Fields every trigger may have
pub const TRIGGER_COMMON: &[Field] = &[
    f("id", Ty::Text, "Names the trigger in records (default kind and position: schedule_0)"),
    f("filter", Ty::Expr, "Checked with `trigger` filled in; false means no run"),
    f("dedupe", Ty::Text, "Two events with the same value start one run"),
    f("dedupe_for", Ty::Duration, "How long a dedupe value is remembered (default 24h)"),
    f("inputs", Ty::Map(&Ty::Any), "The run's inputs; may refer to trigger"),
    f("enabled", Ty::Bool, "Turns this trigger off without removing it"),
];

const CATCH_UP: Ty = Ty::Enum(&["skip", "once", "all"]);

pub struct TriggerDef {
    pub key: &'static str,
    pub ty: Ty,
    /// Fields written next to the kind key (`schedule` and `every`)
    pub siblings: &'static [Field],
    pub desc: &'static str,
}

pub const TRIGGERS: &[TriggerDef] = &[
    TriggerDef { key: "manual", ty: Ty::Open, siblings: &[], desc: "Someone or something starts it" },
    TriggerDef {
        key: "schedule",
        ty: Ty::Text,
        siblings: &[f("timezone", Ty::Text, "IANA name"), f("catch_up", CATCH_UP, "Times missed: skip (default), once, all"), f("jitter", Ty::Duration, "A random delay up to this long")],
        desc: "A cron time comes",
    },
    TriggerDef { key: "every", ty: Ty::Duration, siblings: &[f("starting", Ty::Time, "The first time"), f("catch_up", CATCH_UP, "As for schedule")], desc: "An interval passes" },
    TriggerDef {
        key: "webhook",
        ty: Ty::Object(&[
            req("secret", Ty::Text, "env:NAME: the secret callers must prove"),
            f("signature", Ty::Enum(&["header", "hmac-sha256"]), "How callers prove it (default header)"),
            f("signature_header", Ty::Text, "Another header name for the signature"),
            f("methods", Ty::List(&Ty::Text), "Allowed HTTP methods (default POST)"),
            f("respond", Ty::Enum(&["accepted", "result"]), "accepted (default) or result"),
            f("respond_timeout", Ty::Duration, "For respond: result (default 30s)"),
            f("max_body", Ty::Size, "Larger bodies get 413 (default 1MB)"),
        ]),
        siblings: &[],
        desc: "An outside request with the flow's secret arrives",
    },
    TriggerDef {
        key: "poll",
        ty: Ty::Object(&[
            f("tool", Ty::Text, "What to run each time"),
            f("call", Ty::Text, "Or a method"),
            f("with", Ty::Map(&Ty::Any), "Its input"),
            req("every", Ty::Duration, "At least 1m"),
            f("new_when", Ty::Any, "changed (default), new_items, or an expression"),
            f("items", Ty::Text, "Where the list is, for new_items"),
            f("key", Ty::Text, "What makes an item the same item"),
            f("first", Ty::Enum(&["skip", "run"]), "On the first poll: skip (default) or run"),
        ]),
        siblings: &[],
        desc: "A tool's answer has something new",
    },
    TriggerDef {
        key: "file",
        ty: Ty::Object(&[
            req("path", Ty::Text, "A folder inside permissions.folders"),
            f("glob", Ty::Text, "Matched against file names (default *)"),
            f("events", Ty::List(&Ty::Enum(&["created", "changed", "removed"])), "Default [created]"),
            f("recursive", Ty::Bool, "Include sub-folders"),
            f("debounce", Ty::Duration, "Wait until the file is still (default 2s)"),
            f("ignore", Ty::List(&Ty::Text), "Globs that never start a run"),
        ]),
        siblings: &[],
        desc: "A file appears, changes or goes",
    },
    TriggerDef { key: "engine", ty: Ty::Text, siblings: &[], desc: "An Engine event" },
    TriggerDef { key: "event", ty: Ty::Text, siblings: &[], desc: "Another flow sends an event" },
    TriggerDef {
        key: "tool",
        ty: Ty::Object(&[
            f("description", Ty::Text, "What a calling model reads"),
            f("title", Ty::Text, "Shown in tool lists"),
            f("confirm", Ty::OneOf(&[Ty::Bool, Ty::Enum(&["auto", "full_access", "sandboxed", "strict"])]), "Ask the owner before a call runs the flow: true, false, or a mode (auto follows the owner's agent security mode at the time of the call)"),
        ]),
        siblings: &[],
        desc: "The flow is called as a tool",
    },
    TriggerDef { key: "startup", ty: Ty::Open, siblings: &[], desc: "Automation starts" },
];

pub fn trigger(key: &str) -> Option<&'static TriggerDef> {
    TRIGGERS.iter().find(|t| t.key == key)
}
