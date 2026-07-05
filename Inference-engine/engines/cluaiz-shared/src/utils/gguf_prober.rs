use std::collections::HashMap;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use anyhow::{Result, bail};

/// 🔬 GGUF Prober: Extracts architectural truth directly from binary headers.
/// Zero-dependency implementation to keep the Sovereign Core lightweight.
pub struct GGUFProber;

impl GGUFProber {
    pub fn probe(path: &std::path::Path) -> Result<(HashMap<String, String>, HashMap<String, Vec<usize>>, usize)> {
        let f = File::open(path)?;
        let mut file = std::io::BufReader::with_capacity(1024 * 1024, f);
        
        // 1. Magic Check (GGUF)
        let mut magic = [0u8; 4];
        file.read_exact(&mut magic)?;
        if &magic != b"GGUF" { bail!("Not a valid GGUF binary"); }

        // 2. Version Check
        let mut version = [0u8; 4];
        file.read_exact(&mut version)?;
        let _version = u32::from_le_bytes(version);

        // 3. Header Counts
        let mut counts = [0u8; 16]; // tensor_count (u64) + metadata_kv_count (u64)
        file.read_exact(&mut counts)?;
        let tensor_count = u64::from_le_bytes(counts[0..8].try_into().unwrap());
        let metadata_kv_count = u64::from_le_bytes(counts[8..16].try_into().unwrap());

        let mut metadata = HashMap::new();
        let mut tensor_infos = HashMap::new();

        // 4. Extract Metadata KVs
        for _ in 0..metadata_kv_count {
            let key_res = Self::read_string(&mut file);
            if key_res.is_err() { break; }
            let key = key_res.unwrap();
            
            let vtype_res = Self::read_u32(&mut file);
            if vtype_res.is_err() { break; }
            let value_type = vtype_res.unwrap();
            
            let val_res = Self::read_value(&mut file, value_type);
            if val_res.is_err() { break; }
            let value = val_res.unwrap();
            
            metadata.insert(key, value);
        }

        // 5. Extract Tensor Infos (Partial for Dimension Truth)
        for _ in 0..tensor_count {
            let name_res = Self::read_string(&mut file);
            if name_res.is_err() { break; } // Graceful fallback if partial file ends here
            let name = name_res.unwrap();
            
            let n_dims_res = Self::read_u32(&mut file);
            if n_dims_res.is_err() { break; }
            let n_dims = n_dims_res.unwrap();
            
            let mut dims = Vec::new();
            let mut failed = false;
            for _ in 0..n_dims {
                match Self::read_u64(&mut file) {
                    Ok(d) => dims.push(d as usize),
                    Err(_) => { failed = true; break; }
                }
            }
            if failed { break; }
            
            if Self::read_u32(&mut file).is_err() { break; }
            if Self::read_u64(&mut file).is_err() { break; }
            
            tensor_infos.insert(name, dims);
        }

        Ok((metadata, tensor_infos, tensor_count as usize))
    }

    fn read_string(file: &mut std::io::BufReader<File>) -> Result<String> {
        let len = Self::read_u64(file)?;
        let mut buf = vec![0u8; len as usize];
        file.read_exact(&mut buf)?;
        Ok(String::from_utf8_lossy(&buf).to_string())
    }

    fn read_u32(file: &mut std::io::BufReader<File>) -> Result<u32> {
        let mut buf = [0u8; 4];
        file.read_exact(&mut buf)?;
        Ok(u32::from_le_bytes(buf))
    }

    fn read_u64(file: &mut std::io::BufReader<File>) -> Result<u64> {
        let mut buf = [0u8; 8];
        file.read_exact(&mut buf)?;
        Ok(u64::from_le_bytes(buf))
    }

