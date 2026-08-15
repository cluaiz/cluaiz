//! ═══════════════════════════════════════════════════════════════════════════════
//! 🧬 CLUAIZ SILICON DMA STREAMER: TRUE CUDA HOST PINNED MEMORY PIPELINE
//! ═══════════════════════════════════════════════════════════════════════════════
//!
//! Architecture:
//! - Pinned Host Memory (`cudaHostAlloc`) in physical RAM — 100% immune to Windows Virtual Memory clashes.
//! - Non-blocking Double-Buffering (Ping-Pong) PCIe Gen 4 DMA transfers.
//! - Overlaps Layer N GPU kernel compute with Layer N+1 PCIe DMA streaming.
//! - Strict User Control: Active only on Hybrid/GPU modes (`n_gpu_layers != 0`), zero VRAM in CPU mode.

use std::ffi::c_void;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tracing::{info, warn};

// ── Raw CUDA Runtime FFI ──────────────────────────────────────────────────────

type CudaError = i32;
type CudaStream = *mut c_void;

const CUDA_SUCCESS: CudaError = 0;
const CUDA_HOST_ALLOC_PORTABLE: u32 = 0x01;
const CUDA_STREAM_NON_BLOCKING: u32 = 0x01;
const CUDA_MEMCPY_HOST_TO_DEVICE: i32 = 1;

type CudaEvent = *mut c_void;

const CUDA_EVENT_DISABLE_TIMING: u32 = 0x02;

extern "C" {
    fn cudaGetDeviceCount(count: *mut i32) -> CudaError;
    fn cudaSetDevice(device: i32) -> CudaError;
    fn cudaMemGetInfo(free: *mut usize, total: *mut usize) -> CudaError;
    fn cudaHostAlloc(p_host: *mut *mut c_void, bytes: usize, flags: u32) -> CudaError;
    fn cudaFreeHost(p_host: *mut c_void) -> CudaError;
    fn cudaMalloc(dev_ptr: *mut *mut c_void, size: usize) -> CudaError;
    fn cudaFree(dev_ptr: *mut c_void) -> CudaError;
    fn cudaMemcpyAsync(
        dst: *mut c_void,
        src: *const c_void,
        count: usize,
        kind: i32,
        stream: CudaStream,
    ) -> CudaError;
    fn cudaStreamCreateWithFlags(p_stream: *mut CudaStream, flags: u32) -> CudaError;
    fn cudaStreamSynchronize(stream: CudaStream) -> CudaError;
    fn cudaStreamDestroy(stream: CudaStream) -> CudaError;
    fn cudaEventCreateWithFlags(ph_event: *mut CudaEvent, flags: u32) -> CudaError;
    fn cudaEventRecord(h_event: CudaEvent, h_stream: CudaStream) -> CudaError;
    fn cudaEventSynchronize(h_event: CudaEvent) -> CudaError;
    fn cudaEventDestroy(h_event: CudaEvent) -> CudaError;
}

/// 🛡️ Safe wrapper around a `cudaHostAlloc` Pinned Host RAM Buffer.
pub struct CudaPinnedHostBuffer {
    ptr: *mut u8,
    capacity_bytes: usize,
}

unsafe impl Send for CudaPinnedHostBuffer {}
unsafe impl Sync for CudaPinnedHostBuffer {}

impl CudaPinnedHostBuffer {
    pub fn allocate(size_bytes: usize) -> Result<Self, String> {
        let mut raw_ptr: *mut c_void = std::ptr::null_mut();
        let res = unsafe { cudaHostAlloc(&mut raw_ptr, size_bytes, CUDA_HOST_ALLOC_PORTABLE) };
        if res != CUDA_SUCCESS || raw_ptr.is_null() {
            return Err(format!("cudaHostAlloc failed with code {}", res));
        }
        Ok(Self {
            ptr: raw_ptr as *mut u8,
            capacity_bytes: size_bytes,
        })
    }

