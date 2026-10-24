//! 📦 API Handlers Subsystem (`api/src/handlers/mod.rs`)
//! Modular domain controllers for Cluaiz API gateway.

pub mod audio;
pub mod chat;
pub mod chat_import;
pub mod chat_memory;
pub mod components;
pub mod context;
pub mod control;
pub mod embeddings;
pub mod fs;
pub mod fs_jail;
pub mod ingest;
mod ingest_model;
pub mod mcp;
pub mod memory;
pub mod models;
pub mod optimization;
pub mod permission;
pub mod plugins;
pub mod ps;
pub mod session_tools;
pub mod setup;
pub mod skills;
pub mod storage;
pub mod system;
