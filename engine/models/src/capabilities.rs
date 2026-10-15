//! File-backed model declarations and projector inventory. Metadata is not inference proof.

use crate::{
    fetcher::asset_bundler::AssetBundler,
    prober::{
        fallback::FallbackProber,
        gguf::{GgufHeader, GgufProber},
    },
};
use anyhow::{Context, Result};
use cluaiz_core::contracts::Modality;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Missing declarations stay unknown; absence is never promoted to support.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CapabilityState {
    Supported,
    Unsupported,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InputEvidence {
    pub modality: Modality,
    pub state: CapabilityState,
    pub evidence: Vec<String>,
}

/// Profile from a companion's real header, independent of its file/folder name.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectorProfile {
    pub vision_encoder: Option<bool>,
    pub audio_encoder: Option<bool>,
    pub projector_type: Option<String>,
    pub vision_projector_type: Option<String>,
    pub audio_projector_type: Option<String>,
    pub vision_projection_dim: Option<u64>,
    pub audio_projection_dim: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectorAsset {
    pub path: PathBuf,
    pub profile: ProjectorProfile,
    /// Pairing stays unknown until loaded mtmd validates the chosen model/projector.
    pub compatibility: CapabilityState,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackageCapabilities {
    pub primary_file: PathBuf,
    pub inputs: Vec<InputEvidence>,
    pub projector: Option<ProjectorAsset>,
    pub projector_candidates: Vec<ProjectorAsset>,
    pub diagnostics: Vec<String>,
    pub asset_snapshot: Vec<PackageAssetStamp>,
    #[serde(default)]
    pub onnx_graphs: Vec<crate::prober::OnnxProbeResult>,
}

/// File-stat refresh identity; not a cryptographic content hash.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PackageAssetStamp {
    pub name: String,
    pub bytes: u64,
    pub modified_ns: Option<u64>,
}

impl PackageCapabilities {
    pub fn from_onnx(primary: &Path, dir: &Path) -> Result<Self> {
        let primary = primary.canonicalize()?;
        let dir = dir.canonicalize()?;
        if primary.parent() != Some(dir.as_path()) {
            anyhow::bail!("ONNX primary must belong to its package directory");
        }
        let mut graphs = vec![crate::prober::OnnxProber::read_signature(&primary)?];
        let mut diagnostics = Vec::new();
        for entry in std::fs::read_dir(&dir)? {
            let entry = entry?;
            let path = entry.path();
            if path == primary
                || !entry.file_type()?.is_file()
                || !path
                    .extension()
                    .is_some_and(|e| e.eq_ignore_ascii_case("onnx"))
            {
                continue;
            }
            match crate::prober::OnnxProber::read_signature(&path) {
                Ok(graph) => graphs.push(graph),
                Err(error) => diagnostics.push(format!("{}: {error}", path.display())),
            }
        }
        let audio = graphs.iter().any(|g| g.has_input("input_features"));
        let text = !audio && (graphs[0].has_input("input_ids") || graphs[0].is_tts);
        let image = graphs[0].is_vision;
        let inputs = [
            (Modality::Text, text),
            (Modality::Image, image),
            (Modality::Audio, audio),
        ]
        .into_iter()
        .map(|(modality, supported)| InputEvidence {
            modality,
            state: if supported {
                CapabilityState::Supported
            } else {
                CapabilityState::Unknown
            },
            evidence: if supported {
                vec!["ONNX declared graph input signature".into()]
            } else {
                Vec::new()
            },
        })
        .collect();
        Ok(Self {
            primary_file: primary,
            inputs,
            projector: None,
            projector_candidates: Vec::new(),
            diagnostics,
            asset_snapshot: Self::asset_snapshot(&dir),
            onnx_graphs: graphs,
        })
    }

    /// Task evidence comes from tensor roles, not a model/folder name.
    pub fn onnx_tasks(&self) -> Vec<String> {
        let Some(primary) = self.onnx_graphs.first() else {
            return Vec::new();
        };
        let mut tasks = Vec::new();
        if primary.is_tts {
            tasks.push("text-to-speech".into());
        }
        if self
            .onnx_graphs
            .iter()
            .any(|g| g.has_input("input_features"))
        {
            tasks.push("automatic-speech-recognition".into());
        }
        if primary.is_embedding {
            if primary.is_vision {
                tasks.push("image-feature-extraction".into());
            } else if primary.has_input("input_ids") {
                tasks.push("embedding".into());
            }
        }
        tasks
    }

    /// Required assets for the currently implemented graph contracts. Presence
    /// does not certify processor contents, tensor compatibility or native execution.
    pub fn missing_onnx_assets(&self) -> Vec<String> {
        let Some(dir) = self.primary_file.parent() else {
            return vec!["package_directory".into()];
        };
        let mut required = std::collections::BTreeSet::new();
        for graph in &self.onnx_graphs {
            if graph.has_input("input_ids") {
                required.insert("tokenizer.json".to_owned());
            }
            if graph.is_vision || graph.has_input("input_features") {
                required.insert("preprocessor_config.json".to_owned());
            }
            if graph
                .outputs
                .iter()
                .any(|v| v.name == "last_hidden_state" && v.dimensions.len() == 3)
                && graph.has_input("input_ids")
                && !graph.has_output("sentence_embedding")
                && !graph.is_vision
                && !graph.is_asr
            {
                required.insert("1_Pooling/config.json".to_owned());
            }
            for external in &graph.external_files {
                required.insert(external.clone());
            }
        }
        required
            .into_iter()
            .filter(|asset| {
                dir.join(asset)
                    .canonicalize()
                    .ok()
                    .is_none_or(|path| !path.starts_with(dir) || !path.is_file())
            })
            .collect()
    }
    /// Includes nested helper/config files and weights for cache invalidation.
    pub fn asset_snapshot(dir: &Path) -> Vec<PackageAssetStamp> {
        let mut assets = Vec::new();
        let mut pending = vec![(dir.to_path_buf(), 0)];
        while let Some((folder, depth)) = pending.pop() {
            let Ok(entries) = std::fs::read_dir(&folder) else {
                continue;
            };
            for entry in entries.flatten() {
                let Ok(kind) = entry.file_type() else {
                    continue;
                };
                if kind.is_dir() && depth < 8 {
                    pending.push((entry.path(), depth + 1));
                }
                if !kind.is_file() {
                    continue;
                }
                let Ok(metadata) = entry.metadata() else {
                    continue;
                };
                let path = entry.path();
                let Ok(name) = path.strip_prefix(dir) else {
                    continue;
                };
                assets.push(PackageAssetStamp {
                    name: name.to_string_lossy().replace('\\', "/"),
                    bytes: metadata.len(),
                    modified_ns: metadata
                        .modified()
                        .ok()
                        .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
                        .and_then(|duration| u64::try_from(duration.as_nanos()).ok()),
                });
            }
        }
        assets.sort_by(|a, b| a.name.cmp(&b.name));
        assets
    }
    /// Reads the primary GGUF and same-folder companion headers using the single parser.
    pub fn inspect(primary: &Path) -> Result<Self> {
        let primary = std::fs::canonicalize(primary)?;
        let dir = primary
            .parent()
            .context("Model file has no parent directory")?;
        if primary
            .extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("onnx"))
        {
            return Self::from_onnx(&primary, dir);
        }
        let header = GgufProber::read_header(&primary)?;
        Ok(Self::from_header(&primary, dir, &header))
    }

    /// Reuses a header already read by model discovery; no duplicate GGUF parser.
    pub fn from_header(primary: &Path, dir: &Path, header: &GgufHeader) -> Self {
        let config = FallbackProber::probe_directory(dir);
        let has_text = header.metadata.contains_key("tokenizer.ggml.model")
            || header
                .tensors
                .iter()
                .any(|tensor| tensor.name == "token_embd.weight");
        let inputs = vec![
            declaration(
                Modality::Text,
                has_text.then_some(true),
                None,
                "tokenizer/text embedding",
            ),
            declaration(
                Modality::Image,
                flag(header, "clip.has_vision_encoder"),
                config.vision_input,
                "clip.has_vision_encoder / vision_config",
            ),
            declaration(
                Modality::Audio,
                flag(header, "clip.has_audio_encoder"),
                config.audio_input,
                "clip.has_audio_encoder / audio_config",
            ),
        ];
        let mut diagnostics = Vec::new();
        let mut candidates = Vec::new();
        match std::fs::read_dir(dir) {
            Ok(entries) => {
                for entry in entries {
                    let entry = match entry {
                        Ok(entry) => entry,
                        Err(error) => {
                            diagnostics.push(error.to_string());
                            continue;
                        }
                    };
                    let path = entry.path();
                    if path == primary
                        || !path.is_file()
                        || path.extension().and_then(|ext| ext.to_str()) != Some("gguf")
                    {
                        continue;
                    }
                    match GgufProber::read_header(&path) {
                        Ok(companion) => {
                            let vision_encoder = flag(&companion, "clip.has_vision_encoder");
                            let audio_encoder = flag(&companion, "clip.has_audio_encoder");
                            if vision_encoder.is_none() && audio_encoder.is_none() {
                                continue;
                            }
                            let dimension = |key: &str| {
                                companion
                                    .metadata
                                    .get(key)
                                    .and_then(|v| v.parse::<u64>().ok())
                                    .filter(|value| *value > 0)
                            };
                            let profile = ProjectorProfile {
                                vision_encoder,
                                audio_encoder,
                                projector_type: companion
                                    .metadata
                                    .get("clip.projector_type")
                                    .cloned(),
                                vision_projector_type: companion
                                    .metadata
                                    .get("clip.vision.projector_type")
                                    .cloned(),
                                audio_projector_type: companion
                                    .metadata
                                    .get("clip.audio.projector_type")
                                    .cloned(),
                                vision_projection_dim: dimension("clip.vision.projection_dim"),
                                audio_projection_dim: dimension("clip.audio.projection_dim"),
                            };
                            candidates.push(ProjectorAsset {
                                path,
                                profile,
                                // clip_n_mmproj_embd derives output width from tensors
                                // differently for each projector type, not this metadata hint.
                                compatibility: CapabilityState::Unknown,
                            });
                        }
                        Err(error) => diagnostics.push(format!(
                            "Cannot inspect companion {}: {error}",
                            path.display()
                        )),
                    }
                }
            }
            Err(error) => diagnostics.push(format!("Cannot inspect model directory: {error}")),
        }
        candidates.sort_by(|a, b| a.path.cmp(&b.path));
        // Precision variants of the same profile may use the existing ranker.
        // Different encoder/projector profiles are ambiguous, not silently interchangeable.
        let projector = if let Some(first) = candidates.first() {
            if candidates
                .iter()
                .all(|candidate| candidate.profile == first.profile)
            {
                let paths: Vec<_> = candidates
                    .iter()
                    .filter_map(|candidate| candidate.path.to_str())
                    .collect();
                AssetBundler::select_best_mmproj(&paths)
                    .and_then(|path| {
                        candidates
                            .iter()
                            .find(|candidate| candidate.path == Path::new(path))
                    })
                    .cloned()
            } else {
                diagnostics.push(
                    "Multiple different projector profiles; explicit selection is required".into(),
                );
                None
            }
        } else {
            None
        };
        Self {
            inputs,
            primary_file: primary
                .canonicalize()
                .unwrap_or_else(|_| primary.to_path_buf()),
            projector,
            projector_candidates: candidates,
            diagnostics,
            asset_snapshot: Self::asset_snapshot(dir),
            onnx_graphs: Vec::new(),
        }
    }

    /// Positive intrinsic declarations only. Projector presence does not invent model support.
    pub fn supported_inputs(&self) -> Vec<Modality> {
        self.inputs
            .iter()
            .filter(|input| input.state == CapabilityState::Supported)
            .map(|input| input.modality)
            .collect()
    }
}

fn flag(header: &GgufHeader, key: &str) -> Option<bool> {
    match header.metadata.get(key).map(String::as_str) {
        Some("true") => Some(true),
        Some("false") => Some(false),
        _ => None,
    }
}

fn declaration(
    modality: Modality,
    header: Option<bool>,
    config: Option<bool>,
    source: &str,
) -> InputEvidence {
    let positive = header == Some(true) || config == Some(true);
    let negative = header == Some(false) || config == Some(false);
    let (state, reason) = match (positive, negative) {
        (true, true) => (CapabilityState::Unknown, "conflicting declarations"),
        (true, false) => (CapabilityState::Supported, "positive declaration"),
        (false, true) => (
            CapabilityState::Unsupported,
            "explicit negative declaration",
        ),
        _ => (CapabilityState::Unknown, "no declaration"),
    };
    InputEvidence {
        modality,
        state,
        evidence: vec![format!("{source}: {reason}")],
    }
}

#[cfg(test)]
#[path = "capabilities_tests.rs"]
mod tests;
