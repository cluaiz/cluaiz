//! 🚀 Asynchronous Lookahead MoE Expert Prefetcher
//! Orchestrates background I/O threads to stream upcoming MoE experts
//! from NVMe storage into the Static Ring-Buffer concurrently with compute execution.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use tracing::{debug, error, info, warn};

use crate::hardware::expert_offloading::direct_io::DirectFileReader;
use crate::hardware::expert_offloading::expert_cache::{LoadedExpertBlock, SharedExpertCache};
use crate::hardware::expert_offloading::expert_index::ExpertOffsetIndex;
use crate::hardware::expert_offloading::ring_buffer::SharedStagingBuffer;

/// Commands sent to the background prefetch worker.
pub enum PrefetchCommand {
    /// Prefetch specific active experts for an upcoming layer
    PrefetchLayer {
        layer: usize,
        expert_ids: Vec<usize>,
    },
    /// Ping-Pong swap the staging ring buffer
    SwapRingBuffer,
    /// Terminate the worker thread
    Shutdown,
}

/// The Asynchronous Expert Prefetcher orchestrator.
pub struct AsyncExpertPrefetcher {
    sender: Sender<PrefetchCommand>,
    worker_handle: Option<JoinHandle<()>>,
    is_running: Arc<AtomicBool>,
    pub staging_buffer: SharedStagingBuffer,
    pub cache: SharedExpertCache,
}

impl AsyncExpertPrefetcher {
    /// Spawns a background Direct I/O prefetch worker thread.
    pub fn spawn(
        model_path: &Path,
        expert_index: Arc<ExpertOffsetIndex>,
        cache: SharedExpertCache,
        slot_capacity_bytes: usize,
    ) -> anyhow::Result<Self> {
        let (sender, receiver) = channel::<PrefetchCommand>();
        let is_running = Arc::new(AtomicBool::new(true));
        let staging_buffer = SharedStagingBuffer::new(slot_capacity_bytes)?;

        let model_path_buf = model_path.to_path_buf();
        let running_flag = Arc::clone(&is_running);
        let staging_clone = staging_buffer.clone();
        let cache_clone = cache.clone();
        let index_clone = Arc::clone(&expert_index);

        info!(
            "🚀 [AsyncPrefetcher] Spawning dedicated Direct I/O background worker thread for {:?}",
            model_path.file_name().unwrap_or_default()
        );

        let worker_handle = thread::Builder::new()
            .name("cluaiz-moe-prefetch".to_string())
            .spawn(move || {
                Self::worker_loop(
                    model_path_buf,
                    index_clone,
                    receiver,
                    running_flag,
                    staging_clone,
                    cache_clone,
                );
            })?;

        Ok(Self {
            sender,
            worker_handle: Some(worker_handle),
            is_running,
            staging_buffer,
            cache,
        })
    }

    /// Triggers lookahead prefetching for the next upcoming layer.
    pub fn request_layer_prefetch(&self, layer: usize, expert_ids: &[usize]) {
        let cmd = PrefetchCommand::PrefetchLayer {
            layer,
            expert_ids: expert_ids.to_vec(),
        };
        if let Err(e) = self.sender.send(cmd) {
            warn!("⚠️ [AsyncPrefetcher] Failed to enqueue prefetch command: {}", e);
        }
    }

    /// Signals the ring buffer to swap active slots for the next compute step.
    pub fn trigger_buffer_swap(&self) {
        if let Err(e) = self.sender.send(PrefetchCommand::SwapRingBuffer) {
            warn!("⚠️ [AsyncPrefetcher] Failed to enqueue swap command: {}", e);
        }
    }

