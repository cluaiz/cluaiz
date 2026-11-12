//! Cluaiz Automation: flows that start on a trigger and use the Engine's tools, methods and models.
//! This crate holds no Engine code. Every step that reaches the Engine goes through
//! [`host::StepHost`], which the Engine side implements (docs/automation/README.md).

pub mod ease;
pub mod error;
pub mod expr;
pub mod format;
pub mod host;
pub mod journal;
pub mod limits;
pub mod permissions;
pub mod problem;
pub mod refs;
pub mod run;
pub mod schema;
pub mod trigger;
pub mod store;
pub mod units;
pub mod validate;

pub use error::{FlowError, Kind};
