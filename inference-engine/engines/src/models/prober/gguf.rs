//! ═══════════════════════════════════════════════════════════════════════
//!   Prober: GGUF Header & KV Metadata Probing (SSOT Binding)
//! ═══════════════════════════════════════════════════════════════════════

use std::collections::HashMap;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;
use anyhow::{Result, bail};

#[derive(Debug, Clone, Default)]
pub struct GgufProbeResult {
    pub architecture: Option<String>,
    pub context_window: Option<String>,
    pub chat_template: Option<String>,
    pub quantization: Option<String>,
    pub parameter_count: Option<String>,
    pub think_start_tag: Option<String>,
    pub think_end_tag: Option<String>,
    pub is_embedding: bool,
    pub has_vision: bool,
}

pub struct GgufProber;

impl GgufProber {
    /// Probes a GGUF file and extracts architectural metadata
    pub fn probe_file(path: &Path) -> Result<GgufProbeResult, String> {
        let (metadata, tensor_infos, _tensor_count) = Self::probe(path)
            .map_err(|e| e.to_string())?;

        let architecture = metadata.get("general.architecture").cloned();
        let arch_prefix = architecture.as_deref().unwrap_or("llama");
        
        let context_window = metadata
            .get(&format!("{}.context_length", arch_prefix))
            .or_else(|| metadata.get("general.context_length"))
            .cloned();

        let chat_template = metadata
            .get("tokenizer.chat_template")
            .cloned();

        let quantization = metadata
            .get("general.file_type")
            .cloned();

        let parameter_count = metadata
            .get("general.parameter_count")
            .cloned();

        let has_vision = tensor_infos.keys().any(|k| {
            let kl = k.to_lowercase();
            kl.contains("v.") || kl.contains("vision") || kl.contains("mm.") || kl.contains("image")
        });

        let is_embedding = metadata.get("general.architecture").map(|a| a == "bert" || a == "nomic_bert" || a == "jina_bert").unwrap_or(false)
            || tensor_infos.keys().any(|k| k.to_lowercase().contains("pooling"));

        Ok(GgufProbeResult {
            architecture,
            context_window,
            chat_template,
            quantization,
            parameter_count,
            think_start_tag: None,
            think_end_tag: None,
            is_embedding,
            has_vision,
        })
    }

    /// Low-level zero-dependency binary header extractor
    pub fn probe(path: &Path) -> Result<(HashMap<String, String>, HashMap<String, Vec<usize>>, usize)> {
        let f = File::open(path)?;
        let mut file = std::io::BufReader::with_capacity(1024 * 1024, f);
        
        let mut magic = [0u8; 4];
        file.read_exact(&mut magic)?;
        if &magic != b"GGUF" { bail!("Not a valid GGUF binary"); }

        let mut version = [0u8; 4];
        file.read_exact(&mut version)?;

        let mut counts = [0u8; 16];
        file.read_exact(&mut counts)?;
        let tensor_count = u64::from_le_bytes(counts[0..8].try_into().unwrap());
        let metadata_kv_count = u64::from_le_bytes(counts[8..16].try_into().unwrap());

        let mut metadata = HashMap::new();
        let mut tensor_infos = HashMap::new();

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

        for _ in 0..tensor_count {
            let name_res = Self::read_string(&mut file);
            if name_res.is_err() { break; }
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
            0 | 1 | 7 => {
                let mut buf = [0u8; 1];
                file.read_exact(&mut buf)?;
                if value_type == 7 {
                    Ok(if buf[0] == 0 { "false".into() } else { "true".into() })
                } else {
                    Ok(format!("{}", buf[0]))
                }
            }
            2 | 3 => {
                let mut buf = [0u8; 2];
                file.read_exact(&mut buf)?;
                Ok(format!("{}", u16::from_le_bytes(buf)))
            }
            4 | 5 | 6 => {
                let mut buf = [0u8; 4];
                file.read_exact(&mut buf)?;
                if value_type == 6 {
                    Ok(format!("{}", f32::from_le_bytes(buf)))
                } else {
                    Ok(format!("{}", u32::from_le_bytes(buf)))
                }
            }
            10 | 11 | 12 => {
                let mut buf = [0u8; 8];
                file.read_exact(&mut buf)?;
                if value_type == 12 {
                    Ok(format!("{}", f64::from_le_bytes(buf)))
                } else {
                    Ok(format!("{}", u64::from_le_bytes(buf)))
                }
            }
            8 => Self::read_string(file),
            9 => {
                let item_type = Self::read_u32(file)?;
                let len = Self::read_u64(file)?;
                if len > 1_000_000 { bail!("Array length too large: {}", len); }
                if item_type == 8 {
                    let mut elements = Vec::new();
                    let read_len = std::cmp::min(len, 10);
                    for _ in 0..read_len { elements.push(Self::read_string(file)?); }
                    let mut scratch = vec![0u8; 1024];
                    for _ in read_len..len {
                        let str_len = Self::read_u64(file)? as usize;
                        if str_len > scratch.len() { scratch.resize(str_len, 0); }
                        file.read_exact(&mut scratch[0..str_len])?;
                    }
                    Ok(format!("[StringArray: len={}, first_few={:?}]", len, elements))
                } else {
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
                        bail!("Unsupported array item type: {}", item_type);
                    }
                    Ok(format!("[PrimitiveArray: len={}, type={}]", len, item_type))
                }
            }
            _ => bail!("Unknown GGUF value type: {}", value_type),
        }
    }

    /// ⚡ Checks if the model has Native MTP (Multi-Token Prediction) support
    pub fn check_native_mtp(tensor_infos: &HashMap<String, Vec<usize>>) -> bool {
        tensor_infos.keys().any(|k| k.contains(".mtp") || k.ends_with("mtp"))
    }

    /// ⚡ Checks if the model has recurrent/SSM (State Space Model) layers.
    pub fn check_recurrent_ssm(
        metadata: &HashMap<String, String>,
        tensor_infos: &HashMap<String, Vec<usize>>,
    ) -> bool {
        if let Some(arch) = metadata.get("general.architecture") {
            let arch_lower = arch.to_lowercase();
            let recurrent_archs = ["mamba", "rwkv", "ssm", "falcon_mamba", "jamba", "zamba"];
            if recurrent_archs.iter().any(|a| arch_lower.contains(a)) {
                return true;
            }
        }

        let hybrid_meta_signals = [
            "layer_types", "ssm_state_size", "d_state", "conv_kernel", "time_mix_extra_dim",
        ];
        if metadata.keys().any(|k| hybrid_meta_signals.iter().any(|sig| k.contains(sig))) {
            return true;
        }

        let ssm_tensor_patterns = [
            ".ssm", "ssm_", ".conv_1d", ".a_log", ".dt_", "time_mix", ".mamba", "rwkv_",
        ];
        if tensor_infos.keys().any(|k| ssm_tensor_patterns.iter().any(|p| k.contains(p))) {
            return true;
        }

        false
    }
}