    /// Background worker event loop.
    fn worker_loop(
        model_path: PathBuf,
        index: Arc<ExpertOffsetIndex>,
        receiver: Receiver<PrefetchCommand>,
        running_flag: Arc<AtomicBool>,
        staging_buffer: SharedStagingBuffer,
        cache: SharedExpertCache,
    ) {
        // Open Direct I/O handle in background thread context
        let direct_reader = match DirectFileReader::open(&model_path) {
            Ok(r) => r,
            Err(e) => {
                error!("❌ [AsyncPrefetcher] Worker failed to open Direct I/O handle: {}", e);
                return;
            }
        };

        info!("🚀 [AsyncPrefetcher] Worker thread active and ready for streaming commands.");

        while running_flag.load(Ordering::Relaxed) {
            match receiver.recv() {
                Ok(PrefetchCommand::PrefetchLayer { layer, expert_ids }) => {
                    for expert_id in expert_ids {
                        // 1. Check if expert is already in LRU cache
                        let already_cached = if let Ok(mut c) = cache.0.lock() {
                            c.get(layer, expert_id).is_some()
                        } else {
                            false
                        };

                        if already_cached {
                            debug!("⚡ [AsyncPrefetcher] Layer {} Expert {} already cached in RAM.", layer, expert_id);
                            continue;
                        }

                        // 2. Read expert directly from NVMe SSD using Direct I/O
                        if let Some(entry) = index.lookup(layer, expert_id) {
                            let gate_start = entry.gate.file_offset;
                            let gate_len = entry.gate.byte_length as usize;
                            let up_start = entry.up.file_offset;
                            let up_len = entry.up.byte_length as usize;
                            let down_start = entry.down.file_offset;
                            let down_len = entry.down.byte_length as usize;

                            let total_bytes = gate_len + up_len + down_len;
                            let mut expert_raw_data = vec![0u8; total_bytes];

                            let read_res = (|| -> anyhow::Result<()> {
                                direct_reader.read_range(gate_start, gate_len, &mut expert_raw_data[0..gate_len])?;
                                direct_reader.read_range(up_start, up_len, &mut expert_raw_data[gate_len..gate_len + up_len])?;
                                direct_reader.read_range(down_start, down_len, &mut expert_raw_data[gate_len + up_len..])?;
                                Ok(())
                            })();

                            match read_res {
                                Ok(()) => {
                                    // 3. Stage into ring buffer and LRU cache
                                    let block = LoadedExpertBlock {
                                        expert_id,
                                        layer_index: layer,
                                        size_bytes: total_bytes,
                                        weights_data: Arc::new(expert_raw_data.clone()),
                                    };

                                    if let Ok(mut c) = cache.0.lock() {
                                        c.insert(block);
                                    }

                                    if let Ok(mut ring) = staging_buffer.0.lock() {
                                        let _ = ring.stage_expert(layer, expert_id, &expert_raw_data);
                                    }

                                    debug!(
                                        "⚡ [AsyncPrefetcher] Prefetched L{}E{} ({:.2} MB) via Direct I/O",
                                        layer, expert_id, total_bytes as f64 / (1024.0 * 1024.0)
                                    );
                                }
                                Err(e) => {
                                    warn!("⚠️ [AsyncPrefetcher] Direct read error for L{}E{}: {}", layer, expert_id, e);
                                }
                            }
                        }
                    }
                }
                Ok(PrefetchCommand::SwapRingBuffer) => {
                    if let Ok(mut ring) = staging_buffer.0.lock() {
                        ring.swap_slots();
                    }
                }
                Ok(PrefetchCommand::Shutdown) => {
                    info!("🛑 [AsyncPrefetcher] Worker received shutdown signal.");
                    break;
                }
                Err(_) => {
                    // Channel disconnected
                    break;
                }
            }
        }

        info!("🛑 [AsyncPrefetcher] Worker thread exited cleanly.");
    }
}

impl Drop for AsyncExpertPrefetcher {
    fn drop(&mut self) {
        self.is_running.store(false, Ordering::Relaxed);
        let _ = self.sender.send(PrefetchCommand::Shutdown);
        if let Some(handle) = self.worker_handle.take() {
            let _ = handle.join();
        }
    }
}
