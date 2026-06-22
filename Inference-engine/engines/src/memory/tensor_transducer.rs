//! ⚡ Tensor Transducer: Zero-Latency FFI Bridge to cluaizd LMDB Database
//! Maps raw binary memory from the database directly into the AI Engine's context.

use cluaize_shared::hardware::governor::HardwareGovernor;
use engine_lmdb::env::LmdbEnv;
use engine_lmdb::ffi::{
    cluaizd_ffi_execute_parameterized, cluaizd_ffi_free_neuron_payload, cluaizd_ffi_read_neuron,
    CluaizdFfiNeuron,
};
use std::ffi::c_void;
use std::sync::OnceLock;

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

pub static GLOBAL_LMDB_SHARDS: OnceLock<Vec<LmdbEnv>> = OnceLock::new();

pub struct TensorTransducer;

impl TensorTransducer {
    pub fn boot_environment() {
        let base_path = cluaize_shared::environment::EnvironmentManager::current()
            .ensure_cluaizd_dir()
            .unwrap_or_else(|_| {
                cluaize_shared::environment::EnvironmentManager::current().cluaizd_dir()
            });

        let mut shards = Vec::new();
        let num_shards = 4; // 4 Physical Shards

        for i in 0..num_shards {
            let shard_path = base_path.join(format!("shard_{}", i));
            if !shard_path.exists() {
                let _ = std::fs::create_dir_all(&shard_path);
            }
            // Open with 256MB capacity per shard
            match LmdbEnv::open(&shard_path, 256 * 1024 * 1024) {
                Ok(env) => {
                    shards.push(env);
                    tracing::info!("🧠 LMDB Shard {} booted at {:?}", i, shard_path);
                }
                Err(e) => {
                    tracing::error!("Failed to boot LMDB Shard {}: {:?}", i, e);
                }
            }
        }

        if shards.len() == num_shards {
            let _ = GLOBAL_LMDB_SHARDS.set(shards);
            tracing::info!(
                "🧠 All {} LMDB Shards fully booted for Zero-Latency FFI",
                num_shards
            );
        }
    }
    /// 🧠 Direct Brain Injection: Pulls a Neuron via FFI bypassing tokenization text overhead.
    /// Returns the raw payload bytes directly from LMDB.
    pub fn inject_context(memory_key: &str) -> Option<Vec<u8>> {
        // 1. Check if the Brain is enabled by the Sovereign Governor
        if let Ok(control) = HardwareGovernor::load_system_control() {
            if !control.brain.is_enabled() {
                // FFI Database is turned OFF. Fallback to legacy loader.
                tracing::debug!("Cluaizd FFI Brain is disabled. Falling back.");
                return None;
            }
        } else {
            return None;
        }

        // 2. Transduce: Call the Database FFI via Deterministic Shard Routing
        let shards = GLOBAL_LMDB_SHARDS.get()?;
        let mut hasher = DefaultHasher::new();
        memory_key.hash(&mut hasher);
        let shard_idx = (hasher.finish() as usize) % shards.len();

        let env = &shards[shard_idx];
        let env_ptr = env as *const LmdbEnv as *mut c_void;

        // Generate a 16-byte UUID from the memory_key string (simple MD5-like truncation for now)
        let mut id_array = [0u8; 16];
        let key_bytes = memory_key.as_bytes();
        for (i, &b) in key_bytes.iter().take(16).enumerate() {
            id_array[i] = b;
        }

        let mut out_neuron = CluaizdFfiNeuron {
            id: [0; 16],
            vector: [0.0; 16],
            state_hash: [0; 32],
            payload_ptr: std::ptr::null(),
            payload_len: 0,
        };

        let result =
            unsafe { cluaizd_ffi_read_neuron(env_ptr, id_array.as_ptr(), &mut out_neuron) };

        if result != 0 || out_neuron.payload_ptr.is_null() {
            tracing::debug!(
                "FFI Brain lookup failed or not initialized for key: {}",
                memory_key
            );
            return None;
        }

        // 3. Zero-Copy extraction into Engine Context
        let payload = unsafe {
            // Reconstruct the slice directly from the LMDB memory map pointer
            let slice = std::slice::from_raw_parts(out_neuron.payload_ptr, out_neuron.payload_len);

            // Clone into a Vec for the Engine to own
            let vec_data = slice.to_vec();

            // Free the leaked payload from the FFI
            cluaizd_ffi_free_neuron_payload(
                out_neuron.payload_ptr as *mut u8,
                out_neuron.payload_len,
                out_neuron.payload_len,
            );

            vec_data
        };

        tracing::info!(
            "🧠 Successfully injected Neural Context via Zero-Latency FFI: {} bytes",
            payload.len()
        );
        Some(payload)
    }