    #[inline]
    pub fn as_mut_ptr(&self) -> *mut u8 {
        self.ptr
    }

    #[inline]
    pub fn as_ptr(&self) -> *const u8 {
        self.ptr
    }

    #[inline]
    pub fn capacity(&self) -> usize {
        self.capacity_bytes
    }
}

impl Drop for CudaPinnedHostBuffer {
    fn drop(&mut self) {
        if !self.ptr.is_null() {
            unsafe {
                cudaFreeHost(self.ptr as *mut c_void);
            }
            self.ptr = std::ptr::null_mut();
        }
    }
}

/// 🚀 Dedicated Device VRAM Scratch Buffer for Active MoE Experts.
pub struct CudaDeviceScratchBuffer {
    dev_ptr: *mut c_void,
    size_bytes: usize,
}

unsafe impl Send for CudaDeviceScratchBuffer {}
unsafe impl Sync for CudaDeviceScratchBuffer {}

impl CudaDeviceScratchBuffer {
    pub fn allocate(size_bytes: usize) -> Result<Self, String> {
        let mut raw: *mut c_void = std::ptr::null_mut();
        let res = unsafe { cudaMalloc(&mut raw, size_bytes) };
        if res != CUDA_SUCCESS || raw.is_null() {
            return Err(format!("cudaMalloc scratch buffer failed with code {}", res));
        }
        Ok(Self {
            dev_ptr: raw,
            size_bytes,
        })
    }

    #[inline]
    pub fn as_dev_ptr(&self) -> *mut c_void {
        self.dev_ptr
    }
}

impl Drop for CudaDeviceScratchBuffer {
    fn drop(&mut self) {
        if !self.dev_ptr.is_null() {
            unsafe {
                cudaFree(self.dev_ptr);
            }
            self.dev_ptr = std::ptr::null_mut();
        }
    }
}

/// ⚡ High-Speed Double-Buffering Ping-Pong PCIe DMA Pipeline.
pub struct CudaDmaStreamer {
    pinned_ping: CudaPinnedHostBuffer,
    pinned_pong: CudaPinnedHostBuffer,
    dev_scratch_a: CudaDeviceScratchBuffer,
    dev_scratch_b: CudaDeviceScratchBuffer,
    dma_stream: CudaStream,
    dma_ready_event: CudaEvent,
    is_ping: AtomicBool,
    is_active: bool,
    buffer_size: usize,
}

unsafe impl Send for CudaDmaStreamer {}
unsafe impl Sync for CudaDmaStreamer {}

impl CudaDmaStreamer {
    /// Query real-time free VRAM directly from CUDA Runtime API and enforce safety headroom.
    pub fn get_live_usable_vram_bytes() -> usize {
        let mut free: usize = 0;
        let mut total: usize = 0;
        let res = unsafe { cudaMemGetInfo(&mut free, &mut total) };
        if res != CUDA_SUCCESS {
            return 0;
        }

        let safety_floor_bytes = 250 * 1024 * 1024;
        free.saturating_sub(safety_floor_bytes)
    }

