//! Exact GGUF layout for placement: bytes of every block (attention/shared vs MoE experts),
//! the input layer (always CPU in llama.cpp), the output layer (GPU when n_gpu_layers > n_layer),
//! and the KV geometry per layer (KV heads, K/V widths, sliding-window layers, shared-KV layers).
//!
//! Tensor bytes are the distance between tensor offsets in the file (padding included), the last
//! tensor runs to the end of the file. Nothing here is guessed from the file size.

use std::collections::HashMap;
use std::fs::File;
use std::io::{BufReader, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

static LAYOUT_CACHE: OnceLock<Mutex<HashMap<PathBuf, GgufLayout>>> = OnceLock::new();

#[derive(Debug, Clone)]
enum Val {
    Int(i64),
    Float(f64),
    Bool(bool),
    Str(String),
    Ints(Vec<i64>),
    Bools(Vec<bool>),
    /// Array of strings or nested arrays: only the length is kept.
    Len(u64),
}

#[derive(Debug, Clone, Copy, Default)]
pub struct KvLayer {
    /// K values per cell (n_head_kv * key_length).
    pub k_per_cell: u64,
    /// V values per cell (n_head_kv * value_length).
    pub v_per_cell: u64,
    /// Sliding-window layer: holds at most `n_swa + n_ubatch` cells.
    pub swa: bool,
    /// False for layers that reuse an earlier layer's KV (shared KV) or have no attention.
    pub has_kv: bool,
}

#[derive(Debug, Clone, Default)]
pub struct GgufLayout {
    pub path: PathBuf,
    pub arch: String,
    pub file_bytes: u64,
    pub n_layer: usize,
    pub n_embd: usize,
    pub n_vocab: usize,
    pub native_ctx: usize,
    pub expert_count: usize,
    pub expert_used: usize,
    /// Per block: everything except routed experts (attention, norms, dense/shared FFN).
    pub block_other_bytes: Vec<u64>,
    /// Per block: routed expert tensors (`ffn_*_exps`).
    pub block_expert_bytes: Vec<u64>,
    /// token_embd (+ per-layer token embeddings). llama.cpp keeps the input layer on CPU.
    pub input_bytes: u64,
    /// Output head + output norm + other non-block tensors. Goes to GPU when n_gpu_layers > n_layer.
    pub output_bytes: u64,
    /// No `output.weight` in the file: llama.cpp duplicates token_embd as the output head.
    pub output_tied: bool,
    pub n_swa: usize,
    pub shared_kv_layers: usize,
    pub kv_layers: Vec<KvLayer>,
}

impl GgufLayout {
    pub fn is_moe(&self) -> bool {
        self.expert_count > 0 && self.block_expert_bytes.iter().any(|&b| b > 0)
    }
    pub fn total_expert_bytes(&self) -> u64 {
        self.block_expert_bytes.iter().sum()
    }
    pub fn total_block_bytes(&self) -> u64 {
        self.block_other_bytes.iter().sum::<u64>() + self.total_expert_bytes()
    }
    pub fn block_bytes(&self, il: usize) -> u64 {
        self.block_other_bytes.get(il).copied().unwrap_or(0)
            + self.block_expert_bytes.get(il).copied().unwrap_or(0)
    }
    /// Bytes read per generated token for a block: everything except the experts that are not routed.
    pub fn block_active_bytes(&self, il: usize) -> f64 {
        let exp = self.block_expert_bytes.get(il).copied().unwrap_or(0) as f64;
        let frac = if self.expert_count > 0 {
            self.expert_used as f64 / self.expert_count as f64
        } else {
            1.0
        };
        self.block_other_bytes.get(il).copied().unwrap_or(0) as f64 + exp * frac
    }

    /// Cells llama.cpp allocates for a layer (n_ctx padded to 256; SWA layers capped, swa_full = false).
    pub fn kv_cells(&self, layer: &KvLayer, n_ctx: usize, n_ubatch: usize) -> u64 {
        let pad = |x: usize| x.div_ceil(256) * 256;
        let base = pad(n_ctx.max(1));
        if layer.swa && self.n_swa > 0 {
            base.min(pad(self.n_swa + n_ubatch)) as u64
        } else {
            base as u64
        }
    }

    /// KV bytes of one layer at `n_ctx` with `elem_bytes` per K/V value (F16 = 2, Q8_0 = 34/32, Q4_0 = 18/32).
    pub fn kv_layer_bytes(&self, il: usize, n_ctx: usize, n_ubatch: usize, elem_bytes: f64) -> f64 {
        match self.kv_layers.get(il) {
            Some(l) if l.has_kv => {
                (l.k_per_cell + l.v_per_cell) as f64 * self.kv_cells(l, n_ctx, n_ubatch) as f64 * elem_bytes
            }
            _ => 0.0,
        }
    }

    pub fn kv_total_bytes(&self, n_ctx: usize, n_ubatch: usize, elem_bytes: f64) -> f64 {
        (0..self.n_layer)
            .map(|il| self.kv_layer_bytes(il, n_ctx, n_ubatch, elem_bytes))
            .sum()
    }

    /// KV bytes added per extra context token (only full-attention layers grow with n_ctx).
    pub fn kv_bytes_per_ctx_token(&self, elem_bytes: f64) -> f64 {
        self.kv_layers
            .iter()
            .filter(|l| l.has_kv && !(l.swa && self.n_swa > 0))
            .map(|l| (l.k_per_cell + l.v_per_cell) as f64 * elem_bytes)
            .sum()
    }

    pub fn kv_layer_count(&self) -> usize {
        self.kv_layers.iter().filter(|l| l.has_kv).count()
    }
    pub fn swa_layer_count(&self) -> usize {
        self.kv_layers.iter().filter(|l| l.has_kv && l.swa && self.n_swa > 0).count()
    }
}

pub fn probe_layout(path: &Path) -> Result<GgufLayout, String> {
    let key = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    let cache = LAYOUT_CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    if let Some(l) = cache.lock().ok().and_then(|c| c.get(&key).cloned()) {
        return Ok(l);
    }
    let layout = read_layout(path)?;
    if let Ok(mut c) = cache.lock() {
        c.insert(key, layout.clone());
    }
    Ok(layout)
}

fn read_layout(path: &Path) -> Result<GgufLayout, String> {
    let file_bytes = std::fs::metadata(path).map_err(|e| e.to_string())?.len();
    let mut f = BufReader::with_capacity(1 << 20, File::open(path).map_err(|e| e.to_string())?);

    let mut magic = [0u8; 4];
    f.read_exact(&mut magic).map_err(|e| e.to_string())?;
    if &magic != b"GGUF" {
        return Err("not a GGUF file".into());
    }
    let version = rd_u32(&mut f)?;
    if version < 2 {
        return Err(format!("GGUF v{} not supported", version));
    }
    let n_tensors = rd_u64(&mut f)? as usize;
    let n_kv = rd_u64(&mut f)? as usize;

    let mut meta: HashMap<String, Val> = HashMap::new();
    for _ in 0..n_kv {
        let k = rd_str(&mut f)?;
        let t = rd_u32(&mut f)?;
        let v = rd_val(&mut f, t)?;
        meta.insert(k, v);
    }

    struct T {
        name: String,
        dims: Vec<u64>,
        offset: u64,
    }
    let mut tensors = Vec::with_capacity(n_tensors);
    for _ in 0..n_tensors {
        let name = rd_str(&mut f)?;
        let nd = rd_u32(&mut f)? as usize;
        let mut dims = Vec::with_capacity(nd);
        for _ in 0..nd {
            dims.push(rd_u64(&mut f)?);
        }
        let _ty = rd_u32(&mut f)?;
        let offset = rd_u64(&mut f)?;
        tensors.push(T { name, dims, offset });
    }
    let header_end = f.stream_position().map_err(|e| e.to_string())?;
    let align = match meta.get("general.alignment") {
        Some(Val::Int(a)) if *a > 0 => *a as u64,
        _ => 32,
    };
    let data_start = header_end.div_ceil(align) * align;

    let arch = match meta.get("general.architecture") {
        Some(Val::Str(s)) => s.clone(),
        _ => "llama".into(),
    };
    let int = |k: &str| -> Option<i64> {
        match meta.get(&format!("{}.{}", arch, k)) {
            Some(Val::Int(v)) => Some(*v),
            Some(Val::Float(v)) => Some(*v as i64),
            _ => None,
        }
    };
    // Scalar or per-layer array.
    let per_layer = |k: &str, n: usize| -> Option<Vec<i64>> {
        match meta.get(&format!("{}.{}", arch, k)) {
            Some(Val::Int(v)) => Some(vec![*v; n]),
            Some(Val::Ints(a)) if !a.is_empty() => {
                Some((0..n).map(|i| a.get(i).copied().unwrap_or(*a.last().unwrap())).collect())
            }
            _ => None,
        }
    };

    let n_layer = int("block_count").unwrap_or(0).max(0) as usize;
    if n_layer == 0 {
        return Err("block_count missing in GGUF".into());
    }
    let n_embd = int("embedding_length").unwrap_or(0).max(0) as usize;
    let native_ctx = int("context_length").unwrap_or(0).max(0) as usize;
    let expert_count = int("expert_count").unwrap_or(0).max(0) as usize;
    let expert_used = int("expert_used_count").unwrap_or(0).max(0) as usize;
    let n_head = per_layer("attention.head_count", n_layer).unwrap_or_else(|| vec![0; n_layer]);
    let n_head_kv = per_layer("attention.head_count_kv", n_layer).unwrap_or_else(|| n_head.clone());
    let head_dim_default = if n_head.first().copied().unwrap_or(0) > 0 {
        n_embd as i64 / n_head[0]
    } else {
        0
    };
    let k_len = int("attention.key_length").unwrap_or(head_dim_default);
    let v_len = int("attention.value_length").unwrap_or(k_len);
    let k_len_swa = int("attention.key_length_swa").unwrap_or(k_len);
    let v_len_swa = int("attention.value_length_swa").unwrap_or(v_len);
    let n_swa = int("attention.sliding_window").unwrap_or(0).max(0) as usize;
    let shared_kv_layers = int("attention.shared_kv_layers").unwrap_or(0).max(0) as usize;

    // Sliding-window pattern: bool array (true = SWA layer) or integer N (llama.cpp set_swa_pattern:
    // layer il is SWA when il % N < N - 1). No pattern -> every layer full attention.
    let swa_flags: Vec<bool> = match meta.get(&format!("{}.attention.sliding_window_pattern", arch)) {
        Some(Val::Bools(b)) => (0..n_layer).map(|i| b.get(i).copied().unwrap_or(false)).collect(),
        Some(Val::Int(np)) if *np > 0 => {
            let np = *np as usize;
            (0..n_layer).map(|il| il % np < np - 1).collect()
        }
        _ => vec![false; n_layer],
    };
    let kv_from_start = n_layer.saturating_sub(shared_kv_layers);
    let kv_layers = (0..n_layer)
        .map(|il| {
            let swa = swa_flags[il] && n_swa > 0;
            let kv_heads = n_head_kv[il].max(0) as u64;
            let (kl, vl) = if swa { (k_len_swa, v_len_swa) } else { (k_len, v_len) };
            KvLayer {
                k_per_cell: kv_heads * kl.max(0) as u64,
                v_per_cell: kv_heads * vl.max(0) as u64,
                swa,
                has_kv: kv_heads > 0 && il < kv_from_start,
            }
        })
        .collect();

    // Tensor bytes from offset deltas.
    let mut order: Vec<usize> = (0..tensors.len()).collect();
    order.sort_by_key(|&i| tensors[i].offset);
    let mut bytes = vec![0u64; tensors.len()];
    for (pos, &i) in order.iter().enumerate() {
        let end = order
            .get(pos + 1)
            .map(|&j| tensors[j].offset)
            .unwrap_or(file_bytes.saturating_sub(data_start));
        bytes[i] = end.saturating_sub(tensors[i].offset);
    }

    let mut block_other_bytes = vec![0u64; n_layer];
    let mut block_expert_bytes = vec![0u64; n_layer];
    let mut input_bytes = 0u64;
    let mut output_bytes = 0u64;
    let mut token_embd_bytes = 0u64;
    let mut has_output = false;
    let mut n_vocab = 0usize;
    for (i, t) in tensors.iter().enumerate() {
        let b = bytes[i];
        if let Some(il) = block_index(&t.name) {
            if il < n_layer {
                if t.name.contains("_exps") || t.name.contains("_chexps") {
                    block_expert_bytes[il] += b;
                } else {
                    block_other_bytes[il] += b;
                }
            } else {
                // Extra blocks (MTP / NextN heads) live with the output layer.
                output_bytes += b;
            }
        } else if t.name.contains("token_embd") {
            input_bytes += b;
            if t.name == "token_embd.weight" {
                token_embd_bytes = b;
                n_vocab = t.dims.get(1).copied().unwrap_or(0) as usize;
            }
        } else {
            if t.name == "output.weight" {
                has_output = true;
                if n_vocab == 0 {
                    n_vocab = t.dims.get(1).copied().unwrap_or(0) as usize;
                }
            }
            output_bytes += b;
        }
    }
    if !has_output {
        output_bytes += token_embd_bytes;
    }
    if n_vocab == 0 {
        if let Some(Val::Len(n)) = meta.get("tokenizer.ggml.tokens") {
            n_vocab = *n as usize;
        }
    }

    Ok(GgufLayout {
        path: path.to_path_buf(),
        arch,
        file_bytes,
        n_layer,
        n_embd,
        n_vocab,
        native_ctx,
        expert_count,
        expert_used,
        block_other_bytes,
        block_expert_bytes,
        input_bytes,
        output_bytes,
        output_tied: !has_output,
        n_swa,
        shared_kv_layers,
        kv_layers,
    })
}

fn block_index(name: &str) -> Option<usize> {
    let rest = name.strip_prefix("blk.")?;
    rest.split('.').next()?.parse().ok()
}

fn io(e: std::io::Error) -> String {
    e.to_string()
}
fn rd_u32<R: Read>(f: &mut R) -> Result<u32, String> {
    let mut b = [0u8; 4];
    f.read_exact(&mut b).map_err(io)?;
    Ok(u32::from_le_bytes(b))
}
fn rd_u64<R: Read>(f: &mut R) -> Result<u64, String> {
    let mut b = [0u8; 8];
    f.read_exact(&mut b).map_err(io)?;
    Ok(u64::from_le_bytes(b))
}
fn rd_str<R: Read>(f: &mut R) -> Result<String, String> {
    let n = rd_u64(f)? as usize;
    if n > (1 << 24) {
        return Err("GGUF string too long".into());
    }
    let mut b = vec![0u8; n];
    f.read_exact(&mut b).map_err(io)?;
    Ok(String::from_utf8_lossy(&b).into_owned())
}
fn skip<R: Read + Seek>(f: &mut R, n: u64) -> Result<(), String> {
    f.seek(SeekFrom::Current(n as i64)).map_err(io)?;
    Ok(())
}
fn scalar_size(t: u32) -> Option<u64> {
    match t {
        0 | 1 | 7 => Some(1),
        2 | 3 => Some(2),
        4 | 5 | 6 => Some(4),
        10 | 11 | 12 => Some(8),
        _ => None,
    }
}
fn rd_scalar<R: Read>(f: &mut R, t: u32) -> Result<Val, String> {
    let n = scalar_size(t).ok_or("bad GGUF scalar type")? as usize;
    let mut b = [0u8; 8];
    f.read_exact(&mut b[..n]).map_err(io)?;
    Ok(match t {
        0 => Val::Int(b[0] as i64),
        1 => Val::Int(b[0] as i8 as i64),
        2 => Val::Int(u16::from_le_bytes([b[0], b[1]]) as i64),
        3 => Val::Int(i16::from_le_bytes([b[0], b[1]]) as i64),
        4 => Val::Int(u32::from_le_bytes([b[0], b[1], b[2], b[3]]) as i64),
        5 => Val::Int(i32::from_le_bytes([b[0], b[1], b[2], b[3]]) as i64),
        6 => Val::Float(f32::from_le_bytes([b[0], b[1], b[2], b[3]]) as f64),
        7 => Val::Bool(b[0] != 0),
        10 => Val::Int(u64::from_le_bytes(b) as i64),
        11 => Val::Int(i64::from_le_bytes(b)),
        _ => Val::Float(f64::from_le_bytes(b)),
    })
}
fn rd_val<R: Read + Seek>(f: &mut R, t: u32) -> Result<Val, String> {
    match t {
        8 => Ok(Val::Str(rd_str(f)?)),
        9 => {
            let et = rd_u32(f)?;
            let n = rd_u64(f)?;
            if let Some(sz) = scalar_size(et) {
                if n > 4096 {
                    skip(f, n * sz)?;
                    return Ok(Val::Len(n));
                }
                let mut ints = Vec::new();
                let mut bools = Vec::new();
                for _ in 0..n {
                    match rd_scalar(f, et)? {
                        Val::Bool(b) => bools.push(b),
                        Val::Int(i) => ints.push(i),
                        Val::Float(x) => ints.push(x as i64),
                        _ => {}
                    }
                }
                Ok(if et == 7 { Val::Bools(bools) } else { Val::Ints(ints) })
            } else {
                for _ in 0..n {
                    rd_val(f, et)?;
                }
                Ok(Val::Len(n))
            }
        }
        _ => rd_scalar(f, t),
    }
}
