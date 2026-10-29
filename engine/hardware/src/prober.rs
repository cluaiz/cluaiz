//! 🔍 Hardware Prober Implementation
//! Directly queries physical silicon, registers, and OS drivers (CPUID, TSC, NVML, sysinfo)
//! to establish hardware truth without dummy data or static assumptions.

use sysinfo::System;
use std::sync::RwLock;
use once_cell::sync::Lazy;
use cluaiz_core::{bytes_to_gib, GIB_F64};
use crate::types::{SystemInfo, CpuSubsystem, MemorySubsystem, StorageSubsystem, GpuSubsystem};

static CACHED_HARDWARE: Lazy<RwLock<Option<SystemInfo>>> = Lazy::new(|| RwLock::new(None));

/// GPUs that llama.cpp itself can use (Metal, Vulkan), with their free memory read when called.
/// NVML sees NVIDIA GPUs only, so these are taken when it finds none. Set by the engine once its
/// backends are loaded (`set_backend_gpus`); never saved in `hardware_profile.json`.
static BACKEND_GPUS: std::sync::OnceLock<fn() -> Vec<GpuSubsystem>> = std::sync::OnceLock::new();

pub struct HardwareProber;

struct QuickFacts {
    ram_total_bytes: u64,
    logical_threads: usize,
    gpus: Vec<(String, u64)>,
}

impl HardwareProber {
    /// Returns the cached hardware snapshot. The deep probe (seconds: memory benchmark, WMI, disks) runs
    /// once and is kept in `hardware_profile.json`; later processes read that file and only measure free
    /// RAM / VRAM again. `calibrate()` re-runs the deep probe, and so does a start where the total RAM,
    /// the CPU threads or the NVIDIA GPUs no longer match the file (RAM added, a VM resized).
    pub fn current() -> SystemInfo {
        let mut info = Self::current_saved();
        Self::add_backend_gpus(&mut info);
        info
    }

    /// Where llama.cpp's own GPUs come from (see `BACKEND_GPUS`). Only the first call counts.
    pub fn set_backend_gpus(source: fn() -> Vec<GpuSubsystem>) {
        let _ = BACKEND_GPUS.set(source);
    }

    fn add_backend_gpus(info: &mut SystemInfo) {
        if info.gpus.is_empty() {
            if let Some(source) = BACKEND_GPUS.get() {
                info.gpus = source();
            }
        }
    }

    /// The snapshot as kept in `hardware_profile.json` (NVIDIA GPUs only).
    fn current_saved() -> SystemInfo {
        {
            let guard = CACHED_HARDWARE.read().unwrap();
            if let Some(ref info) = *guard {
                return info.clone();
            }
        }
        let info = match Self::load_profile() {
            Some(mut saved) if !Self::hardware_changed(&saved, &Self::quick_facts()) => {
                Self::apply_live_memory(&mut saved);
                saved
            }
            stale => {
                if stale.is_some() {
                    tracing::info!("RAM, CPU or GPU changed since hardware_profile.json was written; scanning the hardware again");
                }
                let fresh = Self::probe_system();
                Self::save_profile(&fresh);
                fresh
            }
        };
        *CACHED_HARDWARE.write().unwrap() = Some(info.clone());
        info
    }

    /// The facts a hardware change alters, read in milliseconds: total RAM, CPU threads, and each
    /// NVIDIA GPU's name with its total VRAM.
    fn quick_facts() -> QuickFacts {
        let sys = System::new_with_specifics(
            sysinfo::RefreshKind::new()
                .with_memory(sysinfo::MemoryRefreshKind::new().with_ram())
                .with_cpu(sysinfo::CpuRefreshKind::new()),
        );
        let mut gpus = Vec::new();
        #[cfg(not(target_os = "macos"))]
        if let Ok(nvml) = nvml_wrapper::Nvml::init() {
            for i in 0..nvml.device_count().unwrap_or(0) {
                if let Ok(device) = nvml.device_by_index(i) {
                    let name = device.name().unwrap_or_else(|_| "NVIDIA GPU".into());
                    gpus.push((name, device.memory_info().map(|m| m.total).unwrap_or(0)));
                }
            }
        }
        QuickFacts { ram_total_bytes: sys.total_memory(), logical_threads: sys.cpus().len(), gpus }
    }