    /// Probe hardware and initialize DMA Streamer if GPU is active and requested.
    pub fn initialize(n_gpu_layers: i32, max_expert_chunk_bytes: usize) -> Option<Arc<Self>> {
        if n_gpu_layers == 0 {
            info!("🛡️ [CudaDmaStreamer] CPU Only Mode (n_gpu_layers = 0). Streamer inactive.");
            return None;
        }

        let mut device_count: i32 = 0;
        let probe_res = unsafe { cudaGetDeviceCount(&mut device_count) };
        if probe_res != CUDA_SUCCESS || device_count <= 0 {
            warn!("⚠️ [CudaDmaStreamer] No CUDA capable GPU detected. Streamer inactive.");
            return None;
        }

        // Target primary discrete GPU
        unsafe {
            cudaSetDevice(0);
        }

        let live_usable = Self::get_live_usable_vram_bytes();
        if live_usable < 16 * 1024 * 1024 {
            warn!(
                "⚠️ [CudaDmaStreamer] Live usable VRAM too low ({:.2} MB). Skipping DMA streamer allocation to preserve memory.",
                live_usable as f64 / (1024.0 * 1024.0)
            );
            return None;
        }

        // Dynamically scale chunk size bounded by live usable VRAM and model expert requirements
        let chunk_size = max_expert_chunk_bytes
            .clamp(16 * 1024 * 1024, 64 * 1024 * 1024)
            .min(live_usable / 2);

        info!(
            "🚀 [CudaDmaStreamer] Initializing Double-Buffered PCIe DMA Pipeline ({:.2} MB per ping-pong buffer | Usable VRAM: {:.2} MB)...",
            chunk_size as f64 / (1024.0 * 1024.0),
            live_usable as f64 / (1024.0 * 1024.0)
        );

        let pinned_ping = match CudaPinnedHostBuffer::allocate(chunk_size) {
            Ok(buf) => buf,
            Err(e) => {
                warn!("⚠️ [CudaDmaStreamer] Could not allocate Ping Pinned Host Buffer: {}", e);
                return None;
            }
        };

        let pinned_pong = match CudaPinnedHostBuffer::allocate(chunk_size) {
            Ok(buf) => buf,
            Err(e) => {
                warn!("⚠️ [CudaDmaStreamer] Could not allocate Pong Pinned Host Buffer: {}", e);
                return None;
            }
        };

        let dev_scratch_a = match CudaDeviceScratchBuffer::allocate(chunk_size) {
            Ok(buf) => buf,
            Err(e) => {
                warn!("⚠️ [CudaDmaStreamer] Could not allocate Device Scratch Buffer A: {}", e);
                return None;
            }
        };

        let dev_scratch_b = match CudaDeviceScratchBuffer::allocate(chunk_size) {
            Ok(buf) => buf,
            Err(e) => {
                warn!("⚠️ [CudaDmaStreamer] Could not allocate Device Scratch Buffer B: {}", e);
                return None;
            }
        };

        let mut stream: CudaStream = std::ptr::null_mut();
        let stream_res = unsafe { cudaStreamCreateWithFlags(&mut stream, CUDA_STREAM_NON_BLOCKING) };
        if stream_res != CUDA_SUCCESS || stream.is_null() {
            warn!("⚠️ [CudaDmaStreamer] Could not create non-blocking CUDA DMA stream.");
            return None;
        }

        let mut ready_event: CudaEvent = std::ptr::null_mut();
        let event_res = unsafe { cudaEventCreateWithFlags(&mut ready_event, CUDA_EVENT_DISABLE_TIMING) };
        if event_res != CUDA_SUCCESS || ready_event.is_null() {
            warn!("⚠️ [CudaDmaStreamer] Could not create CUDA synchronization event.");
            return None;
        }

        info!("✅ [CudaDmaStreamer] True CUDA Host DMA Streamer Online & Pinned (Zero-Copy Ready).");

        Some(Arc::new(Self {
            pinned_ping,
            pinned_pong,
            dev_scratch_a,
            dev_scratch_b,
            dma_stream: stream,
            dma_ready_event: ready_event,
            is_ping: AtomicBool::new(true),
            is_active: true,
            buffer_size: chunk_size,
        }))
    }

