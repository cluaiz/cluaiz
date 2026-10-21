use super::*;
use crate::formats::FormatRegistry;
use base64::Engine;
use cluaiz_core::{media::MediaSettings, SamplerDefaults, ThinkingBudget};
use std::io::Cursor;

fn request(parts: Vec<InputPart>) -> ChatEnvelope<Vec<InputPart>> {
    ChatEnvelope {
        messages: vec![
            ChatMessage {
                role: "system".into(),
                content: vec![InputPart::Text {
                    text: "context".into(),
                }],
            },
            ChatMessage {
                role: "user".into(),
                content: parts,
            },
        ],
        samplers: SamplerDefaults {
            temp: Some(0.35),
            seed: Some(17),
            ..Default::default()
        },
        think_mode: "auto".into(),
        reasoning_effort: ThinkingBudget::Low,
        reasoning_budget_message: Some("budget".into()),
        chat_template_kwargs: Some(serde_json::json!({"enable_thinking": true})),
        grammar: Some("root ::= \"ok\"".into()),
        response_format: Some(serde_json::json!({"type": "json_object"})),
    }
}

fn resolver(dir: &Path, decoded_limit: u64) -> SourceResolver {
    SourceResolver::new(
        dir.to_path_buf(),
        MediaSettings {
            limits: MediaLimits::new(cluaiz_core::MIB, decoded_limit).unwrap(),
            ..Default::default()
        },
        FormatRegistry::default(),
    )
    .unwrap()
}

fn png() -> Vec<u8> {
    let image = ::image::RgbImage::from_raw(1, 1, vec![12, 34, 56]).unwrap();
    let mut bytes = Cursor::new(Vec::new());
    ::image::DynamicImage::ImageRgb8(image)
        .write_to(&mut bytes, ::image::ImageOutputFormat::Png)
        .unwrap();
    bytes.into_inner()
}

#[tokio::test]
async fn test_document_image_policy_keeps_source_code_as_text() -> Result<()> {
    let dir = tempfile::tempdir()?;
    let resolver = resolver(dir.path(), 256).with_document_images(true);
    let prepared = prepare_chat(
        request(vec![InputPart::Document {
            source: "data:text/plain;base64,Y29uc3QgdmFsdWUgPSA3Ow==".into(),
        }]),
        &resolver,
        |_| Ok(()),
    )
    .await?;
    assert!(
        matches!(&prepared.messages[1].content[..], [PreparedPart::Text { text }]
        if text.contains("const value = 7;"))
    );
    Ok(())
}

#[tokio::test]
async fn test_preparation_reports_each_real_attachment_and_preserves_origin() -> Result<()> {
    use cluaiz_core::processing::{ProcessingReporter, ProcessingStage, ProcessingState};
    let events = Arc::new(std::sync::Mutex::new(Vec::new()));
    let received = events.clone();
    let progress = ProcessingReporter::new(move |event| received.lock().unwrap().push(event));
    let dir = tempfile::tempdir()?;
    let resolver = resolver(dir.path(), 256).with_progress(Some(progress.clone()));
    let prepared = prepare_chat(
        request(vec![
            InputPart::Image {
                source: inline("image/png", &png()),
            },
            InputPart::Image {
                source: inline("image/png", &png()),
            },
        ]),
        &resolver,
        |_| Ok(()),
    )
    .await?;
    assert_eq!(prepared.messages[1].content.len(), 2);
    let events = events.lock().unwrap();
    assert_eq!(events.len(), 8);
    for (index, group) in events.chunks_exact(4).enumerate() {
        assert_eq!(group[0].stage, ProcessingStage::Reading);
        assert_eq!(group[3].stage, ProcessingStage::Decoding);
        assert_eq!(group[3].state, ProcessingState::Completed);
        let target = group[3].attachment.unwrap();
        assert_eq!((target.message_index, target.part_index), (1, index));
        assert_eq!(progress.attachment(1, index), target);
        assert!(group.iter().all(|event| event.work.is_none()));
    }
    Ok(())
}

fn wav() -> Vec<u8> {
    let mut bytes = Cursor::new(Vec::new());
    let mut writer = hound::WavWriter::new(
        &mut bytes,
        hound::WavSpec {
            channels: 2,
            sample_rate: 8_000,
            bits_per_sample: 32,
            sample_format: hound::SampleFormat::Float,
        },
    )
    .unwrap();
    for sample in [2.0_f32, -2.0, 0.25, 0.75] {
        writer.write_sample(sample).unwrap();
    }
    writer.finalize().unwrap();
    bytes.into_inner()
}