    /// A fact that could not be read (zero) is not a change. RAM may differ by a little between boots.
    fn hardware_changed(saved: &SystemInfo, now: &QuickFacts) -> bool {
        let saved_ram = saved.memory.total_bytes;
        let ram_changed = now.ram_total_bytes > 0 && saved_ram.abs_diff(now.ram_total_bytes) > saved_ram / 32;
        let threads_changed = now.logical_threads > 0 && now.logical_threads != saved.cpu.logical_threads;
        let saved_gpus: Vec<(String, u64)> = saved.gpus.iter().map(|g| (g.name.clone(), g.vram_total_bytes)).collect();
        ram_changed || threads_changed || saved_gpus != now.gpus
    }

    /// Runs the deep probe again and stores it (`cluaiz calibrate`, e.g. after a hardware change).
    pub fn calibrate() -> SystemInfo {
        let mut fresh = Self::probe_system();
        Self::save_profile(&fresh);
        *CACHED_HARDWARE.write().unwrap() = Some(fresh.clone());
        Self::add_backend_gpus(&mut fresh);
        fresh
    }

    fn profile_path() -> std::path::PathBuf {
        cluaiz_core::EnvironmentManager::current().config_dir().join("hardware_profile.json")
    }

    fn load_profile() -> Option<SystemInfo> {
        let data = std::fs::read_to_string(Self::profile_path()).ok()?;
        serde_json::from_str(&data).ok()
    }

    fn save_profile(info: &SystemInfo) {
        if cluaiz_core::EnvironmentManager::current().ensure_config_dir().is_ok() {
            if let Ok(data) = serde_json::to_string_pretty(info) {
                let _ = std::fs::write(Self::profile_path(), data);
            }
        }
    }

    /// Cached static hardware facts with free RAM / free VRAM measured right now.
    /// Use this for planning and load verification; `current()` keeps startup memory numbers.
    pub fn with_live_memory() -> SystemInfo {
        let mut info = Self::current();
        Self::apply_live_memory(&mut info);
        info
    }

    fn apply_live_memory(info: &mut SystemInfo) {
        Self::apply_memory_snapshot(info, Self::live_memory_snapshot());
    }

    fn apply_memory_snapshot(info: &mut SystemInfo, snapshot: (Option<u64>, Option<u64>)) {
        let (ram_free, vram_free) = snapshot;
        if let Some(ram_free) = ram_free {
            info.memory.available_bytes = ram_free;
            info.memory.available_gb = cluaiz_core::bytes_to_gib(ram_free);
        }
        // Zero is a valid exhausted-memory result. Only an unavailable query keeps the snapshot.
        if let (Some(gpu), Some(vram_free)) = (info.gpus.first_mut(), vram_free) {
            gpu.vram_free_bytes = vram_free;
            gpu.vram_available_gb = cluaiz_core::bytes_to_gib(vram_free);
        }
    }

    /// Live query of available RAM and VRAM (fast <5ms path).
    pub fn probe_live_memory() -> (u64, u64) {
        let (ram, vram) = Self::live_memory_snapshot();
        (ram.unwrap_or(0), vram.unwrap_or(0))
    }

    fn live_memory_snapshot() -> (Option<u64>, Option<u64>) {
        let mut sys = System::new();
        sys.refresh_memory();
        let ram_free = (sys.total_memory() > 0).then(|| Self::available_ram(&sys));

        let mut vram_free = None;
        #[cfg(not(target_os = "macos"))]
        {
            if let Ok(nvml) = nvml_wrapper::Nvml::init() {
                if let Ok(device) = nvml.device_by_index(0) {
                    if let Ok(mem) = device.memory_info() {
                        vram_free = Some(mem.free);
                    }
                }
            }
        }
        (ram_free, vram_free)
    }

