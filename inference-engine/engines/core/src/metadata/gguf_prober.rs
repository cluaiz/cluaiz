use std::collections::HashMap;
use std::fs::File;
use std::io::{BufReader, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

static ARCH_CACHE: OnceLock<Mutex<HashMap<PathBuf, GgufArchitectureInfo>>> = OnceLock::new();
static EXPERT_TENSOR_CACHE: OnceLock<
    Mutex<HashMap<PathBuf, (usize, HashMap<(usize, String), (u64, u64)>)>>,
> = OnceLock::new();

/// Ground-truth architectural and tensor sizing extracted directly from GGUF binary headers.
#[derive(Debug, Clone, Default)]
pub struct GgufArchitectureInfo {
    pub architecture: String,
    pub layer_count: usize,
    pub head_count: usize,
    pub head_count_kv: usize,
    pub head_dim: usize,
    pub context_length: usize,
    pub is_moe: bool,
    pub expert_count: usize,
    pub expert_used_count: usize,
    pub attention_bytes_per_layer: u64,
    pub expert_bytes_per_layer: u64,
    pub dense_ffn_bytes_per_layer: u64,
    pub base_reserve_bytes: u64,
    pub total_dense_bytes: u64,
    pub total_expert_bytes: u64,
    pub file_size_bytes: u64,
}

impl GgufArchitectureInfo {
    /// Dynamically computes the exact KV cache bytes required per token.
    /// Formula: 2 * n_layers * n_head_kv * head_dim * element_bytes
    pub fn kv_bytes_per_token(&self, element_bytes: f64) -> f64 {
        let layers = self.layer_count.max(1) as f64;
        let kv_heads = self.head_count_kv.max(1) as f64;
        let dim = self.head_dim.max(64) as f64;
        2.0 * layers * kv_heads * dim * element_bytes
    }

    /// Single layer footprint on GPU (Dense Attention + Dense Norm/Router + MoE Experts in that layer)
    pub fn layer_footprint_gb(&self) -> f64 {
        let per_layer_bytes = self.attention_bytes_per_layer
            + self.dense_ffn_bytes_per_layer
            + self.expert_bytes_per_layer;
        (per_layer_bytes as f64) / (1024.0 * 1024.0 * 1024.0)
    }

    /// Single expert size in GB
    pub fn single_expert_gb(&self) -> f64 {
        if self.expert_count > 0 && self.expert_bytes_per_layer > 0 {
            let per_expert_bytes = self.expert_bytes_per_layer as f64 / self.expert_count as f64;
            per_expert_bytes / (1024.0 * 1024.0 * 1024.0)
        } else {
            0.0
        }
    }
}

pub struct GgufBinaryProber;

impl GgufBinaryProber {
    /// Fast zero-dependency parser that scans GGUF binary metadata and tensor header tables.
    pub fn probe(path: &Path) -> Result<GgufArchitectureInfo, String> {
        let abs_path = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
        let cache = ARCH_CACHE.get_or_init(|| Mutex::new(HashMap::new()));
        if let Ok(guard) = cache.lock() {
            if let Some(cached) = guard.get(&abs_path) {
                return Ok(cached.clone());
            }
        }

        let file_meta = std::fs::metadata(path).map_err(|e| e.to_string())?;
        let file_size_bytes = file_meta.len();

        let f = File::open(path).map_err(|e| e.to_string())?;
        let mut file = BufReader::with_capacity(1024 * 1024, f);

        let mut magic = [0u8; 4];
        file.read_exact(&mut magic).map_err(|e| e.to_string())?;
        if &magic != b"GGUF" {
            return Err("Invalid GGUF binary magic header".to_string());
        }

        let mut version_bytes = [0u8; 4];
        file.read_exact(&mut version_bytes).map_err(|e| e.to_string())?;
        let version = u32::from_le_bytes(version_bytes);
        if version < 2 {
            return Err(format!("Unsupported legacy GGUF version: {}", version));
        }

        let mut counts = [0u8; 16];
        file.read_exact(&mut counts).map_err(|e| e.to_string())?;
        let tensor_count = u64::from_le_bytes(counts[0..8].try_into().unwrap()) as usize;
        let metadata_kv_count = u64::from_le_bytes(counts[8..16].try_into().unwrap()) as usize;

        let mut metadata: HashMap<String, String> = HashMap::new();

        for _ in 0..metadata_kv_count {
            let key = match Self::read_string(&mut file) {
                Ok(k) => k,
                Err(_) => break,
            };
            let vtype = match Self::read_u32(&mut file) {
                Ok(vt) => vt,
                Err(_) => break,
            };
            let val = match Self::read_value(&mut file, vtype) {
                Ok(v) => v,
                Err(_) => break,
            };
            metadata.insert(key, val);
        }

        let arch = metadata
            .get("general.architecture")
            .cloned()
            .unwrap_or_else(|| "llama".to_string());

        let get_u64_field = |keys: &[&str]| -> Option<usize> {
            for k in keys {
                if let Some(v) = metadata.get(*k) {
                    if let Ok(val) = v.parse::<u64>() {
                        return Some(val as usize);
                    }
                }
            }
            None
        };

        let layer_count = get_u64_field(&[
            &format!("{}.block_count", arch),
            "general.block_count",
            "general.layer_count",
        ])
        .unwrap_or(32);

        let head_count = get_u64_field(&[
            &format!("{}.attention.head_count", arch),
            "general.attention.head_count",
        ])
        .unwrap_or(32);

        let head_count_kv = get_u64_field(&[
            &format!("{}.attention.head_count_kv", arch),
            "general.attention.head_count_kv",
        ])
        .unwrap_or(head_count);

        let head_dim = get_u64_field(&[
            &format!("{}.attention.key_length", arch),
            &format!("{}.attention.head_dim", arch),
        ])
        .or_else(|| {
            get_u64_field(&[&format!("{}.embedding_length", arch)]).map(|emb| emb / head_count.max(1))
        })
        .unwrap_or(128);

        let context_length = get_u64_field(&[
            &format!("{}.context_length", arch),
            "general.context_length",
        ])
        .unwrap_or(8192);

        let expert_count = get_u64_field(&[
            &format!("{}.expert_count", arch),
            "general.expert_count",
        ])
        .unwrap_or(0);

        let expert_used_count = get_u64_field(&[
            &format!("{}.expert_used_count", arch),
            "general.expert_used_count",
        ])
        .unwrap_or(0);

        let is_moe = expert_count > 0;

        // Parse tensor information offsets to compute exact tensor byte sizes
        struct RawTensor {
            name: String,
            offset: u64,
        }

        let mut raw_tensors = Vec::with_capacity(tensor_count.min(8192));

        for _ in 0..tensor_count {
            let name = match Self::read_string(&mut file) {
                Ok(n) => n,
                Err(_) => break,
            };
            let n_dims = match Self::read_u32(&mut file) {
                Ok(d) => d,
                Err(_) => break,
            };
            // Skip dimensions
            for _ in 0..n_dims {
                if Self::read_u64(&mut file).is_err() {
                    break;
                }
            }
            // Skip type
            if Self::read_u32(&mut file).is_err() {
                break;
            }
            let offset = match Self::read_u64(&mut file) {
                Ok(o) => o,
                Err(_) => break,
            };
            raw_tensors.push(RawTensor { name, offset });
        }

        let mut attention_bytes_per_layer = 0u64;
        let mut expert_bytes_per_layer = 0u64;
        let mut dense_ffn_bytes_per_layer = 0u64;
        let mut base_reserve_bytes = 0u64;
        let mut total_expert_bytes = 0u64;
        let mut total_dense_bytes = 0u64;

        if raw_tensors.len() >= 2 {
            // Sort by offset to determine accurate tensor sizes by delta
            raw_tensors.sort_by_key(|t| t.offset);

            let mut attention_layer_accum: HashMap<usize, u64> = HashMap::new();
            let mut expert_layer_accum: HashMap<usize, u64> = HashMap::new();
            let mut dense_ffn_layer_accum: HashMap<usize, u64> = HashMap::new();

            for i in 0..raw_tensors.len() {
                let size = if i + 1 < raw_tensors.len() {
                    raw_tensors[i + 1].offset.saturating_sub(raw_tensors[i].offset)
                } else {
                    // Approximate last tensor size as average
                    raw_tensors[i].offset.saturating_sub(raw_tensors[i - 1].offset)
                };

                let name = &raw_tensors[i].name;
                let layer_idx = Self::extract_layer_index(name);

                if let Some(l) = layer_idx {
                    if name.contains("attn") || name.contains("attention") || name.contains("q_") || name.contains("k_") || name.contains("v_") || name.contains("o_") {
                        *attention_layer_accum.entry(l).or_insert(0) += size;
                        total_dense_bytes += size;
                    } else if name.contains("exps") || name.contains("expert") || name.contains("exp.") {
                        *expert_layer_accum.entry(l).or_insert(0) += size;
                        total_expert_bytes += size;
                    } else {
                        *dense_ffn_layer_accum.entry(l).or_insert(0) += size;
                        total_dense_bytes += size;
                    }
                } else {
                    base_reserve_bytes += size;
                    total_dense_bytes += size;
                }
            }

            if !attention_layer_accum.is_empty() {
                let count = attention_layer_accum.len() as u64;
                attention_bytes_per_layer = attention_layer_accum.values().sum::<u64>() / count;
            }
            if !expert_layer_accum.is_empty() {
                let count = expert_layer_accum.len() as u64;
                expert_bytes_per_layer = expert_layer_accum.values().sum::<u64>() / count;
            }
            if !dense_ffn_layer_accum.is_empty() {
                let count = dense_ffn_layer_accum.len() as u64;
                dense_ffn_bytes_per_layer = dense_ffn_layer_accum.values().sum::<u64>() / count;
            }
        }

        // Fallback sanity for sizing if tensor offsets were unparseable
        if total_dense_bytes == 0 && total_expert_bytes == 0 {
            if is_moe {
                total_expert_bytes = (file_size_bytes as f64 * 0.75) as u64;
                total_dense_bytes = file_size_bytes.saturating_sub(total_expert_bytes);
                expert_bytes_per_layer = total_expert_bytes / layer_count.max(1) as u64;
                attention_bytes_per_layer = (total_dense_bytes / layer_count.max(1) as u64) / 2;
            } else {
                total_dense_bytes = file_size_bytes;
                attention_bytes_per_layer = (file_size_bytes / layer_count.max(1) as u64) / 3;
            }
        }

        let info = GgufArchitectureInfo {
            architecture: arch,
            layer_count,
            head_count,
            head_count_kv,
            head_dim,
            context_length,
            is_moe,
            expert_count,
            expert_used_count,
            attention_bytes_per_layer,
            expert_bytes_per_layer,
            dense_ffn_bytes_per_layer,
            base_reserve_bytes,
            total_dense_bytes,
            total_expert_bytes,
            file_size_bytes,
        };

        if let Ok(mut guard) = cache.lock() {
            guard.insert(abs_path, info.clone());
        }

        Ok(info)
    }

    fn extract_layer_index(name: &str) -> Option<usize> {
        let parts: Vec<&str> = name.split('.').collect();
        for (i, part) in parts.iter().enumerate() {
            if (*part == "blk" || *part == "layers") && i + 1 < parts.len() {
                if let Ok(idx) = parts[i + 1].parse::<usize>() {
                    return Some(idx);
                }
            }
        }
        None
    }

    fn read_string(file: &mut BufReader<File>) -> std::io::Result<String> {
        let len = Self::read_u64(file)?;
        if len > 65536 {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "String length exceeded safety bound",
            ));
        }
        let mut buf = vec![0u8; len as usize];
        file.read_exact(&mut buf)?;
        Ok(String::from_utf8_lossy(&buf).to_string())
    }

    fn read_u32(file: &mut BufReader<File>) -> std::io::Result<u32> {
        let mut buf = [0u8; 4];
        file.read_exact(&mut buf)?;
        Ok(u32::from_le_bytes(buf))
    }

    fn read_u64(file: &mut BufReader<File>) -> std::io::Result<u64> {
        let mut buf = [0u8; 8];
        file.read_exact(&mut buf)?;
        Ok(u64::from_le_bytes(buf))
    }

    fn read_value(file: &mut BufReader<File>, value_type: u32) -> std::io::Result<String> {
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
                if len > 1_000_000 {
                    return Err(std::io::Error::new(
                        std::io::ErrorKind::InvalidData,
                        "Array length too large",
                    ));
                }
                if item_type == 8 {
                    let mut elements = Vec::new();
                    let read_len = std::cmp::min(len, 10);
                    for _ in 0..read_len {
                        elements.push(Self::read_string(file)?);
                    }
                    let mut scratch = [0u8; 8192];
                    for _ in read_len..len {
                        let mut str_len = Self::read_u64(file)?;
                        if str_len > 65536 {
                            return Err(std::io::Error::new(
                                std::io::ErrorKind::InvalidData,
                                "String length exceeded safety bound in array",
                            ));
                        }
                        while str_len > 0 {
                            let chunk = std::cmp::min(str_len, scratch.len() as u64) as usize;
                            file.read_exact(&mut scratch[..chunk])?;
                            str_len -= chunk as u64;
                        }
                    }
                    Ok(format!("{:?}", elements))
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
                        return Err(std::io::Error::new(
                            std::io::ErrorKind::InvalidData,
                            "Unsupported array type",
                        ));
                    }
                    Ok(format!("[Array: len={}]", len))
                }
            }
            _ => Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("Unknown GGUF value type: {}", value_type),
            )),
        }
    }

    /// Extracts byte offsets and element counts for all MoE expert tensors across layers safely.
    pub fn probe_raw_expert_tensors(
        path: &Path,
    ) -> Result<(usize, HashMap<(usize, String), (u64, u64)>), String> {
        let abs_path = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
        let cache = EXPERT_TENSOR_CACHE.get_or_init(|| Mutex::new(HashMap::new()));
        if let Ok(guard) = cache.lock() {
            if let Some(cached) = guard.get(&abs_path) {
                return Ok(cached.clone());
            }
        }

        let f = File::open(path).map_err(|e| e.to_string())?;
        let mut file = BufReader::with_capacity(1024 * 1024, f);

        let mut magic = [0u8; 4];
        file.read_exact(&mut magic).map_err(|e| e.to_string())?;
        if &magic != b"GGUF" {
            return Err("Invalid GGUF binary magic header".to_string());
        }

        let mut version_bytes = [0u8; 4];
        file.read_exact(&mut version_bytes).map_err(|e| e.to_string())?;
        let version = u32::from_le_bytes(version_bytes);
        if version < 2 {
            return Err(format!("Unsupported legacy GGUF version: {}", version));
        }

        let mut counts = [0u8; 16];
        file.read_exact(&mut counts).map_err(|e| e.to_string())?;
        let tensor_count = u64::from_le_bytes(counts[0..8].try_into().unwrap()) as usize;
        let metadata_kv_count = u64::from_le_bytes(counts[8..16].try_into().unwrap()) as usize;

        // Skip metadata safely
        for _ in 0..metadata_kv_count {
            let _key = match Self::read_string(&mut file) {
                Ok(k) => k,
                Err(_) => break,
            };
            let vtype = match Self::read_u32(&mut file) {
                Ok(vt) => vt,
                Err(_) => break,
            };
            if Self::read_value(&mut file, vtype).is_err() {
                break;
            }
        }

        let mut max_layer = 0usize;
        let mut raw_offsets: HashMap<(usize, String), (u64, u64)> = HashMap::new();

        for _ in 0..tensor_count {
            let name = match Self::read_string(&mut file) {
                Ok(n) => n,
                Err(_) => break,
            };
            let ndims = match Self::read_u32(&mut file) {
                Ok(d) => d as usize,
                Err(_) => break,
            };
            let mut total_elements = 1u64;
            for _ in 0..ndims {
                let dim = match Self::read_u64(&mut file) {
                    Ok(d) => d,
                    Err(_) => break,
                };
                total_elements = total_elements.saturating_mul(dim);
            }
            if Self::read_u32(&mut file).is_err() {
                break;
            }
            let offset = match Self::read_u64(&mut file) {
                Ok(o) => o,
                Err(_) => break,
            };

            if let Some(l) = Self::extract_layer_index(&name) {
                max_layer = max_layer.max(l + 1);
                if name.contains("gate_exps") {
                    raw_offsets.insert((l, "gate".to_string()), (offset, total_elements));
                } else if name.contains("up_exps") {
                    raw_offsets.insert((l, "up".to_string()), (offset, total_elements));
                } else if name.contains("down_exps") {
                    raw_offsets.insert((l, "down".to_string()), (offset, total_elements));
                }
            }
        }

        let res = (max_layer, raw_offsets);
        if let Ok(mut guard) = cache.lock() {
            guard.insert(abs_path, res.clone());
        }
        Ok(res)
    }
}
