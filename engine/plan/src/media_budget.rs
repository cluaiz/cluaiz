//! Additive media preflight against live memory AFTER the text model is loaded.
//! Estimates do not reserve OS memory or measure the native encoder's peak usage.

use crate::{ram_budget_bytes, vram_buffer_options};
use anyhow::{bail, Context, Result};
use cluaiz_core::{gib_to_bytes, ActiveUserSettings};
use cluaiz_hw::SystemInfo;

/// Caller-supplied upper estimates; decoded_bytes includes simultaneously retained copies.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MediaRequirements {
    pub weights_bytes: u64,
    pub workspace_bytes: u64,
    pub decoded_bytes: u64,
}

/// Projector placement; independent of the text model's placement.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProjectorPlacement {
    Cpu,
    Gpu,
}

/// Checked estimate for one projector load, not an allocation or a transferable permit.
#[derive(Debug, Clone, Copy)]
pub struct MediaBudget {
    placement: ProjectorPlacement,
    host_bytes: u64,
    device_bytes: u64,
    device_buffer_bytes: u64,
}

impl MediaBudget {
    /// Reuses existing safety policies. Live free RAM already excludes resident model memory.
    /// GPU placement includes host staging/workspace as well as device weights/workspace.
    pub fn plan(
        needs: MediaRequirements,
        live: &SystemInfo,
        settings: &ActiveUserSettings,
        prefer_gpu: bool,
    ) -> Result<Self> {
        if needs.weights_bytes == 0 || needs.workspace_bytes == 0 {
            bail!("Projector weights and workspace estimates must be non-zero");
        }
        settings
            .media
            .limits
            .check_decoded_bytes(needs.decoded_bytes)?;
        let device_bytes = needs
            .weights_bytes
            .checked_add(needs.workspace_bytes)
            .context("Media weight/workspace estimate overflow")?;
        let host_bytes = device_bytes
            .checked_add(needs.decoded_bytes)
            .context("Media decoded-buffer estimate overflow")?;
        for value in [live.memory.total_gb, live.memory.available_gb]
            .into_iter()
            .chain(settings.optimization.custom_ram_buffer_gb)
        {
            if !value.is_finite() || value < 0.0 {
                bail!("Invalid live RAM or RAM buffer value");
            }
        }
        if live.memory.total_gb == 0.0 || live.memory.available_gb > live.memory.total_gb {
            bail!("Live RAM snapshot has an unknown total or free memory above total");
        }
        let ram_available = ram_budget_bytes(live, settings);
        if host_bytes > ram_available {
            bail!("Media needs {host_bytes} host bytes; live RAM budget is {ram_available}");
        }
        if prefer_gpu {
            let buffers = vram_buffer_options(settings);
            if buffers
                .iter()
                .any(|value| !value.is_finite() || *value < 0.0)
            {
                bail!("Invalid projector VRAM buffer value");
            }
            if let Some(gpu) = live
                .gpus
                .first()
                .filter(|gpu| gpu.vram_available_gb.is_finite() && gpu.vram_available_gb > 0.0)
            {
                let free = gib_to_bytes(gpu.vram_available_gb);
                for buffer in buffers.into_iter().map(gib_to_bytes) {
                    if device_bytes
                        .checked_add(buffer)
                        .is_some_and(|required| required <= free)
                    {
                        return Ok(Self {
                            placement: ProjectorPlacement::Gpu,
                            host_bytes,
                            device_bytes,
                            device_buffer_bytes: buffer,
                        });
                    }
                }
            }
        }
        Ok(Self {
            placement: ProjectorPlacement::Cpu,
            host_bytes,
            device_bytes: 0,
            device_buffer_bytes: 0,
        })
    }