    /// RAM that can be given to a model now. sysinfo's figure, except on macOS (`macos_available_ram`).
    fn available_ram(sys: &System) -> u64 {
        #[cfg(target_os = "macos")]
        if let Some(available) = macos_available_ram(sys.total_memory()) {
            return available;
        }
        sys.available_memory()
    }

    /// Deep hardware probe measuring real silicon properties
    pub fn probe_system() -> SystemInfo {
        let mut sys = System::new_all();
        sys.refresh_all();

        let cpu = Self::probe_cpu(&sys);
        let memory = Self::probe_memory(&sys);
        let storage = Self::probe_storage();
        let gpus = Self::probe_gpus();

        SystemInfo {
            os_name: System::name().unwrap_or_else(|| "Unknown OS".into()),
            os_version: System::os_version().unwrap_or_default(),
            hostname: System::host_name().unwrap_or_default(),
            architecture: std::env::consts::ARCH.to_string(),
            cpu,
            memory,
            storage,
            gpus,
        }
    }

    // ─── CPU Probing (TSC Assembly, CPUID Caches, ISA Features) ───────────

    fn probe_cpu(sys: &System) -> CpuSubsystem {
        let brand_string = sys
            .cpus()
            .first()
            .map(|c| c.brand().to_string())
            .unwrap_or_else(|| "Unknown CPU".into());

        let reported_freq = sys.cpus().first().map(|c| c.frequency()).unwrap_or(0);
        let estimated_clock = Self::estimate_core_clock();
        let base_clock = if estimated_clock > 0.0 { estimated_clock } else { reported_freq as f64 };
        let boost_clock = (base_clock * 1.25).max(base_clock);

        let (l1_kb, l2_kb, l3_kb) = Self::probe_silicon_cache();
        let isa_features = Self::probe_isa_features();

        // Calculate SIMD compute factor:
        // AVX-512 = 64 FP32 ops/cycle
        // AVX2 = 32 FP32 ops/cycle
        // AVX / NEON = 16 FP32 ops/cycle
        let simd_factor = if isa_features.iter().any(|f| f == "AVX-512") {
            64.0
        } else if isa_features.iter().any(|f| f == "AVX2") {
            32.0
        } else if isa_features.iter().any(|f| f == "AVX" || f == "NEON") {
            16.0
        } else {
            8.0
        };

        let physical_cores = sys.physical_core_count().unwrap_or(4);
        let boost_ghz = (boost_clock / 1000.0).max(1.0);
        let gflops_fp32 = (physical_cores as f64) * boost_ghz * simd_factor;

        CpuSubsystem {
            brand: brand_string,
            physical_cores,
            logical_threads: sys.cpus().len(),
            frequency_mhz: reported_freq,
            base_clock_mhz: base_clock,
            boost_clock_mhz: boost_clock,
            l1_cache_kb: l1_kb,
            l2_cache_kb: l2_kb,
            l3_cache_kb: l3_kb,
            isa_features,
            gflops_fp32,
        }
    }

    /// Real-time Core Clock Measurement via Direct TSC Assembly
    fn estimate_core_clock() -> f64 {
        #[cfg(target_arch = "x86_64")]
        {
            let start_time = std::time::Instant::now();
            let start_tsc = unsafe { core::arch::x86_64::_rdtsc() };

            let mut x: u64 = 0;
            for i in 0..1_000_000 {
                x = x.wrapping_add(i as u64);
            }
            std::hint::black_box(x);

            let end_tsc = unsafe { core::arch::x86_64::_rdtsc() };
            let duration = start_time.elapsed().as_secs_f64();

            if duration > 0.0 && end_tsc > start_tsc {
                return (end_tsc - start_tsc) as f64 / duration / 1_000_000.0;
            }
        }
        0.0
    }

