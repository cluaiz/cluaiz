use crate::hardware::{SiliconTruth, StorageSubsystem};
use sysinfo::System;

pub struct cluaizHealthChecker;

impl cluaizHealthChecker {
    /// Conducts a macro-benchmark of the entire system on first boot
    pub fn execute_initial_diagnostic(mut profile: SiliconTruth) -> SiliconTruth {
        cluaiz_shared::dev_info!("🩺 [cluaiz Health] Initiating Deep Profiling Sequence...");

        // 1. RAM Profiling via sysinfo (Lightweight)
        let mut sys = System::new();
        sys.refresh_memory();
        let total_ram_gb = sys.total_memory() as f64 / 1_073_741_824.0;
        
        profile.memory.total_capacity_gb = total_ram_gb;
        
        cluaiz_shared::dev_info!("📊 [Memory] Total: {:.1} GB Detected.", total_ram_gb);

        // 2. Storage Profiling (Lightweight Metadata Read)
        let storage_speed = Self::estimate_disk_io();
        let is_nvme = storage_speed > 1000.0;
        
        profile.storage = vec![StorageSubsystem {
            mount_point: "/".to_string(), // Universal Root for Diagnostics
            drive_type: if is_nvme { "NVMe (High-Performance)".into() } else { "SSD".into() },
            read_speed_mbps: storage_speed,
            total_gb: 512.0, // Base estimate
            free_gb: 256.0,
            is_primary_workspace: true,
            ..Default::default()
        }];

        cluaiz_shared::dev_info!("💾 [Storage] Speed Estimate: {:.1} MB/s | Type: {}", 
                 storage_speed, if is_nvme { "NVMe (Optimal)" } else { "SATA SSD" });

        profile
    }

    /// Estimates disk I/O capabilities without writing large files to avoid slowing down boot.
    /// In a deeper implementation, this reads sysfs on Linux or WMI on Windows.
    fn estimate_disk_io() -> f64 {
        let path = cluaiz_shared::environment::EnvironmentManager::current()
            .local_dir
            .join(".cluaiz_boot_bench.tmp");
        let payload = vec![0u8; 5 * 1024 * 1024]; // 5MB payload
        let start = std::time::Instant::now();
        if let Ok(mut file) = std::fs::File::create(&path) {
            use std::io::Write;
            if file.write_all(&payload).is_ok() {
                let _ = file.sync_all();
            }
        }
        let _ = std::fs::read(&path);
        let duration = start.elapsed().as_secs_f64();
        let _ = std::fs::remove_file(&path);
        
        if duration > 0.0 {
            10.0 / duration // 5MB write + 5MB read = 10MB total
        } else {
            0.0
        }
    }

    /// Runs a deep manual benchmark consisting of a 50MB disk I/O write/read test
    pub fn run_full_benchmark() {
        cluaiz_shared::dev_info!("🚀 [cluaiz Benchmark] Initiating Deep Hardware Diagnostics...");
        let start = std::time::Instant::now();
        
        let path = cluaiz_shared::environment::EnvironmentManager::current()
            .local_dir
            .join(".cluaiz_io_bench.tmp");
        let payload = vec![0u8; 50 * 1024 * 1024]; // 50MB payload
        
        if let Ok(mut file) = std::fs::File::create(&path) {
            use std::io::Write;
            if file.write_all(&payload).is_ok() {
                file.sync_all().unwrap();
            }
        }
        
        if let Ok(data) = std::fs::read(&path) {
            assert_eq!(data.len(), 50 * 1024 * 1024);
        }
        
        let duration = start.elapsed();
        let speed_mbps = (100.0) / duration.as_secs_f64(); // 50 write + 50 read
        
        let _ = std::fs::remove_file(&path);
        
        cluaiz_shared::dev_info!("✅ [cluaiz Benchmark] Storage Speed: {:.1} MB/s", speed_mbps);
        cluaiz_shared::dev_info!("✅ [cluaiz Benchmark] Complete in {:.2}s", duration.as_secs_f64());
    }
}
