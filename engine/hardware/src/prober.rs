//! 🔍 Hardware Prober Implementation
//! Directly queries OS and GPU drivers (NVML, sysinfo) to establish hardware truth.

use sysinfo::System;
use std::sync::RwLock;
use once_cell::sync::Lazy;
use cluaiz_core::{bytes_to_gib, GIB_F64};
use crate::types::{SystemInfo, CpuSubsystem, MemorySubsystem, StorageSubsystem, GpuSubsystem};

static CACHED_HARDWARE: Lazy<RwLock<Option<SystemInfo>>> = Lazy::new(|| RwLock::new(None));

pub struct HardwareProber;

impl HardwareProber {
    /// Returns cached hardware snapshot or performs initial probe.
    pub fn current() -> SystemInfo {
        {
            let guard = CACHED_HARDWARE.read().unwrap();
            if let Some(ref info) = *guard {
                return info.clone();
            }
        }
        let fresh = Self::probe_system();
        let mut guard = CACHED_HARDWARE.write().unwrap();
        *guard = Some(fresh.clone());
        fresh
    }

    /// Live query of available RAM and VRAM (fast <5ms path).
    pub fn probe_live_memory() -> (u64, u64) {
        let mut sys = System::new();
        sys.refresh_memory();
        let ram_free = sys.available_memory();

        let mut vram_free = 0u64;
        #[cfg(not(target_os = "macos"))]
        {
            if let Ok(nvml) = nvml_wrapper::Nvml::init() {
                if let Ok(device) = nvml.device_by_index(0) {
                    if let Ok(mem) = device.memory_info() {
                        vram_free = mem.free;
                    }
                }
            }
        }
        (ram_free, vram_free)
    }

    /// Deep hardware probe
    pub fn probe_system() -> SystemInfo {
        let mut sys = System::new_all();
        sys.refresh_all();

        // CPU
        let cpu_brand = sys.cpus().first().map(|c| c.brand().to_string()).unwrap_or_else(|| "Unknown CPU".to_string());
        let cpu_freq = sys.cpus().first().map(|c| c.frequency()).unwrap_or(0);
        let cpu = CpuSubsystem {
            brand: cpu_brand,
            physical_cores: sys.physical_core_count().unwrap_or(4),
            logical_threads: sys.cpus().len(),
            frequency_mhz: cpu_freq,
        };

        // Memory
        let total_mem = sys.total_memory();
        let avail_mem = sys.available_memory();
        let memory = MemorySubsystem {
            total_bytes: total_mem,
            available_bytes: avail_mem,
            total_gb: bytes_to_gib(total_mem),
            available_gb: bytes_to_gib(avail_mem),
        };

        // Storage
        let mut storage = Vec::new();
        let disks = sysinfo::Disks::new_with_refreshed_list();
        for disk in &disks {
            let mount = disk.mount_point().to_string_lossy().to_string();
            let total_gb = disk.total_space() as f64 / GIB_F64;
            let free_gb = disk.available_space() as f64 / GIB_F64;
            let is_primary = mount == "C:\\" || mount == "/";
            storage.push(StorageSubsystem {
                mount_point: mount,
                total_gb,
                free_gb,
                is_primary,
            });
        }

        // GPUs via NVML
        let mut gpus = Vec::new();
        #[cfg(not(target_os = "macos"))]
        {
            if let Ok(nvml) = nvml_wrapper::Nvml::init() {
                if let Ok(count) = nvml.device_count() {
                    for i in 0..count {
                        if let Ok(device) = nvml.device_by_index(i) {
                            let name = device.name().unwrap_or_else(|_| "NVIDIA GPU".into());
                            let mem = device.memory_info().ok();
                            let total_bytes = mem.as_ref().map(|m| m.total).unwrap_or(0);
                            let free_bytes = mem.as_ref().map(|m| m.free).unwrap_or(0);
                            let compute_cap = device.cuda_compute_capability().ok().map(|c| format!("{}.{}", c.major, c.minor));
                            let pcie_gen = device.current_pcie_link_gen().ok();
                            let pcie_width = device.current_pcie_link_width().ok();

                            gpus.push(GpuSubsystem {
                                index: i,
                                name,
                                vendor: "NVIDIA".into(),
                                vram_total_bytes: total_bytes,
                                vram_free_bytes: free_bytes,
                                vram_total_gb: bytes_to_gib(total_bytes),
                                vram_available_gb: bytes_to_gib(free_bytes),
                                compute_capability: compute_cap,
                                pcie_gen,
                                pcie_width,
                            });
                        }
                    }
                }
            }
        }

        SystemInfo {
            os_name: System::name().unwrap_or_else(|| "Unknown OS".into()),
            os_version: System::os_version().unwrap_or_default(),
            hostname: System::host_name().unwrap_or_default(),
            cpu,
            memory,
            storage,
            gpus,
        }
    }
}