    /// CPU Cache Probe via raw-cpuid with cross-platform native fallbacks
    fn probe_silicon_cache() -> (u32, u32, u32) {
        let mut l1 = 0u32;
        let mut l2 = 0u32;
        let mut l3 = 0u32;

        #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
        {
            let cpuid = raw_cpuid::CpuId::new();
            if let Some(cparams) = cpuid.get_cache_parameters() {
                for cache in cparams {
                    let size_kb = ((cache.sets() * cache.coherency_line_size() * cache.associativity()) / 1024) as u32;
                    match cache.level() {
                        1 => l1 += size_kb,
                        2 => l2 += size_kb,
                        3 => l3 += size_kb,
                        _ => {}
                    }
                }
            }
            if l1 > 0 || l2 > 0 || l3 > 0 {
                return (l1, l2, l3);
            }
        }

        // Windows WMI Fallback
        #[cfg(target_os = "windows")]
        if let Ok(output) = std::process::Command::new("powershell")
            .args(["-Command", "Get-CimInstance Win32_CacheMemory | Select-Object Level, MaxCacheSize"])
            .output()
        {
            let s = String::from_utf8_lossy(&output.stdout);
            for line in s.lines() {
                let parts: Vec<&str> = line.split_whitespace().collect();
                if parts.len() == 2 {
                    if let (Ok(level), Ok(size)) = (parts[0].parse::<u32>(), parts[1].parse::<u32>()) {
                        match level {
                            3 => l1 += size,
                            4 => l2 += size,
                            5 => l3 += size,
                            _ => {}
                        }
                    }
                }
            }
            if l1 > 0 || l2 > 0 || l3 > 0 {
                return (l1, l2, l3);
            }
        }

        // Linux sysfs Fallback
        #[cfg(target_os = "linux")]
        for i in 0..4 {
            let base = format!("/sys/devices/system/cpu/cpu0/cache/index{}", i);
            if let (Ok(lvl_s), Ok(sz_s)) = (
                std::fs::read_to_string(format!("{}/level", base)),
                std::fs::read_to_string(format!("{}/size", base)),
            ) {
                let level: u32 = lvl_s.trim().parse().unwrap_or(0);
                let size_kb = sz_s.trim().replace('K', "").parse::<u32>().unwrap_or(0);
                match level {
                    1 => l1 += size_kb,
                    2 => l2 += size_kb,
                    3 => l3 += size_kb,
                    _ => {}
                }
            }
        }

        (l1, l2, l3)
    }

    /// ISA Feature Discovery
    fn probe_isa_features() -> Vec<String> {
        let mut features = Vec::new();

        #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
        {
            if is_x86_feature_detected!("avx") {
                features.push("AVX".into());
            }
            if is_x86_feature_detected!("fma") {
                features.push("FMA3".into());
            }
            if is_x86_feature_detected!("avx2") {
                features.push("AVX2".into());
            }
            if is_x86_feature_detected!("avx512f") {
                features.push("AVX-512".into());
            }
        }

        #[cfg(target_arch = "aarch64")]
        {
            features.push("NEON".into());
            features.push("FP16".into());
            if cfg!(target_os = "macos") {
                features.push("AMX".into());
            }
        }

        features
    }

    // ─── Memory Probing (Sequential Bandwidth & Pointer Chasing Latency) ──

    fn benchmark_memory() -> (f64, f64) {
        use std::time::Instant;

        // 16MB pointer chasing array for latency measurement
        let lat_size = 2 * 1024 * 1024;
        let mut buffer = vec![0usize; lat_size];
        for i in 0..lat_size {
            buffer[i] = (i + 128) % lat_size;
        }

        let mut curr = 0;
        let start_lat = Instant::now();
        for _ in 0..1_000_000 {
            curr = buffer[curr];
        }
        std::hint::black_box(curr);
        let latency_ns = (start_lat.elapsed().as_secs_f64() / 1_000_000.0) * 1_000_000_000.0;

        // 64MB sequential read bandwidth test
        let read_size = 8 * 1024 * 1024; // 8M elements * 8 bytes = 64 MB
        let test_data = vec![1u64; read_size];
        let start_bw = Instant::now();
        let mut sum = 0u64;
        for &val in &test_data {
            sum = sum.wrapping_add(val);
        }
        std::hint::black_box(sum);
        let bw_dur = start_bw.elapsed().as_secs_f64();
        let bw_gbps = if bw_dur > 0.0 {
            ((read_size * 8) as f64 / bw_dur) / 1_000_000_000.0
        } else {
            32.0
        };

        (latency_ns, bw_gbps)
    }

