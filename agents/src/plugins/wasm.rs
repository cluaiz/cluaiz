//! Runs a WebAssembly plugin (`cluaiz-tools/plugins/*`) inside the Engine.
//!
//! The plugin exports `memory`, `allocate(len) -> ptr`, and `execute_cel(ptr, len) -> u64`, whose
//! result is `(ptr << 32) | len` of its JSON answer. It reaches nothing outside its own memory
//! except the two host functions below: no files, no network, no other programs.

use anyhow::{anyhow, Result};
use std::path::Path;
use std::sync::LazyLock;
use wasmtime::{Config, Engine, Linker, Module, Store, StoreLimits, StoreLimitsBuilder};

/// Most instructions one call may run: a loop that never ends becomes an error
const FUEL: u64 = 2_000_000_000;
/// Memory of one call, when the manifest does not name less
const MAX_MEMORY_BYTES: usize = 256 * 1024 * 1024;

static ENGINE: LazyLock<Result<Engine, String>> = LazyLock::new(|| {
    let mut config = Config::new();
    config.consume_fuel(true);
    Engine::new(&config).map_err(|e| e.to_string())
});

pub fn run_file(path: &Path, payload: &[u8], max_memory_mb: Option<u64>) -> Result<Vec<u8>> {
    run(&std::fs::read(path)?, payload, max_memory_mb)
}

fn run(wasm: &[u8], payload: &[u8], max_memory_mb: Option<u64>) -> Result<Vec<u8>> {
    let engine = ENGINE.as_ref().map_err(|e| anyhow!("WebAssembly is not available: {e}"))?;
    let module = Module::new(engine, wasm).map_err(|e| anyhow!("Not a valid WebAssembly plugin: {e}"))?;
    let memory_limit = max_memory_mb
        .map_or(MAX_MEMORY_BYTES, |mb| (mb as usize).saturating_mul(1024 * 1024).min(MAX_MEMORY_BYTES));
    let limits: StoreLimits = StoreLimitsBuilder::new().memory_size(memory_limit).build();
    let mut store = Store::new(engine, limits);
    store.limiter(|limits| limits);
    store.set_fuel(FUEL).map_err(|e| anyhow!("{e}"))?;

    let mut linker: Linker<StoreLimits> = Linker::new(engine);
    linker
        .func_wrap("cluaiz", "now_utc_ms", || -> i64 {
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |since| since.as_millis() as i64)
        })
        .map_err(|e| anyhow!("{e}"))?;
    linker
        .func_wrap("cluaiz", "os_platform", || -> i32 {
            if cfg!(target_os = "windows") {
                1
            } else if cfg!(target_os = "macos") {
                2
            } else if cfg!(target_os = "linux") {
                3
            } else {
                0
            }
        })
        .map_err(|e| anyhow!("{e}"))?;

    let instance = linker
        .instantiate(&mut store, &module)
        .map_err(|e| anyhow!("The plugin could not start: {e}"))?;
    let allocate = instance
        .get_typed_func::<u32, i32>(&mut store, "allocate")
        .map_err(|e| anyhow!("The plugin has no 'allocate': {e}"))?;
    let execute = instance
        .get_typed_func::<(i32, u32), u64>(&mut store, "execute_cel")
        .map_err(|e| anyhow!("The plugin has no 'execute_cel': {e}"))?;
    let memory = instance
        .get_memory(&mut store, "memory")
        .ok_or_else(|| anyhow!("The plugin has no 'memory'"))?;

    let length = u32::try_from(payload.len()).map_err(|_| anyhow!("The input is too large"))?;
    let input = allocate.call(&mut store, length).map_err(|e| anyhow!("The plugin failed: {e:#}"))?;
    memory
        .write(&mut store, input as u32 as usize, payload)
        .map_err(|e| anyhow!("The plugin gave a bad input address: {e}"))?;
    let result = execute
        .call(&mut store, (input, length))
        .map_err(|e| anyhow!("The plugin failed: {e:#}"))?;

    let (start, length) = ((result >> 32) as usize, (result & 0xFFFF_FFFF) as usize);
    let answer = start
        .checked_add(length)
        .and_then(|end| memory.data(&store).get(start..end))
        .ok_or_else(|| anyhow!("The plugin gave an answer outside its memory"))?;
    Ok(answer.to_vec())
}

#[cfg(test)]
mod tests {
    use super::run;

    /// Echoes its input; `now_utc_ms` is called so the host functions are linked
    const ECHO: &str = r#"(module
        (import "cluaiz" "now_utc_ms" (func $now (result i64)))
        (memory (export "memory") 1)
        (func (export "allocate") (param i32) (result i32) i32.const 1024)
        (func (export "execute_cel") (param $p i32) (param $l i32) (result i64)
            call $now drop
            local.get $p i64.extend_i32_u i64.const 32 i64.shl
            local.get $l i64.extend_i32_u i64.or))"#;

    fn wasm(text: &str) -> Vec<u8> {
        wat::parse_str(text).unwrap()
    }

    #[test]
    fn a_plugin_answers_through_its_memory() {
        assert_eq!(run(&wasm(ECHO), br#"{"a":1}"#, None).unwrap(), br#"{"a":1}"#);
    }

    #[test]
    fn a_loop_that_never_ends_is_stopped() {
        let forever = r#"(module (memory (export "memory") 1)
            (func (export "allocate") (param i32) (result i32) i32.const 0)
            (func (export "execute_cel") (param i32 i32) (result i64) (loop $l (br $l)) i64.const 0))"#;
        assert!(run(&wasm(forever), b"x", None).is_err());
    }

    #[test]
    fn a_plugin_cannot_take_more_memory_than_allowed() {
        // 100 pages (6.4 MB) at the start, with 1 MB allowed
        let big = r#"(module (memory (export "memory") 100)
            (func (export "allocate") (param i32) (result i32) i32.const 0)
            (func (export "execute_cel") (param i32 i32) (result i64) i64.const 0))"#;
        assert!(run(&wasm(big), b"x", Some(1)).is_err());
    }

    #[test]
    fn an_answer_outside_its_memory_is_an_error() {
        let bad = r#"(module (memory (export "memory") 1)
            (func (export "allocate") (param i32) (result i32) i32.const 0)
            (func (export "execute_cel") (param i32 i32) (result i64) i64.const -1))"#;
        assert!(run(&wasm(bad), b"x", None).is_err());
    }
}