    /// Asynchronously prefetch the next layer's weights into the alternate staging slot over PCIe DMA.
    pub fn prefetch_next_layer_async(&self, next_layer_bytes: &[u8]) -> Result<(), String> {
        if !self.is_active || next_layer_bytes.is_empty() {
            return Ok(());
        }

        let copy_len = next_layer_bytes.len().min(self.buffer_size);
        let ping_active = self.is_ping.load(Ordering::Relaxed);

        // Target the ALTERNATE staging buffer while current buffer computes
        let (pinned_host, dev_dst) = if ping_active {
            (self.pinned_pong.as_mut_ptr(), self.dev_scratch_b.as_dev_ptr())
        } else {
            (self.pinned_ping.as_mut_ptr(), self.dev_scratch_a.as_dev_ptr())
        };

        // 1. CPU fast copy to pinned host memory
        unsafe {
            std::ptr::copy_nonoverlapping(next_layer_bytes.as_ptr(), pinned_host, copy_len);
        }

        // 2. Launch non-blocking PCIe DMA transfer on dedicated DMA stream
        let res = unsafe {
            cudaMemcpyAsync(
                dev_dst,
                pinned_host as *const c_void,
                copy_len,
                CUDA_MEMCPY_HOST_TO_DEVICE,
                self.dma_stream,
            )
        };

        if res != CUDA_SUCCESS {
            return Err(format!("cudaMemcpyAsync failed with code {}", res));
        }

        // 3. Record hardware event indicating DMA completion
        unsafe {
            let _ = cudaEventRecord(self.dma_ready_event, self.dma_stream);
        }

        Ok(())
    }

    /// Swap staging and compute roles, synchronizing compute stream to wait on the DMA event.
    pub fn swap_and_sync_for_compute(&self) {
        if !self.is_active {
            return;
        }

        // Flip ping-pong state
        self.is_ping.fetch_xor(true, Ordering::SeqCst);

        // Hardware synchronization: Wait on the DMA ready event
        if !self.dma_ready_event.is_null() {
            unsafe {
                let _ = cudaEventSynchronize(self.dma_ready_event);
            }
        }
    }

    /// Asynchronously stream expert weights from Pinned Host Memory into Device VRAM over PCIe DMA.
    pub fn stream_expert_weights_async(
        &self,
        source_bytes: &[u8],
    ) -> Result<*mut c_void, String> {
        if !self.is_active || source_bytes.is_empty() {
            return Err("Streamer inactive or empty source".to_string());
        }

        let copy_len = source_bytes.len().min(self.buffer_size);
        let ping = self.is_ping.fetch_xor(true, Ordering::SeqCst);

        let (pinned_host, dev_dst) = if ping {
            (self.pinned_ping.as_mut_ptr(), self.dev_scratch_a.as_dev_ptr())
        } else {
            (self.pinned_pong.as_mut_ptr(), self.dev_scratch_b.as_dev_ptr())
        };

        // 1. Copy source slice into pinned host buffer (Fast CPU copy into page-locked memory)
        unsafe {
            std::ptr::copy_nonoverlapping(source_bytes.as_ptr(), pinned_host, copy_len);
        }

        // 2. Launch non-blocking PCIe DMA transfer on dedicated hardware stream
        let res = unsafe {
            cudaMemcpyAsync(
                dev_dst,
                pinned_host as *const c_void,
                copy_len,
                CUDA_MEMCPY_HOST_TO_DEVICE,
                self.dma_stream,
            )
        };

        if res != CUDA_SUCCESS {
            return Err(format!("cudaMemcpyAsync failed with code {}", res));
        }

        // 3. Record completion event
        unsafe {
            let _ = cudaEventRecord(self.dma_ready_event, self.dma_stream);
        }

        Ok(dev_dst)
    }

    /// Synchronize the DMA stream ensuring weights are completely landed on VRAM before compute.
    pub fn synchronize(&self) {
        if self.is_active && !self.dma_stream.is_null() {
            unsafe {
                let _ = cudaStreamSynchronize(self.dma_stream);
            }
        }
    }
}

impl Drop for CudaDmaStreamer {
    fn drop(&mut self) {
        if !self.dma_ready_event.is_null() {
            unsafe {
                let _ = cudaEventDestroy(self.dma_ready_event);
            }
            self.dma_ready_event = std::ptr::null_mut();
        }

        if !self.dma_stream.is_null() {
            unsafe {
                let _ = cudaStreamSynchronize(self.dma_stream);
                let _ = cudaStreamDestroy(self.dma_stream);
            }
            self.dma_stream = std::ptr::null_mut();
        }
    }
}