fn inline(mime: &str, bytes: &[u8]) -> String {
    format!(
        "data:{mime};base64,{}",
        base64::engine::general_purpose::STANDARD.encode(bytes)
    )
}

#[tokio::test]
async fn test_request_order_options_actual_buffers_and_cloned_continuation() {
    let dir = tempfile::tempdir().unwrap();
    let input = request(vec![
        InputPart::Text {
            text: "before".into(),
        },
        InputPart::Image {
            source: inline("image/png", &png()),
        },
        InputPart::Text {
            text: "between".into(),
        },
        InputPart::Audio {
            source: inline("audio/wav", &wav()),
        },
        InputPart::Text {
            text: "after".into(),
        },
    ]);
    let options = input.clone();
    let resolver = resolver(dir.path(), cluaiz_core::MIB);
    // The owned request preparation future can run on the async runtime's worker.
    let prepared = tokio::spawn(async move { prepare_chat(input, &resolver, |_| Ok(())).await })
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        prepared
            .messages
            .iter()
            .map(|message| message.role.as_str())
            .collect::<Vec<_>>(),
        ["system", "user"]
    );
    assert_eq!(prepared.samplers, options.samplers);
    assert_eq!(prepared.think_mode, options.think_mode);
    assert_eq!(prepared.reasoning_effort, options.reasoning_effort);
    assert_eq!(
        prepared.reasoning_budget_message,
        options.reasoning_budget_message
    );
    assert_eq!(prepared.chat_template_kwargs, options.chat_template_kwargs);
    assert_eq!(prepared.grammar, options.grammar);
    assert_eq!(prepared.response_format, options.response_format);
    let parts = &prepared.messages[1].content;
    assert_eq!(
        parts.iter().map(PreparedPart::modality).collect::<Vec<_>>(),
        [
            cluaiz_core::Modality::Text,
            cluaiz_core::Modality::Image,
            cluaiz_core::Modality::Text,
            cluaiz_core::Modality::Audio,
            cluaiz_core::Modality::Text,
        ]
    );
    for (index, expected) in [(0, "before"), (2, "between"), (4, "after")] {
        assert!(matches!(&parts[index], PreparedPart::Text { text } if text == expected));
    }
    let PreparedPart::Image { image } = &parts[1] else {
        panic!("Expected image");
    };
    assert_eq!(image.pixels(), &[12, 34, 56]);
    let image_ptr = image.pixels().as_ptr();
    let PreparedPart::Audio { audio } = &parts[3] else {
        panic!("Expected audio");
    };
    assert_eq!((audio.sample_rate(), audio.channels()), (8_000, 2));
    assert_eq!(audio.samples(), &[2.0, -2.0, 0.25, 0.75]);
    let audio_ptr = audio.samples().as_ptr();
    assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 0);
    let continuation = prepared.clone();
    drop(prepared);
    let PreparedPart::Image { image } = &continuation.messages[1].content[1] else {
        panic!("Expected image");
    };
    assert_eq!(image.pixels().as_ptr(), image_ptr);
    let PreparedPart::Audio { audio } = &continuation.messages[1].content[3] else {
        panic!("Expected audio");
    };
    assert_eq!(audio.samples().as_ptr(), audio_ptr);
    assert_eq!(audio.samples()[2], 0.25);
}

#[tokio::test]
async fn test_request_invalid_source_permissions_and_unsupported_provider_reject() {
    let dir = tempfile::tempdir().unwrap();
    let resolver = resolver(dir.path(), cluaiz_core::MIB);
    let local = dir.path().join("input.png");
    std::fs::write(&local, png()).unwrap();
    let result = prepare_chat(
        request(vec![InputPart::Image {
            source: local.to_str().unwrap().into(),
        }]),
        &resolver,
        |_| anyhow::bail!("Access denied"),
    )
    .await;
    assert!(format!("{:#}", result.unwrap_err()).contains("Access denied"));
    assert!(prepare_chat(
        request(vec![InputPart::Image {
            source: "data:image/png;base64,!!!!".into()
        }]),
        &resolver,
        |_| Ok(())
    )
    .await
    .is_err());
    assert!(prepare_chat(
        request(vec![InputPart::Audio {
            source: inline("image/png", &png())
        }]),
        &resolver,
        |_| Ok(())
    )
    .await
    .is_err());
    for part in [InputPart::Video {
        source: "not-read.mp4".into(),
        include_audio: false,
    }] {
        let result = prepare_chat(
            request(vec![
                InputPart::Image {
                    source: "must-not-be-read.png".into(),
                },
                part,
            ]),
            &resolver,
            |_| panic!("Unsupported provider must fail before source authorization"),
        )
        .await;
        assert!(matches!(
            result.unwrap_err().downcast_ref::<EngineError>(),
            Some(EngineError::UnsupportedModality(_))
        ));
    }
    assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 1);
}

