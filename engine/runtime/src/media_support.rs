//! Effective request support: declarations, assets and actual wired backend routes.

use cluaiz_core::contracts::Modality;
use cluaiz_models::capabilities::{CapabilityState, PackageCapabilities};
use serde::Serialize;
use std::path::Path;

/// Wired native request routes; actual media availability still requires loaded pairing evidence.
pub const CURRENT_NATIVE_CHAT_INPUTS: &[Modality] =
    &[Modality::Text, Modality::Image, Modality::Audio];

/// A genuinely enabled attachment route, supplied by its request/provider owner.
pub struct AttachmentRoute<'a> {
    pub format: &'a str,
    pub input: Modality,
}

/// Trusted loaded-backend pairing; never deserialize this from a user request.
#[derive(Clone, Copy)]
pub struct ValidatedPairing<'a> {
    pub model: &'a Path,
    pub projector: &'a Path,
}

#[derive(Debug, Serialize)]
pub struct InputAvailability {
    pub input: Modality,
    pub model_support: CapabilityState,
    pub available: bool,
    pub reason: Option<&'static str>,
}

#[derive(Debug, Serialize)]
pub struct MediaSupportReport {
    pub inputs: Vec<Modality>,
    pub available_inputs: Vec<Modality>,
    pub input_availability: Vec<InputAvailability>,
    pub supported_attachments: Vec<String>,
    pub task_routes: Vec<OnnxTaskRoute>,
    pub missing_assets: Vec<String>,
}