    fn read_value(file: &mut std::io::BufReader<File>, value_type: u32) -> Result<String> {
        match value_type {
            0 | 1 | 7 => { // UINT8, INT8, BOOL
                let mut buf = [0u8; 1];
                file.read_exact(&mut buf)?;
                if value_type == 7 {
                    Ok(if buf[0] == 0 { "false".into() } else { "true".into() })
                } else {
                    Ok(format!("{}", buf[0]))
                }
            }
            2 | 3 => { // UINT16, INT16
                let mut buf = [0u8; 2];
                file.read_exact(&mut buf)?;
                Ok(format!("{}", u16::from_le_bytes(buf)))
            }
            4 | 5 | 6 => { // UINT32, INT32, FLOAT32
                let mut buf = [0u8; 4];
                file.read_exact(&mut buf)?;
                if value_type == 6 {
                    Ok(format!("{}", f32::from_le_bytes(buf)))
                } else {
                    Ok(format!("{}", u32::from_le_bytes(buf)))
                }
            }
            10 | 11 | 12 => { // UINT64, INT64, FLOAT64
                let mut buf = [0u8; 8];
                file.read_exact(&mut buf)?;
                if value_type == 12 {
                    Ok(format!("{}", f64::from_le_bytes(buf)))
                } else {
                    Ok(format!("{}", u64::from_le_bytes(buf)))
                }
            }
            8 => { // STRING
                Self::read_string(file)
            }
            9 => { // ARRAY
                let item_type = Self::read_u32(file)?;
                let len = Self::read_u64(file)?;
                
                // Safety check to prevent massive allocations
                if len > 1_000_000 {
                    bail!("Array length too large: {}", len);
                }

                // If it's a primitive array, we can just skip it quickly if we don't need the elements
                // But wait, the metadata might contain important arrays (like tokenizer tokens!)
                // Let's parse string arrays properly, and just record length for others.
                if item_type == 8 {
                    let mut elements = Vec::new();
                    // Limit reading to prevent massive logs
                    let read_len = std::cmp::min(len, 10); 
                    for _ in 0..read_len {
                        elements.push(Self::read_string(file)?);
                    }
                    // Skip the rest
                    for _ in read_len..len {
                        let str_len = Self::read_u64(file)?;
                        file.seek(SeekFrom::Current(str_len as i64))?;
                    }
                    Ok(format!("[StringArray: len={}, first_few={:?}]", len, elements))
                } else {
                    // Primitive skips
                    let size_per_item = match item_type {
                        0 | 1 | 7 => 1,
                        2 | 3 => 2,
                        4 | 5 | 6 => 4,
                        10 | 11 | 12 => 8,
                        _ => 0,
                    };
                    if size_per_item > 0 {
                        file.seek(SeekFrom::Current((size_per_item as u64 * len) as i64))?;
                    } else {
                        // Unknown array type
                        bail!("Unsupported array item type: {}", item_type);
                    }
                    Ok(format!("[PrimitiveArray: len={}, type={}]", len, item_type))
                }
            }
            _ => bail!("Unknown GGUF value type: {}", value_type),
        }
    }

    fn read_u8(file: &mut std::io::BufReader<File>) -> Result<u8> {
        let mut buf = [0u8; 1];
        file.read_exact(&mut buf)?;
        Ok(buf[0])
    }

    /// ⚡ Checks if the model has Native MTP (Multi-Token Prediction) support
    pub fn check_native_mtp(tensor_infos: &HashMap<String, Vec<usize>>) -> bool {
        // Universal Structural Check: Looks for generic MTP tensor patterns across any model
        tensor_infos.keys().any(|k| k.contains(".mtp") || k.ends_with("mtp"))
    }

    /// ⚡ Checks if the model has recurrent/SSM (State Space Model) layers.
    /// Handles pure SSM (Mamba, RWKV, Falcon-Mamba) AND hybrid models (Qwen3.5 GDN, Jamba).
    /// Hybrid detection uses GGUF metadata `*.attention.layer_types` key, which llama.cpp
    /// uses to declare mixed attention+recurrent layer topologies.
    pub fn check_recurrent_ssm(
        metadata: &HashMap<String, String>,
        tensor_infos: &HashMap<String, Vec<usize>>,
    ) -> bool {
        // ── 1. Architecture-level: pure SSM architectures ───────────────────
        if let Some(arch) = metadata.get("general.architecture") {
            let arch_lower = arch.to_lowercase();
            let recurrent_archs = ["mamba", "rwkv", "ssm", "falcon_mamba", "jamba", "zamba"];
            if recurrent_archs.iter().any(|a| arch_lower.contains(a)) {
                eprintln!("⚖️ [GGUFProber] Architecture '{}' is a known SSM architecture.", arch);
                return true;
            }
        }

        // ── 2. Hybrid layer-type metadata: Qwen3.5 GDN, and any future hybrid ─
        // These metadata keys signal mixed attention+recurrent topologies.
        let hybrid_meta_signals = [
            "layer_types",       // qwen3.attention.layer_types → GDN/hybrid flag
            "ssm_state_size",    // explicit SSM state dim
            "d_state",           // Mamba-family state dim key
            "conv_kernel",       // 1-D conv = SSM family
            "time_mix_extra_dim",// RWKV-7 key
        ];
        if metadata.keys().any(|k| hybrid_meta_signals.iter().any(|sig| k.contains(sig))) {
            eprintln!("⚖️ [GGUFProber] Hybrid SSM/recurrent layer metadata detected.");
            return true;
        }

        // ── 3. Tensor name patterns: fallback for non-standard GGUF metadata ──
        let ssm_tensor_patterns = [
            ".ssm", "ssm_", ".conv_1d", ".a_log",
            ".dt_", "time_mix", ".mamba", "rwkv_",
        ];
        if tensor_infos.keys().any(|k| ssm_tensor_patterns.iter().any(|p| k.contains(p))) {
            eprintln!("⚖️ [GGUFProber] SSM tensor patterns found in model weights.");
            return true;
        }

        false
    }
}