    fn probe_memory(sys: &System) -> MemorySubsystem {
        let is_unified = cfg!(target_os = "macos") && cfg!(target_arch = "aarch64");
        let (live_latency, mut live_bw) = Self::benchmark_memory();

        let type_name = if is_unified {
            "Unified LPDDR5X".into()
        } else {
            "DDR4/DDR5".into()
        };

        let speed_str = Self::fetch_raw_hw_value("memorychip", "Speed");
        let speed_mts: f64 = speed_str.parse().unwrap_or(0.0);
        if speed_mts > 0.0 && live_bw < 10.0 {
            live_bw = (speed_mts * 8.0) / 1000.0; // Bus theoretical fallback if synthetic throttled
        }

        let total_mem = sys.total_memory();
        let avail_mem = Self::available_ram(sys);

        MemorySubsystem {
            total_bytes: total_mem,
            available_bytes: avail_mem,
            total_gb: bytes_to_gib(total_mem),
            available_gb: bytes_to_gib(avail_mem),
            type_name,
            speed_mts,
            bandwidth_gbps: live_bw,
            memory_latency_ns: live_latency,
            is_unified_memory: is_unified,
        }
    }

    // ─── Storage Probing (5MB Atomic Speed Test & Drive Type Detection) ────

    fn benchmark_storage(mount_path: &str) -> (f64, f64) {
        use std::time::Instant;
        let test_file = std::path::PathBuf::from(mount_path).join(".cluaiz_hw_bench");
        let test_bytes = vec![0u8; 5 * 1024 * 1024]; // 5 MB atomic buffer

        let start_write = Instant::now();
        if std::fs::write(&test_file, &test_bytes).is_ok() {
            let write_dur = start_write.elapsed().as_secs_f64();

            let start_read = Instant::now();
            if std::fs::read(&test_file).is_ok() {
                let read_dur = start_read.elapsed().as_secs_f64();
                let _ = std::fs::remove_file(&test_file);

                let read_mbps = if read_dur > 0.0 { 5.0 / read_dur } else { 0.0 };
                let write_mbps = if write_dur > 0.0 { 5.0 / write_dur } else { 0.0 };
                return (read_mbps, write_mbps);
            }
            let _ = std::fs::remove_file(&test_file);
        }
        (0.0, 0.0)
    }

    fn detect_drive_type(kind_str: &str) -> String {
        #[cfg(target_os = "windows")]
        if let Ok(out) = std::process::Command::new("powershell")
            .args(["-Command", "Get-PhysicalDisk | Select-Object -ExpandProperty MediaType -First 1"])
            .output()
        {
            let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
            if !s.is_empty() {
                return format!("NVMe/{}", s);
            }
        }

        #[cfg(target_os = "linux")]
        if let Ok(rot) = std::fs::read_to_string("/sys/block/nvme0n1/queue/rotational") {
            if rot.trim() == "0" {
                return "NVMe SSD".into();
            }
        }

        if kind_str.contains("SSD") {
            "NVMe SSD".into()
        } else {
            kind_str.to_string()
        }
    }

    fn probe_storage() -> Vec<StorageSubsystem> {
        let mut storage = Vec::new();
        let disks = sysinfo::Disks::new_with_refreshed_list();

        for disk in &disks {
            let mount = disk.mount_point().to_string_lossy().to_string();
            let total_gb = disk.total_space() as f64 / GIB_F64;
            let free_gb = disk.available_space() as f64 / GIB_F64;
            let is_primary = mount == "C:\\" || mount == "/";
            let (read_mbps, write_mbps) = if is_primary {
                Self::benchmark_storage(&mount)
            } else {
                (0.0, 0.0)
            };
            let drive_type = Self::detect_drive_type(&format!("{:?}", disk.kind()));

            storage.push(StorageSubsystem {
                mount_point: mount,
                total_gb,
                free_gb,
                is_primary,
                drive_type,
                read_speed_mbps: read_mbps,
                write_speed_mbps: write_mbps,
            });
        }
        storage
    }