#[tokio::test]
async fn test_request_aggregate_budget_and_literal_text_markers() {
    let dir = tempfile::tempdir().unwrap();
    let limit = 12;
    // 7 retained system bytes + 6 bytes for overlapping RGB Vec/Arc exceed 12.
    let result = prepare_chat(
        request(vec![InputPart::Image {
            source: inline("image/png", &png()),
        }]),
        &resolver(dir.path(), limit),
        |_| Ok(()),
    )
    .await;
    assert!(result.is_err());
    let images = vec![
        InputPart::Image {
            source: inline("image/png", &png())
        };
        2
    ];
    // First image fits, but its retained RGB bytes reduce the next decoder's budget.
    assert!(
        prepare_chat(request(images.clone()), &resolver(dir.path(), 15), |_| Ok(
            ()
        ))
        .await
        .is_err()
    );
    let paired = prepare_chat(request(images), &resolver(dir.path(), 16), |_| Ok(()))
        .await
        .unwrap();
    assert_eq!(paired.messages[1].content.len(), 2);
    let mut reserved_text = String::with_capacity(129);
    reserved_text.push('x');
    assert!(prepare_chat(
        request(vec![InputPart::Text {
            text: reserved_text
        }]),
        &resolver(dir.path(), 128),
        |_| Ok(())
    )
    .await
    .is_err());
    let marker = "<media type=\"image\" url=\"literal\" />";
    let prepared = prepare_chat(
        request(vec![InputPart::Text {
            text: marker.into(),
        }]),
        &resolver(dir.path(), 128),
        |_| Ok(()),
    )
    .await
    .unwrap();
    assert!(
        matches!(&prepared.messages[1].content[0], PreparedPart::Text { text } if text == marker)
    );
    assert!(prepare_chat(
        request(vec![InputPart::Text {
            text: "x".repeat(129)
        }]),
        &resolver(dir.path(), 128),
        |_| Ok(())
    )
    .await
    .is_err());
    assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 0);
}

#[tokio::test]
#[ignore = "requires CLUAIZ_MEDIA_TEST_IMAGE and CLUAIZ_MEDIA_TEST_RETAINED_BYTES"]
async fn test_actual_image_with_retained_history_across_requests() -> Result<()> {
    let source = std::fs::canonicalize(std::env::var("CLUAIZ_MEDIA_TEST_IMAGE")?)?;
    let retained = std::env::var("CLUAIZ_MEDIA_TEST_RETAINED_BYTES")?.parse::<usize>()?;
    let bytes = std::fs::read(&source)?;
    let reference = ::image::load_from_memory(&bytes)?.into_rgb8();
    let dir = tempfile::tempdir()?;
    let settings = MediaSettings::default();
    let resolver = SourceResolver::new(
        dir.path().to_owned(),
        settings.clone(),
        FormatRegistry::default(),
    )?;
    for request_index in 0..3 {
        let mut input = request(vec![InputPart::Image {
            source: source.to_string_lossy().into_owned(),
        }]);
        input.messages[0].content = vec![InputPart::Text {
            text: "x".repeat(retained),
        }];
        let prepared = prepare_chat(input, &resolver, |path| {
            anyhow::ensure!(
                std::fs::canonicalize(path)? == source,
                "Unexpected source path"
            );
            Ok(())
        })
        .await?;
        let PreparedPart::Image { image } = &prepared.messages[1].content[0] else {
            anyhow::bail!("Expected decoded image");
        };
        assert_eq!((image.width(), image.height()), reference.dimensions());
        assert!(image.pixels() == reference.as_raw(), "RGB pixels changed");
        println!("ACTUAL REQUEST PREPARATION: request={request_index}; retained_history={retained}; rgb_bytes={}; ceiling={}; exact_pixels=true", image.pixels().len(), settings.limits.max_decoded_bytes());
        drop(prepared);
    }
    Ok(())
}

#[test]
fn test_request_wire_parts_round_trip_and_unknown_type_field_reject() {
    let value = vec![
        InputPart::Text { text: "hi".into() },
        InputPart::Image {
            source: "input.png".into(),
        },
    ];
    let encoded = serde_json::to_string(&value).unwrap();
    assert_eq!(
        serde_json::from_str::<Vec<InputPart>>(&encoded).unwrap(),
        value
    );
    assert!(serde_json::from_str::<InputPart>(r#"{"type":"unknown","source":"input"}"#).is_err());
    assert!(serde_json::from_str::<InputPart>(
        r#"{"type":"image","source":"input","surprise":true}"#
    )
    .is_err());
}
