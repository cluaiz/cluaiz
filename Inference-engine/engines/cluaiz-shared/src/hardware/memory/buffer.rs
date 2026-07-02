use std::path::Path;
use anyhow::Result;

/// 🧠 SovereignBuffer
/// The universal interface for data residing in memory, 
/// whether it's mmap-ed from disk or allocated in RAM.
pub trait SovereignBuffer: Send + Sync {
    /// Returns a pointer to the start of the data.
    fn as_ptr(&self) -> *const u8;
    
    /// Returns the size of the buffer in bytes.
    fn len(&self) -> usize;
    
    /// Returns the underlying data as a byte slice.
    fn as_slice(&self) -> &[u8];
    
    /// Check if the buffer is empty.
    fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// A buffer that is mapped directly from a file on disk.
/// Zero-copy, zero-overhead.
pub struct MappedBuffer {
    mmap: memmap2::Mmap,
}

impl MappedBuffer {
    pub fn from_file<P: AsRef<Path>>(path: P) -> Result<Self> {
        let file = std::fs::File::open(path)?;
        let mmap = unsafe { memmap2::Mmap::map(&file)? };
        Ok(Self { mmap })
    }
}

impl SovereignBuffer for MappedBuffer {
    fn as_ptr(&self) -> *const u8 {
        self.mmap.as_ptr()
    }
    
    fn len(&self) -> usize {
        self.mmap.len()
    }

    fn as_slice(&self) -> &[u8] {
        &self.mmap[..]
    }
}

/// A buffer mapped from a SafeTensors file, providing zero-copy access to the underlying tensor data.
pub struct SafeTensorsMappedBuffer {
    mmap: memmap2::Mmap,
    data_offset: usize,
    data_len: usize,
}

impl SafeTensorsMappedBuffer {
    pub fn from_file<P: AsRef<Path>>(path: P) -> Result<Self> {
        let file = std::fs::File::open(path)?;
        let mmap = unsafe { memmap2::Mmap::map(&file)? };
        
        // Parse SafeTensors to find the data offset for "llama_state"
        let data_offset = {
            let st = safetensors::SafeTensors::deserialize(&mmap)
                .map_err(|e| anyhow::anyhow!("Failed to parse safetensors: {:?}", e))?;
            let tensor = st.tensor("llama_state")
                .map_err(|e| anyhow::anyhow!("Failed to find llama_state tensor: {:?}", e))?;
            
            // We need to calculate the exact offset into the mmap slice.
            // Safetensors data() slice is borrowed from the mmap. We can find its offset using ptr arithmetic.
            let mmap_start = mmap.as_ptr() as usize;
            let tensor_start = tensor.data().as_ptr() as usize;
            tensor_start - mmap_start
        };
        
        let data_len = {
            let st = safetensors::SafeTensors::deserialize(&mmap)
                .map_err(|e| anyhow::anyhow!("Failed to parse safetensors: {:?}", e))?;
            let tensor = st.tensor("llama_state").unwrap();
            tensor.data().len()
        };

        Ok(Self { mmap, data_offset, data_len })
    }
}

impl SovereignBuffer for SafeTensorsMappedBuffer {
    fn as_ptr(&self) -> *const u8 {
        unsafe { self.mmap.as_ptr().add(self.data_offset) }
    }
    
    fn len(&self) -> usize {
        self.data_len
    }

    fn as_slice(&self) -> &[u8] {
        &self.mmap[self.data_offset..self.data_offset + self.data_len]
    }
}

/// A buffer that resides in standard heap memory.
pub struct ActiveBuffer {
    data: Vec<u8>,
}

impl ActiveBuffer {
    pub fn new(size: usize) -> Self {
        Self { data: vec![0; size] }
    }

    pub fn new_from_vec(data: Vec<u8>) -> Self {
        Self { data }
    }
}

impl SovereignBuffer for ActiveBuffer {
    fn as_ptr(&self) -> *const u8 {
        self.data.as_ptr()
    }
    
    fn len(&self) -> usize {
        self.data.len()
    }

    fn as_slice(&self) -> &[u8] {
        &self.data
    }
}