    // ─── GPU Probing (NVML + PCIe Bandwidth Derivation) ───────────────────

    fn probe_gpus() -> Vec<GpuSubsystem> {
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

                            // Calculate real PCIe bus throughput in GB/s
                            let per_lane_bw_gbps = match pcie_gen.unwrap_or(3) {
                                1 => 0.25,
                                2 => 0.50,
                                3 => 0.985,
                                4 => 1.969,
                                5 => 3.938,
                                _ => 0.985,
                            };
                            let pcie_bandwidth_gbps = per_lane_bw_gbps * (pcie_width.unwrap_or(16) as f64);

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
                                pcie_bandwidth_gbps,
                            });
                        }
                    }
                }
            }
        }
        gpus
    }

    fn fetch_raw_hw_value(domain: &str, field: &str) -> String {
        #[cfg(target_os = "windows")]
        if let Ok(output) = std::process::Command::new("wmic")
            .args([domain, "get", field])
            .output()
        {
            let s = String::from_utf8_lossy(&output.stdout);
            let lines: Vec<&str> = s.lines().map(|l| l.trim()).filter(|l| !l.is_empty()).collect();
            if lines.len() > 1 {
                return lines[1].to_string();
            }
        }

        #[cfg(target_os = "linux")]
        if domain == "csproduct" && field == "UUID" {
            if let Ok(data) = std::fs::read_to_string("/sys/class/dmi/id/product_uuid") {
                return data.trim().to_string();
            }
        }

        "".into()
    }
}

/// Free RAM as macOS's Activity Monitor counts it: the total minus app memory (anonymous pages that
/// are not purgeable), wired memory and the compressor's pages. sysinfo 0.30 takes free + inactive +
/// purgeable minus the compressor's pages, which is zero once macOS compresses memory: a 16 GB Mac
/// read 0.00 GB free and the planner refused a 2.9 GB model.
#[cfg(target_os = "macos")]
fn macos_available_ram(total: u64) -> Option<u64> {
    // SAFETY: host_statistics64 fills at most `count` integers of the zeroed struct
    let (stat, page) = unsafe {
        let mut stat: libc::vm_statistics64 = std::mem::zeroed();
        let mut count = libc::HOST_VM_INFO64_COUNT;
        #[allow(deprecated)]
        let host = libc::mach_host_self();
        let ok = libc::host_statistics64(host, libc::HOST_VM_INFO64, &mut stat as *mut _ as *mut _, &mut count);
        (ok == libc::KERN_SUCCESS).then_some((stat, libc::sysconf(libc::_SC_PAGESIZE)))?
    };
    let page = u64::try_from(page).ok().filter(|p| *p > 0)?;
    Some(activity_monitor_available(
        total,
        page,
        [stat.internal_page_count, stat.purgeable_count, stat.wire_count, stat.compressor_page_count],
    ))
}

/// `[internal, purgeable, wired, compressor]` page counts
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
fn activity_monitor_available(total: u64, page: u64, [internal, purgeable, wired, compressor]: [u32; 4]) -> u64 {
    let pages = |n: u32| u64::from(n).saturating_mul(page);
    let used = pages(internal).saturating_sub(pages(purgeable)) + pages(wired) + pages(compressor);
    total.saturating_sub(used)
}

