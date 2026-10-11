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

/// One tensor entry from the GGUF tensor table.
#[derive(Debug, Clone)]
pub struct GgufTensor {
    pub name: String,
    pub dims: Vec<usize>,
    pub ggml_type: u32,
    /// Offset inside the tensor data section
    pub offset: u64,
    /// Bytes on disk: distance to the next tensor (or to the end of file for the last one)
    pub size_bytes: u64,
}

/// Everything the GGUF header says, read by the single parser below.
#[derive(Debug, Clone, Default)]
pub struct GgufHeader {
    /// Scalars and strings as text; arrays as a short "[...Array: len=N]" summary
    pub metadata: HashMap<String, String>,
    /// Integer / bool arrays up to MAX_STORED_ARRAY_LEN items (e.g. per-layer head_count_kv, sliding_window_pattern)
    pub int_arrays: HashMap<String, Vec<i64>>,
    pub tensors: Vec<GgufTensor>,
}

/// Per-layer arrays are tiny; vocab-sized arrays (token types, scores) are skipped, not stored.
const MAX_STORED_ARRAY_LEN: u64 = 4096;
/// GGUF default alignment of the tensor data section when `general.alignment` is absent (GGUF spec).
const GGUF_DEFAULT_ALIGNMENT: u64 = 32;

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

        let (think_start_tag, think_end_tag) = (None, None);

        Ok(GgufProbeResult {
            architecture,
            context_window,
            chat_template,
            quantization,
            parameter_count,
            think_start_tag,
            think_end_tag,
            is_embedding,
            has_vision,
        })
    }

    /// The single GGUF header parser: metadata, integer arrays, and the tensor table with byte sizes.
    pub fn read_header(path: &Path) -> Result<GgufHeader> {
        let file_len = std::fs::metadata(path)?.len();
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

        let mut header = GgufHeader::default();

        for _ in 0..metadata_kv_count {
            let key = Self::read_string(&mut file)?;
            let value_type = Self::read_u32(&mut file)?;
            let (value, array) = Self::read_value(&mut file, value_type)?;
            if let Some(values) = array {
                header.int_arrays.insert(key.clone(), values);
            }
            header.metadata.insert(key, value);
        }

        for _ in 0..tensor_count {
            let name = Self::read_string(&mut file)?;
            let n_dims = Self::read_u32(&mut file)?;
            let mut dims = Vec::with_capacity(n_dims as usize);
            for _ in 0..n_dims {
                dims.push(Self::read_u64(&mut file)? as usize);
            }
            let ggml_type = Self::read_u32(&mut file)?;
            let offset = Self::read_u64(&mut file)?;
            header.tensors.push(GgufTensor { name, dims, ggml_type, offset, size_bytes: 0 });
        }

        // Tensor data starts at the next aligned position after the tensor table
        let alignment = header.metadata.get("general.alignment")
            .and_then(|v| v.parse::<u64>().ok())
            .filter(|a| *a > 0)
            .unwrap_or(GGUF_DEFAULT_ALIGNMENT);
        let table_end = file.stream_position()?;
        let data_start = table_end.div_ceil(alignment) * alignment;
        let data_len = file_len.saturating_sub(data_start);

        let mut order: Vec<usize> = (0..header.tensors.len()).collect();
        order.sort_by_key(|&i| header.tensors[i].offset);
        for (k, &i) in order.iter().enumerate() {
            let end = order.get(k + 1).map(|&j| header.tensors[j].offset).unwrap_or(data_len);
            header.tensors[i].size_bytes = end.saturating_sub(header.tensors[i].offset);
        }

        Ok(header)
    }

    /// Metadata map, tensor dims and tensor count (thin view over `read_header`).
    pub fn probe(path: &Path) -> Result<(HashMap<String, String>, HashMap<String, Vec<usize>>, usize)> {
        let header = Self::read_header(path)?;
        let count = header.tensors.len();
        let dims = header.tensors.into_iter().map(|t| (t.name, t.dims)).collect();
        Ok((header.metadata, dims, count))
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

    /// Reads one metadata value. Integer/bool arrays up to MAX_STORED_ARRAY_LEN are also returned as numbers.
    fn read_value(file: &mut std::io::BufReader<File>, value_type: u32) -> Result<(String, Option<Vec<i64>>)> {
        if value_type == 9 {
            return Self::read_array(file);
        }
        Ok((Self::read_scalar(file, value_type)?, None))
    }

    fn read_scalar(file: &mut std::io::BufReader<File>, value_type: u32) -> Result<String> {
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
            _ => bail!("Unknown GGUF value type: {}", value_type),
        }
    }

    fn read_array(file: &mut std::io::BufReader<File>) -> Result<(String, Option<Vec<i64>>)> {
        let item_type = Self::read_u32(file)?;
        let len = Self::read_u64(file)?;
        match item_type {
            8 => {
                // String array (vocab, merges): read past it, keep a short summary
                let mut first = Vec::new();
                for i in 0..len {
                    let s = Self::read_string(file)?;
                    if i < 10 { first.push(s); }
                }
                Ok((format!("[StringArray: len={}, first_few={:?}]", len, first), None))
            }
            0 | 1 | 2 | 3 | 4 | 5 | 7 | 10 | 11 => {
                let summary = format!("[PrimitiveArray: len={}, type={}]", len, item_type);
                if len > MAX_STORED_ARRAY_LEN {
                    file.seek(SeekFrom::Current((Self::scalar_size(item_type) * len) as i64))?;
                    return Ok((summary, None));
                }
                let mut values = Vec::with_capacity(len as usize);
                for _ in 0..len {
                    let v = match Self::read_scalar(file, item_type)?.as_str() {
                        "true" => 1,
                        "false" => 0,
                        t => t.parse::<i64>().unwrap_or(0),
                    };
                    values.push(v);
                }
                Ok((summary, Some(values)))
            }
            6 | 12 => {
                // Float arrays are not used for placement; skip them
                file.seek(SeekFrom::Current((Self::scalar_size(item_type) * len) as i64))?;
                Ok((format!("[PrimitiveArray: len={}, type={}]", len, item_type), None))
            }
            _ => bail!("Unsupported GGUF array item type: {}", item_type),
        }
    }

    /// Byte size of one GGUF scalar of the given type (GGUF spec).
    fn scalar_size(value_type: u32) -> u64 {
        match value_type {
            0 | 1 | 7 => 1,
            2 | 3 => 2,
            4 | 5 | 6 => 4,
            10 | 11 | 12 => 8,
            _ => 0,
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

    /// Reads model geometry from the GGUF header. Missing required keys are an error, never a guess.
    /// Optional keys fall back to the same defaults llama.cpp uses (llama-model.cpp load_hparams).
    pub fn probe_geometry(path: &Path) -> Result<crate::gguf::ModelGeometry> {
        let header = Self::read_header(path)?;
        let meta = &header.metadata;
        let architecture = meta.get("general.architecture").cloned()
            .ok_or_else(|| anyhow::anyhow!("GGUF has no general.architecture: {}", path.display()))?;
        let key = |k: &str| format!("{}.{}", architecture, k);
        let get_u32 = |k: &str| meta.get(&key(k)).and_then(|v| v.parse::<u32>().ok());
        let required = |k: &str| get_u32(k)
            .ok_or_else(|| anyhow::anyhow!("GGUF is missing {} ({})", key(k), path.display()));

        let block_count = required("block_count")?;
        let context_length = required("context_length")?;
        let embedding_length = required("embedding_length")?;
        let head_count = required("attention.head_count")?;

        // head_count_kv is a scalar or a per-layer array; absent means no GQA (llama.cpp: n_head_kv = n_head)
        let head_count_kv_per_layer: Vec<u32> = match header.int_arrays.get(&key("attention.head_count_kv")) {
            Some(arr) => arr.iter().map(|v| *v as u32).collect(),
            None => vec![get_u32("attention.head_count_kv").unwrap_or(head_count); block_count as usize],
        };
        let head_count_kv = head_count_kv_per_layer.first().copied().unwrap_or(head_count);

        // llama.cpp default head size: n_embd / n_head
        let default_head = if head_count > 0 { embedding_length / head_count } else { 0 };
        let key_length = get_u32("attention.key_length").unwrap_or(default_head);
        let value_length = get_u32("attention.value_length").unwrap_or(default_head);
        let key_length_swa = get_u32("attention.key_length_swa").unwrap_or(key_length);
        let value_length_swa = get_u32("attention.value_length_swa").unwrap_or(value_length);

        // Sliding window layers are only marked when the file lists them; otherwise every layer is
        // counted as full attention (an over-estimate of KV, never an under-estimate)
        let sliding_window = get_u32("attention.sliding_window").filter(|w| *w > 0);
        let swa_layers: Vec<bool> = header.int_arrays.get(&key("attention.sliding_window_pattern"))
            .map(|arr| arr.iter().map(|v| *v != 0).collect())
            .unwrap_or_default();
        let shared_kv_layers = get_u32("attention.shared_kv_layers").unwrap_or(0);

        let mut weights = crate::gguf::WeightBytes {
            layer: vec![0; block_count as usize],
            layer_experts: vec![0; block_count as usize],
            ..Default::default()
        };
        let mut token_embd_bytes = 0u64;
        let mut has_output_weight = false;
        let mut vocab_size = 0u32;
        for t in &header.tensors {
            weights.total += t.size_bytes;
            let block = t.name.strip_prefix("blk.")
                .and_then(|rest| rest.split('.').next())
                .and_then(|n| n.parse::<usize>().ok());
            match block {
                Some(b) if b < weights.layer.len() => {
                    if Self::is_routed_expert(&t.name) {
                        weights.layer_experts[b] += t.size_bytes;
                    } else {
                        weights.layer[b] += t.size_bytes;
                    }
                }
                _ if Self::is_input_embedding(&t.name) => {
                    weights.input_cpu += t.size_bytes;
                    if t.name == "token_embd.weight" {
                        token_embd_bytes = t.size_bytes;
                        // shape is [n_embd, n_vocab]
                        vocab_size = t.dims.get(1).copied().unwrap_or(0) as u32;
                    }
                }
                _ => {
                    if t.name == "output.weight" {
                        has_output_weight = true;
                    }
                    weights.output += t.size_bytes;
                }
            }
        }
        if !has_output_weight {
            weights.tied_output_copy = token_embd_bytes;
        }

        Ok(crate::gguf::ModelGeometry {
            architecture: architecture.clone(),
            context_length,
            embedding_length,
            block_count,
            head_count,
            head_count_kv,
            head_count_kv_per_layer,
            key_length,
            value_length,
            key_length_swa,
            value_length_swa,
            sliding_window,
            swa_layers,
            shared_kv_layers,
            expert_count: get_u32("expert_count"),
            expert_used_count: get_u32("expert_used_count"),
            file_size_bytes: std::fs::metadata(path)?.len(),
            vocab_size,
            weights,
        })
    }

    /// Routed-expert tensors, same match as llama.cpp `LLM_FFN_EXPS_REGEX` (`.ffn_(up|down|gate|gate_up)_(ch|)exps`).
    pub fn is_routed_expert(name: &str) -> bool {
        name.contains(".ffn_") && (name.contains("_exps") || name.contains("_chexps"))
    }

    /// Input-layer embedding tensors, which llama.cpp keeps on CPU regardless of n_gpu_layers.
    fn is_input_embedding(name: &str) -> bool {
        name.starts_with("token_embd.") || name.starts_with("per_layer_token_embd.")
    }
}