    /// Placement chosen by the estimate, not a native driver success report.
    pub fn placement(&self) -> ProjectorPlacement {
        self.placement
    }
    /// Estimated host staging, workspace and retained decoded data.
    pub fn host_bytes(&self) -> u64 {
        self.host_bytes
    }
    /// Estimated GPU allocation, zero for CPU placement.
    pub fn device_bytes(&self) -> u64 {
        self.device_bytes
    }
    /// Existing planner's GPU safety buffer selected for this estimate.
    pub fn device_buffer_bytes(&self) -> u64 {
        self.device_buffer_bytes
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cluaiz_core::bytes_to_gib;
    use cluaiz_hw::GpuSubsystem;

    fn snapshot(ram: u64, vram: u64) -> (SystemInfo, ActiveUserSettings) {
        let mut hw = SystemInfo::default();
        hw.memory.total_gb = bytes_to_gib(ram);
        hw.memory.available_gb = bytes_to_gib(ram);
        hw.gpus.push(GpuSubsystem {
            vram_available_gb: bytes_to_gib(vram),
            ..Default::default()
        });
        let mut settings = ActiveUserSettings::default();
        settings.optimization.custom_ram_buffer_gb = Some(0.0);
        settings.optimization.custom_vram_buffer_gb = Some(bytes_to_gib(1));
        (hw, settings)
    }

    const NEEDS: MediaRequirements = MediaRequirements {
        weights_bytes: 3,
        workspace_bytes: 2,
        decoded_bytes: 1,
    };

    #[test]
    fn test_media_budget_uses_existing_buffer_policy_and_decoded_limit() {
        let (mut hw, mut settings) = snapshot(6, cluaiz_core::MIB * 512 + 5);
        settings.optimization.custom_vram_buffer_gb = None;
        let budget = MediaBudget::plan(NEEDS, &hw, &settings, true).unwrap();
        assert_eq!(budget.device_buffer_bytes(), cluaiz_core::MIB * 512);
        hw.gpus[0].vram_available_gb = bytes_to_gib(cluaiz_core::MIB * 512 + 4);
        assert_eq!(
            MediaBudget::plan(NEEDS, &hw, &settings, true)
                .unwrap()
                .placement(),
            ProjectorPlacement::Cpu
        );
        settings.media.limits = cluaiz_core::media_limits::MediaLimits::new(1, 1).unwrap();
        hw.memory.total_gb = bytes_to_gib(7);
        hw.memory.available_gb = bytes_to_gib(7);
        assert!(MediaBudget::plan(
            MediaRequirements {
                decoded_bytes: 2,
                ..NEEDS
            },
            &hw,
            &settings,
            false
        )
        .is_err());
    }

    #[test]
    fn test_media_budget_exact_boundaries_and_cpu_fallback() {
        let (mut hw, settings) = snapshot(6, 6);
        let budget = MediaBudget::plan(NEEDS, &hw, &settings, true).unwrap();
        assert_eq!(budget.placement(), ProjectorPlacement::Gpu);
        assert_eq!(
            (
                budget.host_bytes(),
                budget.device_bytes(),
                budget.device_buffer_bytes()
            ),
            (6, 5, 1)
        );
        hw.gpus[0].vram_available_gb = bytes_to_gib(5);
        assert_eq!(
            MediaBudget::plan(NEEDS, &hw, &settings, true)
                .unwrap()
                .placement(),
            ProjectorPlacement::Cpu
        );
        hw.memory.available_gb = bytes_to_gib(5);
        assert!(MediaBudget::plan(NEEDS, &hw, &settings, true).is_err());
    }

    #[test]
    fn test_media_budget_no_gpu_and_unknown_gpu_use_cpu() {
        let (mut hw, settings) = snapshot(6, 6);
        hw.gpus[0].vram_available_gb = f64::NAN;
        assert_eq!(
            MediaBudget::plan(NEEDS, &hw, &settings, true)
                .unwrap()
                .placement(),
            ProjectorPlacement::Cpu
        );
        hw.gpus.clear();
        assert_eq!(
            MediaBudget::plan(NEEDS, &hw, &settings, true)
                .unwrap()
                .placement(),
            ProjectorPlacement::Cpu
        );
    }

    #[test]
    fn test_media_budget_rejects_missing_estimates_overflow_and_bad_ram() {
        let (mut hw, settings) = snapshot(6, 6);
        for needs in [
            MediaRequirements {
                workspace_bytes: 0,
                ..NEEDS
            },
            MediaRequirements {
                weights_bytes: 0,
                ..NEEDS
            },
            MediaRequirements {
                weights_bytes: u64::MAX,
                ..NEEDS
            },
            MediaRequirements {
                decoded_bytes: u64::MAX,
                ..NEEDS
            },
        ] {
            assert!(MediaBudget::plan(needs, &hw, &settings, false).is_err());
        }
        hw.memory.available_gb = f64::NAN;
        assert!(MediaBudget::plan(NEEDS, &hw, &settings, false).is_err());
    }
}