impl MediaSupportReport {
    /// Add transport formats after resolving inputs, without holding a native-session lock.
    pub fn set_attachment_routes(&mut self, routes: &[AttachmentRoute<'_>]) {
        self.supported_attachments.clear();
        for route in routes {
            if self.available_inputs.contains(&route.input)
                && !self
                    .supported_attachments
                    .iter()
                    .any(|format| format == route.format)
            {
                self.supported_attachments.push(route.format.to_owned());
            }
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct OnnxTaskRoute {
    pub task: String,
    pub endpoint: &'static str,
    pub inputs: Vec<Modality>,
    pub outputs: Vec<&'static str>,
    pub available: bool,
}

/// Comes only from a successful native request and its retained owner.
#[derive(Debug, Clone)]
pub struct ValidatedOnnxTask {
    pub task: String,
    pub inputs: Vec<Modality>,
}

pub fn resolve_onnx_support(
    package: &PackageCapabilities,
    loaded: Option<&ValidatedOnnxTask>,
    enabled_attachments: &[AttachmentRoute<'_>],
) -> MediaSupportReport {
    let missing_assets = package.missing_onnx_assets();
    let tasks = package.onnx_tasks();
    let mut task_routes = Vec::new();
    for task in tasks {
        let (endpoint, inputs, outputs) = match task.as_str() {
            "embedding" => ("/v1/embeddings", vec![Modality::Text], vec!["embedding"]),
            "image-feature-extraction" => {
                ("/v1/embeddings", vec![Modality::Image], vec!["embedding"])
            }
            "text-to-speech" => ("/v1/audio/speech", vec![Modality::Text], vec!["audio"]),
            "automatic-speech-recognition" => {
                ("/v1/audio/transcriptions", vec![Modality::Audio], vec!["text"])
            }
            _ => continue,
        };
        let available = missing_assets.is_empty()
            && loaded.is_some_and(|loaded| loaded.task == task && loaded.inputs == inputs);
        task_routes.push(OnnxTaskRoute {
            task,
            endpoint,
            inputs,
            outputs,
            available,
        });
    }
    let mut available_inputs = Vec::new();
    for route in task_routes.iter().filter(|route| route.available) {
        for &input in &route.inputs {
            if !available_inputs.contains(&input) {
                available_inputs.push(input);
            }
        }
    }
    let input_availability = package
        .inputs
        .iter()
        .map(|declaration| {
            let available = available_inputs.contains(&declaration.modality);
            let reason = if available {
                None
            } else if declaration.state != CapabilityState::Supported {
                Some("model_capability_unknown")
            } else if !missing_assets.is_empty() {
                Some("onnx_helper_assets_missing")
            } else if !task_routes
                .iter()
                .any(|r| r.inputs.contains(&declaration.modality))
            {
                Some("onnx_input_requires_other_modality_or_unimplemented_task")
            } else {
                Some("onnx_task_session_unverified")
            };
            InputAvailability {
                input: declaration.modality,
                model_support: declaration.state,
                available,
                reason,
            }
        })
        .collect();
    let supported_attachments = enabled_attachments
        .iter()
        .filter(|r| available_inputs.contains(&r.input))
        .map(|r| r.format.to_owned())
        .collect();
    MediaSupportReport {
        inputs: package.supported_inputs(),
        available_inputs,
        input_availability,
        supported_attachments,
        task_routes,
        missing_assets,
    }
}

/// A parsed projector is not a validated pairing. Its path must come from the loaded backend.
pub fn resolve_support(
    package: &PackageCapabilities,
    backend_inputs: &[Modality],
    validated_pairing: Option<ValidatedPairing<'_>>,
    enabled_attachments: &[AttachmentRoute<'_>],
) -> MediaSupportReport {
    let validated_model = validated_pairing.and_then(|pair| pair.model.canonicalize().ok());
    let validated_projector = validated_pairing.and_then(|pair| pair.projector.canonicalize().ok());
    let mut available_inputs = Vec::new();
    let mut input_availability = Vec::new();
    for declaration in &package.inputs {
        let input = declaration.modality;
        let reason = match declaration.state {
            CapabilityState::Unknown => Some("model_capability_unknown"),
            CapabilityState::Unsupported => Some("model_input_unsupported"),
            CapabilityState::Supported if input != Modality::Text => {
                match package.projector.as_ref() {
                    None if package.projector_candidates.is_empty() => Some("projector_missing"),
                    None => Some("projector_selection_ambiguous"),
                    Some(projector) if !projector.path.is_file() => Some("projector_missing"),
                    Some(projector) if projector.compatibility == CapabilityState::Unsupported => {
                        Some("projector_dimension_mismatch")
                    }
                    Some(projector)
                        if match input {
                            Modality::Image => projector.profile.vision_encoder != Some(true),
                            Modality::Audio => projector.profile.audio_encoder != Some(true),
                            Modality::Text => false,
                        } =>
                    {
                        Some("projector_encoder_missing")
                    }
                    Some(_) if !backend_inputs.contains(&input) => {
                        Some("native_media_dispatch_unavailable")
                    }
                    Some(projector)
                        if validated_model.as_ref() != Some(&package.primary_file)
                            || validated_projector.as_ref()
                                != projector.path.canonicalize().ok().as_ref() =>
                    {
                        Some("projector_pairing_unverified")
                    }
                    Some(_) => None,
                }
            }
            CapabilityState::Supported if !backend_inputs.contains(&input) => {
                Some("backend_input_unavailable")
            }
            CapabilityState::Supported => None,
        };
        let available = reason.is_none();
        if available {
            available_inputs.push(input);
        }
        input_availability.push(InputAvailability {
            input,
            model_support: declaration.state,
            available,
            reason,
        });
    }
    let mut report = MediaSupportReport {
        inputs: package.supported_inputs(),
        available_inputs,
        input_availability,
        supported_attachments: Vec::new(),
        task_routes: Vec::new(),
        missing_assets: Vec::new(),
    };
    report.set_attachment_routes(enabled_attachments);
    report
}

#[cfg(test)]
mod tests {
    use super::*;
    use cluaiz_models::capabilities::{InputEvidence, ProjectorAsset, ProjectorProfile};

    #[test]
    fn test_support_requires_real_asset_encoder_route_and_pairing() {
        let file = tempfile::NamedTempFile::new().unwrap();
        let model = tempfile::NamedTempFile::new().unwrap();
        let pairing = Some(ValidatedPairing {
            model: model.path(),
            projector: file.path(),
        });
        let projector = ProjectorAsset {
            path: file.path().to_path_buf(),
            profile: ProjectorProfile {
                vision_encoder: Some(true),
                audio_encoder: Some(true),
                projector_type: None,
                vision_projector_type: None,
                audio_projector_type: None,
                vision_projection_dim: None,
                audio_projection_dim: None,
            },
            compatibility: CapabilityState::Unknown,
        };
        let mut package = PackageCapabilities {
            primary_file: model.path().canonicalize().unwrap(),
            inputs: [Modality::Text, Modality::Image, Modality::Audio]
                .into_iter()
                .map(|modality| InputEvidence {
                    modality,
                    state: CapabilityState::Supported,
                    evidence: Vec::new(),
                })
                .collect(),
            projector: None,
            projector_candidates: Vec::new(),
            diagnostics: Vec::new(),
            asset_snapshot: Vec::new(),
            onnx_graphs: Vec::new(),
        };
        let report = resolve_support(&package, CURRENT_NATIVE_CHAT_INPUTS, None, &[]);
        assert_eq!(report.available_inputs, [Modality::Text]);
        assert_eq!(
            report.input_availability[1].reason,
            Some("projector_missing")
        );
        package.projector = Some(projector);
        let report = resolve_support(&package, &[Modality::Text], None, &[]);
        assert_eq!(
            report.input_availability[1].reason,
            Some("native_media_dispatch_unavailable")
        );
        let inputs = [Modality::Text, Modality::Image, Modality::Audio];
        assert_eq!(
            resolve_support(&package, &inputs, None, &[]).input_availability[2].reason,
            Some("projector_pairing_unverified")
        );
        let routes = [
            AttachmentRoute {
                format: "image",
                input: Modality::Image,
            },
            AttachmentRoute {
                format: "audio",
                input: Modality::Audio,
            },
        ];
        let report = resolve_support(&package, &inputs, pairing, &routes);
        assert_eq!(report.available_inputs, inputs);
        assert_eq!(report.supported_attachments, ["image", "audio"]);
        package.projector.as_mut().unwrap().profile.audio_encoder = Some(false);
        assert_eq!(
            resolve_support(&package, &inputs, pairing, &routes).input_availability[2].reason,
            Some("projector_encoder_missing")
        );
        package.projector.as_mut().unwrap().compatibility = CapabilityState::Unsupported;
        assert_eq!(
            resolve_support(&package, &inputs, pairing, &routes).input_availability[1].reason,
            Some("projector_dimension_mismatch")
        );
        package.inputs[1].state = CapabilityState::Unknown;
        assert_eq!(
            resolve_support(&package, &inputs, pairing, &routes).input_availability[1].reason,
            Some("model_capability_unknown")
        );
    }
}
