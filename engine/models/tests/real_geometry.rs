//! On-demand: print the geometry the prober reads from the real installed GGUF files.
//! Run: cargo test -p cluaiz-models --test real_geometry -- --ignored --nocapture

use cluaiz_core::KvCacheQuantization;

#[test]
#[ignore]
fn print_real_geometry() {
    let gib = |b: u64| b as f64 / (1u64 << 30) as f64;
    let root = cluaiz_core::EnvironmentManager::current().models_dir().join("chat");
    for dir in std::fs::read_dir(&root).into_iter().flatten().flatten() {
        for f in std::fs::read_dir(dir.path()).into_iter().flatten().flatten() {
            let p = f.path();
            let n = p.file_name().unwrap().to_string_lossy().to_lowercase();
            if !n.ends_with(".gguf") || n.starts_with("mmproj") || n.starts_with("mtp-") { continue; }
            let g = cluaiz_models::GgufProber::probe_geometry(&p).unwrap();
            let w = &g.weights;
            println!(
                "{} | {} layers, kv heads {:?}.., K/V {}/{} (swa {}/{}), window {:?} on {} layers, shared {} | vocab {} \
                 | file {:.2} tensors {:.2} | input(CPU) {:.2} output {:.2} tied {:.2} experts {:.2} | full GPU {:.2} \
                 | KV@4096 F16 {:.3} Q8 {:.3}",
                n, g.block_count, &g.head_count_kv_per_layer[..g.head_count_kv_per_layer.len().min(6)],
                g.key_length, g.value_length, g.key_length_swa, g.value_length_swa,
                g.sliding_window, g.swa_layers.iter().filter(|s| **s).count(), g.shared_kv_layers, g.vocab_size,
                gib(g.file_size_bytes), gib(w.total), gib(w.input_cpu), gib(w.output), gib(w.tied_output_copy),
                gib(w.experts_total()), gib(w.full_gpu()),
                gib(g.kv_cache_bytes(4096, KvCacheQuantization::Kv16, 512)),
                gib(g.kv_cache_bytes(4096, KvCacheQuantization::Kv8, 512)),
            );
            // Tensor bytes must account for the file (header is small)
            assert!(w.total <= g.file_size_bytes && w.total + (64 << 20) >= g.file_size_bytes, "{}", n);
        }
    }
}