    /// ⚡ Direct Brain Write: Saves a Memory/Skill Vector directly to LMDB via Parameterized FFI.
    /// This bypasses string parsing for the heavy 64-byte vector array.
    pub fn save_context(memory_id: &str, payload: &str, vector: [f32; 16]) -> Result<(), String> {
        // 1. Check if the Brain is enabled
        if let Ok(control) = HardwareGovernor::load_system_control() {
            if !control.brain.is_enabled() {
                tracing::debug!("Cluaizd FFI Brain is disabled. Skipping save.");
                return Ok(());
            }
        } else {
            return Err("Failed to load system control".to_string());
        }

        let shards = GLOBAL_LMDB_SHARDS.get().ok_or("LMDB Shards not booted")?;
        let mut hasher = DefaultHasher::new();
        memory_id.hash(&mut hasher);
        let shard_idx = (hasher.finish() as usize) % shards.len();

        let env = &shards[shard_idx];
        let env_ptr = env as *const LmdbEnv as *mut c_void;

        // Create the parameterised CDQL Query Shell
        let query = format!(
            "insert into Context(id: \"{}\", payload: \"{}\", vector: ?)\0",
            memory_id, payload
        );

        // Map vector to raw bytes
        let vector_ptr = vector.as_ptr() as *const u8;
        // 16 floats * 4 bytes = 64 bytes length
        let vector_len = 64;

        let result = unsafe {
            cluaizd_ffi_execute_parameterized(
                env_ptr,
                query.as_ptr() as *const std::ffi::c_char,
                vector_ptr,
                vector_len,
            )
        };

        if result != 0 {
            return Err(format!(
                "FFI parameterized insertion failed with error code: {}",
                result
            ));
        }

        tracing::info!(
            "🧠 Successfully saved contextual vector to Engine Brain ({}).",
            memory_id
        );
        Ok(())
    }

    /// ⚡ Raw CDQL Execution: Passes a raw query string to the DB and returns the JSON output.
    pub fn execute_raw_cdql(query: &str) -> Result<String, String> {
        let shards = GLOBAL_LMDB_SHARDS.get().ok_or("LMDB Shards not booted")?;

        // For raw queries, we can route it to shard_0 by default,
        // or a specific shard if the CDQL engine handles distributed queries.
        // For now, we pass it to the first shard.
        if shards.is_empty() {
            return Err("No active shards found".to_string());
        }
        let env_ptr = &shards[0] as *const LmdbEnv as *mut c_void;

        let null_terminated_query = format!("{}\0", query);

        let result = unsafe {
            cluaizd_ffi_execute_parameterized(
                env_ptr,
                null_terminated_query.as_ptr() as *const std::ffi::c_char,
                std::ptr::null(),
                0,
            )
        };

        if result != 0 {
            return Err(format!("CDQL execution failed with error code: {}", result));
        }

        tracing::info!("⚡ Successfully executed raw CDQL query");

        // Note: Currently cluaizd_ffi_execute_parameterized returns an int status code.
        // If the DB is expected to return JSON string results for queries like "find Neuron",
        // we will need an FFI function that returns a string/bytes, similar to read_neuron.
        // For now, we return a success message.
        Ok("Query executed successfully".to_string())
    }
}
