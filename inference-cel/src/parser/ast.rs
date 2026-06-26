use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// A single value in a CEL expression
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum CelValue {
    Text(String),
    Number(f64),
    Bool(bool),
    Vector(Vec<f32>),
    Variable(String), // E.g., $user
    Parameter, // Placeholder ?
    Null,
}

/// Comparison operators for basic filters
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum CompareOp {
    Eq,        // =
    NotEq,     // !=
    Gt,        // >
    Lt,        // <
    Gte,       // >=
    Lte,       // <=
    Contains,  // contains
}

/// A single key-value filter condition
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Filter {
    pub field: String,
    pub op: CompareOp,
    pub value: CelValue,
}

/// Core CEL Operations / Directives within a Pipeline
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum CelOp {
    /// `use plugin::<name>` -> Import a skill/plugin WASM DNA
    ImportPlugin {
        name: String,
    },
    
    /// `invoke(<action>, args...)` -> Call a complex action inside the plugin
    InvokeAction {
        method: String,
        args: HashMap<String, CelValue>,
    },
    
    /// `-> filter age > 18` -> Natively parsed structural filter
    Filter {
        field: String,
        op: CompareOp,
        value: CelValue,
    },

    /// Fast Path Action for simple string processing (e.g. `process('Hello')`)
    FastProcess {
        method: String,
        payload_text: String,
    },

    /// `->` Pipe the memory stream to the next plugin or execution node
    Pipe {
        next_plugin: String,
    },
    
    /// Time Series / State Window logic
    TimeWindow {
        size: String,
    },

    /// Vector Search inside the KV Cache / VRAM
    SimilarTo {
        vector: Vec<f32>,
        metric: String,
    },

    /// Generic Command / CDQL Query (e.g., `find User(age: >18)`)
    Command {
        action: String,
        target: Option<String>,
        args: HashMap<String, CelValue>,
    },

    /// Projection / Select specific fields to save RAM
    Select {
        fields: Vec<String>,
    },

    // ── Hardcore Engine Directives (Phase 3 Ecosystem) ──

    /// `engine -> kv_cache -> clear($user_id)`
    EngineMemoryControl {
        action: String,
        target: String,
    },
    
    /// `engine -> mid_layer -> inject($data)`
    MidLayerInjection {
        payload: CelValue,
    },
    
    /// `engine -> inference -> pause()`
    InferenceControl {
        command: String,
    },
    
    /// `engine -> os -> process("ps")` (If permitted)
    SystemCall {
        command: String,
        args: Vec<CelValue>,
    }
}

/// A sequential pipeline of operations (e.g. `use plugin -> find -> limit`)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CelPipeline {
    pub ops: Vec<CelOp>,
}

impl CelPipeline {
    pub fn new() -> Self {
        Self { ops: Vec::new() }
    }
}

/// High-Level Language Constructs (The Turing Complete Wrapper)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum CelStatement {
    /// A standalone pipeline execution
    Expression(CelPipeline),
    
    /// Variable assignment: `let x = use plugin::db...`
    Assignment {
        var_name: String,
        pipeline: CelPipeline,
    },
    
    /// Conditional branching: `if ($x > 0) { ... } else { ... }`
    IfElse {
        condition: String,
        if_block: Box<CelAst>,
        else_block: Option<Box<CelAst>>,
    },

    /// Iterator loop: `foreach ($item in $list) { ... }`
    Foreach {
        item_var: String,
        list_var: String,
        block: Box<CelAst>,
    }
}

/// The full CEL AST tree holding multiple statements
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CelAst {
    pub statements: Vec<CelStatement>,
}

impl CelAst {
    pub fn new() -> Self {
        Self { statements: Vec::new() }
    }
}
