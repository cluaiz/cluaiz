//! Compose existing whole-package host admission and backbone placement without sharing chat state.
use anyhow::{bail, Context, Result};
use cluaiz_core::{bytes_to_gib, gib_to_bytes, ActiveUserSettings};
use cluaiz_hw::SystemInfo;
use cluaiz_models::ModelGeometry;
use cluaiz_plan::{
    media_budget::{MediaBudget, MediaRequirements, ProjectorPlacement},
    Planner,
};
use llama_engine::native::speech::{backbone_workspace_bytes, planning_settings, SpeechPlacement};

#[derive(Debug)]
pub(crate) struct SpeechPlan {
    pub placement: SpeechPlacement,
    pub host_bytes: u64,
    pub device_bytes: u64,
}

pub(crate) fn select_placement(
    geometry: &ModelGeometry,
    codec_bytes: u64,
    decoded_bytes: u64,
    live: &SystemInfo,
    settings: &ActiveUserSettings,
) -> Result<SpeechPlan> {
    let codec_workspace = settings.media.projector_workspace_bytes.get();
    let needs = MediaRequirements {
        weights_bytes: geometry
            .file_size_bytes
            .checked_add(codec_bytes)
            .context("Speech weights overflow")?,
        workspace_bytes: backbone_workspace_bytes(geometry)?
            .checked_add(codec_workspace)
            .context("Speech workspace overflow")?,
        decoded_bytes,
    };
    // Keep the complete host staging/workspace check even for split or GPU placement.
    let host = MediaBudget::plan(needs, live, settings, false)?;
    let captured = planning_settings(settings);
    let (threads, threads_batch) = settings
        .optimization
        .resolved_threads(live.cpu.physical_cores);
    for prefer_codec_gpu in [true, false] {
        let codec = MediaBudget::plan(
            MediaRequirements {
                weights_bytes: codec_bytes,
                workspace_bytes: codec_workspace,
                decoded_bytes,
            },
            live,
            settings,
            prefer_codec_gpu && settings.optimization.n_gpu_layers != Some(0),
        )?;
        let projector_gpu = codec.placement() == ProjectorPlacement::Gpu;
        let mut remaining = live.clone();
        if let Some(gpu) = remaining.gpus.first_mut() {
            gpu.vram_available_gb = bytes_to_gib(
                gib_to_bytes(gpu.vram_available_gb).saturating_sub(codec.device_bytes()),
            );
        }
        let mut backbone = geometry.clone();
        // The speech owner always creates this exact context, including for shorter model metadata.
        backbone.context_length = captured
            .optimization
            .custom_context_window
            .context("Speech context missing")?;
        let plan = Planner::create_plan("speech backbone", &backbone, &remaining, &captured);
        let free = remaining
            .gpus
            .first()
            .map_or(0, |gpu| gib_to_bytes(gpu.vram_available_gb));
        // The generic planner permits manual overcommit; speech admission must still refuse it.
        let fits_device = plan.n_gpu_layers == 0
            || gib_to_bytes(plan.vram_budget_gb)
                .checked_add(gib_to_bytes(plan.vram_buffer_gb))
                .is_some_and(|bytes| bytes <= free);
        let requested_gpu = settings.optimization.n_gpu_layers.is_some_and(|n| n != 0);
        if plan.is_feasible()
            && plan.disk_stream_gb == 0.0
            && plan.n_cpu_moe == 0
            && fits_device
            && (!requested_gpu || plan.n_gpu_layers != 0)
        {
            return Ok(SpeechPlan {
                placement: SpeechPlacement {
                    n_gpu_layers: plan.n_gpu_layers,
                    projector_gpu,
                    offload_kqv: plan.kv_offload,
                    op_offload: plan.op_offload,
                    threads,
                    threads_batch,
                    use_mmap: settings.optimization.resolved_use_mmap(),
                },
                host_bytes: host.host_bytes(),
                device_bytes: gib_to_bytes(plan.vram_budget_gb)
                    .checked_add(codec.device_bytes())
                    .context("Speech device estimate overflow")?,
            });
        }
    }
    // Speech applies whole-layer placement, not the generic planner's expert tensor overrides.
    if settings.optimization.n_gpu_layers.is_none() {
        let mut cpu = settings.clone();
        cpu.optimization.n_gpu_layers = Some(0);
        return select_placement(geometry, codec_bytes, decoded_bytes, live, &cpu);
    }
    bail!("Requested speech placement exceeds the current shared memory budget; unload another model or select CPU/Auto")
}

#[cfg(test)]
mod tests {
    use super::*;
    use cluaiz_core::{GIB, MIB};
    use cluaiz_models::gguf::WeightBytes;

    fn fixture(vram: u64) -> (ModelGeometry, SystemInfo, ActiveUserSettings) {
        let geometry = ModelGeometry {
            context_length: 8192,
            block_count: 4,
            head_count_kv: 1,
            key_length: 64,
            value_length: 64,
            file_size_bytes: GIB,
            weights: WeightBytes {
                total: GIB,
                input_cpu: MIB * 128,
                output: MIB * 128,
                layer: vec![MIB * 192; 4],
                layer_experts: vec![0; 4],
                ..Default::default()
            },
            ..Default::default()
        };
        let mut live = SystemInfo::default();
        live.memory.total_gb = 16.0;
        live.memory.available_gb = 12.0;
        live.cpu.physical_cores = 8;
        live.gpus.push(cluaiz_hw::GpuSubsystem {
            vram_available_gb: bytes_to_gib(vram),
            ..Default::default()
        });
        let mut settings = ActiveUserSettings::default();
        settings.optimization.custom_vram_buffer_gb = Some(0.5);
        settings.media.projector_workspace_bytes = std::num::NonZeroU64::new(MIB * 512).unwrap();
        (geometry, live, settings)
    }