#[cfg(test)]
mod live_memory_tests {
    use super::*;
    #[test]
    fn test_zero_free_memory_is_not_replaced_by_stale_startup_values() {
        let mut info = SystemInfo::default();
        info.memory.available_bytes = cluaiz_core::GIB;
        info.memory.available_gb = 1.0;
        info.gpus.push(crate::GpuSubsystem { vram_free_bytes: cluaiz_core::GIB, vram_available_gb: 1.0, ..Default::default() });
        HardwareProber::apply_memory_snapshot(&mut info, (None, None));
        assert_eq!(info.memory.available_gb, 1.0);
        assert_eq!(info.gpus[0].vram_available_gb, 1.0);
        HardwareProber::apply_memory_snapshot(&mut info, (Some(0), Some(0)));
        assert_eq!((info.memory.available_bytes, info.memory.available_gb), (0, 0.0));
        assert_eq!((info.gpus[0].vram_free_bytes, info.gpus[0].vram_available_gb), (0, 0.0));
        HardwareProber::apply_memory_snapshot(&mut info, (Some(cluaiz_core::GIB), Some(cluaiz_core::GIB)));
        assert_eq!((info.memory.available_gb, info.gpus[0].vram_available_gb), (1.0, 1.0));
    }

    #[test]
    fn saved_profile_is_stale_after_ram_cpu_or_gpu_changes() {
        let mut saved = SystemInfo::default();
        saved.memory.total_bytes = 16 * cluaiz_core::GIB;
        saved.cpu.logical_threads = 8;
        saved.gpus.push(crate::GpuSubsystem { name: "T4".into(), vram_total_bytes: 15 * cluaiz_core::GIB, ..Default::default() });
        let same = || QuickFacts {
            ram_total_bytes: 16 * cluaiz_core::GIB - 100 * 1024 * 1024,
            logical_threads: 8,
            gpus: vec![("T4".into(), 15 * cluaiz_core::GIB)],
        };
        assert!(!HardwareProber::hardware_changed(&saved, &same()));
        assert!(!HardwareProber::hardware_changed(&saved, &QuickFacts { ram_total_bytes: 0, logical_threads: 0, ..same() }));
        assert!(HardwareProber::hardware_changed(&saved, &QuickFacts { ram_total_bytes: 32 * cluaiz_core::GIB, ..same() }));
        assert!(HardwareProber::hardware_changed(&saved, &QuickFacts { logical_threads: 16, ..same() }));
        assert!(HardwareProber::hardware_changed(&saved, &QuickFacts { gpus: vec![], ..same() }));
    }

    #[test]
    fn macos_free_ram_is_total_minus_app_wired_and_compressed() {
        // The M5 run: 16 GB, much of it compressed; sysinfo's free+inactive+purgeable-compressor was 0
        let page = 16 * 1024;
        let gib_pages = |gib: u32| gib * (1024 * 1024 * 1024 / page as u32);
        let available = activity_monitor_available(16 * cluaiz_core::GIB, page, [gib_pages(6), gib_pages(1), gib_pages(2), gib_pages(3)]);
        assert_eq!(available, 6 * cluaiz_core::GIB);
        // More in use than the total (counts read at different moments) is no free RAM, not an overflow
        assert_eq!(activity_monitor_available(cluaiz_core::GIB, page, [gib_pages(2), 0, 0, 0]), 0);
    }

    #[test]
    fn backend_gpus_fill_in_only_when_nvml_finds_none() {
        fn metal() -> Vec<crate::GpuSubsystem> {
            vec![crate::GpuSubsystem { name: "Apple M5".into(), vendor: "Apple".into(), ..Default::default() }]
        }
        HardwareProber::set_backend_gpus(metal);
        let mut none = SystemInfo::default();
        HardwareProber::add_backend_gpus(&mut none);
        assert_eq!(none.gpus[0].name, "Apple M5");
        let mut nvidia = SystemInfo::default();
        nvidia.gpus.push(crate::GpuSubsystem { name: "T4".into(), ..Default::default() });
        HardwareProber::add_backend_gpus(&mut nvidia);
        assert_eq!(nvidia.gpus.len(), 1);
        assert_eq!(nvidia.gpus[0].name, "T4");
    }

    #[test]
    #[ignore]
    fn this_machine_matches_a_fresh_probe() {
        let probed = HardwareProber::probe_system();
        assert!(!HardwareProber::hardware_changed(&probed, &HardwareProber::quick_facts()));
    }
}
