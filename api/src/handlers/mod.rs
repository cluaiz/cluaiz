//! 📦 API Handlers Subsystem (`api/src/handlers/mod.rs`)
//! Modular domain controllers for Cluaiz API gateway.

pub mod audio;
pub mod chat;
pub mod health;
pub mod components;
pub mod control;
pub mod embeddings;
pub mod ingest;
mod ingest_model;
pub mod mcp;
pub mod models;
pub mod permission;
pub mod plugins;
pub mod session_tools;
pub mod skills;