    #[test]
    fn test_speech_plan_full_split_codec_cpu_and_explicit_cpu() -> Result<()> {
        let (g, mut live, mut settings) = fixture(4 * GIB);
        let full = select_placement(&g, MIB * 512, 0, &live, &settings)?;
        assert_eq!(full.placement.n_gpu_layers, -1);
        assert!(full.placement.projector_gpu);
        live.gpus[0].vram_available_gb = 2.0;
        let split = select_placement(&g, MIB * 512, 0, &live, &settings)?;
        assert!(split.placement.projector_gpu);
        assert!(
            split.placement.n_gpu_layers > 0
                && split.placement.n_gpu_layers < g.block_count as i32 + 1
        );
        assert!(split.device_bytes + MIB * 512 <= 2 * GIB);
        live.gpus[0].vram_available_gb = 1.625;
        let codec_only = select_placement(&g, MIB * 512, 0, &live, &settings)?;
        assert_eq!(codec_only.placement.n_gpu_layers, 0);
        assert!(codec_only.placement.projector_gpu);
        live.gpus[0].vram_available_gb = 1.25;
        let codec_cpu = select_placement(&g, MIB * 512, 0, &live, &settings)?;
        assert!(!codec_cpu.placement.projector_gpu && codec_cpu.placement.n_gpu_layers > 0);
        settings.optimization.n_gpu_layers = Some(0);
        settings.optimization.n_threads = Some(3);
        settings.optimization.n_threads_batch = Some(4);
        let cpu = select_placement(&g, MIB * 512, 0, &live, &settings)?.placement;
        assert_eq!(
            (cpu.n_gpu_layers, cpu.threads, cpu.threads_batch),
            (0, 3, 4)
        );
        assert!(!cpu.projector_gpu);
        live.gpus.clear();
        settings.optimization.n_gpu_layers = Some(3);
        assert!(select_placement(&g, MIB * 512, 0, &live, &settings).is_err());
        Ok(())
    }

    #[test]
    fn test_speech_plan_manual_gpu_refuses_overcommit_and_keeps_host_guard() -> Result<()> {
        let (g, mut live, mut settings) = fixture(2 * GIB);
        settings.optimization.n_gpu_layers = Some(3);
        let manual = select_placement(&g, MIB * 512, 0, &live, &settings)?.placement;
        assert_eq!(manual.n_gpu_layers, 3);
        live.gpus[0].vram_available_gb = 0.5;
        assert!(select_placement(&g, MIB * 512, 0, &live, &settings).is_err());
        settings.optimization.n_gpu_layers = None;
        let cpu = select_placement(&g, MIB * 512, 0, &live, &settings)?.placement;
        assert_eq!(cpu.n_gpu_layers, 0);
        assert!(!cpu.projector_gpu);
        live.memory.available_gb = 1.0;
        assert!(select_placement(&g, MIB * 512, 0, &live, &settings).is_err());
        live.memory.available_gb = f64::NAN;
        assert!(select_placement(&g, MIB * 512, 0, &live, &settings).is_err());
        Ok(())
    }

    #[test]
    fn test_speech_plan_matches_native_layout_flags_and_decoded_ceiling() -> Result<()> {
        let (g, live, mut settings) = fixture(4 * GIB);
        settings.optimization.custom_context_window = Some(32768);
        settings.optimization.batch_size = Some(32);
        settings.optimization.kv_cache_quantization = cluaiz_core::KvCacheQuantization::Kv8;
        settings.optimization.offload_kqv = Some(false);
        settings.optimization.op_offload = Some(false);
        let captured = planning_settings(&settings);
        assert_eq!(captured.optimization.custom_context_window, Some(4096));
        assert_eq!(captured.optimization.resolved_ubatch_size(), 512);
        assert_eq!(
            captured.optimization.kv_cache_quantization,
            cluaiz_core::KvCacheQuantization::Kv16
        );
        let placement = select_placement(&g, MIB * 512, 0, &live, &settings)?.placement;
        assert!(!placement.offload_kqv && !placement.op_offload);
        settings.media.limits = cluaiz_core::media_limits::MediaLimits::new(1, 1)?;
        assert!(select_placement(&g, MIB * 512, 2, &live, &settings).is_err());
        Ok(())
    }

    #[test]
    fn test_speech_plan_never_emits_unapplied_expert_offload() -> Result<()> {
        let (mut geometry, live, mut settings) = fixture(cluaiz_core::gib_to_bytes(1.6));
        geometry.weights.layer_experts[0] = GIB;
        geometry.weights.total += GIB;
        geometry.file_size_bytes += GIB;
        settings.media.projector_workspace_bytes = std::num::NonZeroU64::new(64 * MIB).unwrap();
        let generic =
            Planner::create_plan("fixture", &geometry, &live, &planning_settings(&settings));
        assert!(
            generic.n_cpu_moe > 0,
            "Fixture must exercise a real expert-offload candidate"
        );
        let speech = select_placement(&geometry, 64 * MIB, 0, &live, &settings)?;
        assert_eq!(speech.placement.n_gpu_layers, 0);
        assert!(!speech.placement.projector_gpu);
        Ok(())
    }
}
