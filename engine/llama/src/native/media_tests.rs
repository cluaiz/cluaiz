use super::*;

#[test]
fn test_shared_audio_bitmap_prepares_stereo_and_checks_copy_budget() -> Result<()> {
    let audio = DecodedAudio::new(24_000, 2, vec![0.25; 96].into())?;
    let (_bitmap, prepared) = Bitmap::from_audio(&audio, 16_000, MediaLimits::default())?;
    assert_eq!(prepared.channels(), 1);
    assert_eq!(prepared.sample_rate(), 16_000);
    assert_eq!(prepared.samples().len(), 32);
    assert!(prepared.samples().iter().all(|v| *v == 0.25));
    assert!(Bitmap::from_audio(&audio, 16_000, MediaLimits::new(1024, 1)?).is_err());
    Ok(())
}

fn request(parts: Vec<PreparedPart>) -> ChatEnvelope<Vec<PreparedPart>> {
    ChatEnvelope {
        messages: vec![ChatMessage {
            role: "user".into(),
            content: parts,
        }],
        samplers: cluaiz_core::SamplerDefaults {
            temp: Some(0.0),
            seed: Some(9),
            ..Default::default()
        },
        think_mode: "off".into(),
        reasoning_effort: Default::default(),
        reasoning_budget_message: Some("budget message".into()),
        chat_template_kwargs: None,
        grammar: Some("grammar".into()),
        response_format: None,
    }
}

#[test]
fn test_native_copy_budget_counts_retained_arcs_instead_of_configured_ceiling() -> Result<()> {
    let image = cluaiz_core::media::DecodedImage::new(1, 1, Arc::from([255, 0, 0]))?;
    let request = request(vec![
        PreparedPart::Image {
            image: image.clone(),
        },
        PreparedPart::Image { image },
    ]);
    let support = ProjectorSupport {
        vision: true,
        audio_sample_rate: None,
    };
    assert_eq!(
        MediaInput::additional_bytes(&request, support, MediaLimits::default())?,
        6
    );
    // All source Arcs coexist with all native copies: six original + six native bytes.
    assert!(MediaInput::prepare(request.clone(), support, MediaLimits::new(1, 11)?).is_err());
    assert!(MediaInput::prepare(request, support, MediaLimits::new(1, 12)?).is_ok());
    Ok(())
}

#[test]
fn test_media_template_preserves_literal_marker_and_rejects_loss_reorder() -> Result<()> {
    let image = cluaiz_core::media::DecodedImage::new(1, 1, Arc::from([255, 0, 0]))?;
    let (text, input) = MediaInput::prepare(
        request(vec![
            PreparedPart::Text {
                text: "literal <__media__> ".into(),
            },
            PreparedPart::Image { image },
        ]),
        ProjectorSupport {
            vision: true,
            audio_sample_rate: None,
        },
        MediaLimits::default(),
    )?;
    assert!(text.messages[0].content.starts_with("literal <__media__> "));
    assert_eq!(text.samplers.seed, Some(9));
    assert_eq!(text.grammar.as_deref(), Some("grammar"));
    assert_eq!(
        marker_ranges(&text.messages[0].content, &input.markers)?.len(),
        1
    );
    assert!(marker_ranges("removed", &input.markers).is_err());
    assert!(marker_ranges(&text.messages[0].content.repeat(2), &input.markers).is_err());
    assert!(marker_ranges("second first", &["first".into(), "second".into()]).is_err());
    Ok(())
}

#[test]
fn test_media_audio_uses_shared_mono_rate_and_rejects_unsupported_or_budget() -> Result<()> {
    let audio =
        cluaiz_core::media::DecodedAudio::new(8000, 2, Arc::from([0.0_f32, 1.0, 0.0, -1.0]))?;
    let support = ProjectorSupport {
        vision: false,
        audio_sample_rate: NonZeroU32::new(16000),
    };
    let (_, input) = MediaInput::prepare(
        request(vec![PreparedPart::Audio {
            audio: audio.clone(),
        }]),
        support,
        MediaLimits::default(),
    )?;
    // SAFETY: the owned native audio bitmap is live; queries only inspect its copied PCM.
    unsafe {
        assert!(mtmd::mtmd_bitmap_is_audio(input.bitmaps[0].0.as_ptr()));
        assert_eq!(
            mtmd::mtmd_bitmap_get_n_bytes(input.bitmaps[0].0.as_ptr()),
            4 * std::mem::size_of::<f32>()
        );
    }
    assert!(MediaInput::prepare(
        request(vec![PreparedPart::Audio {
            audio: audio.clone()
        }]),
        ProjectorSupport {
            vision: false,
            audio_sample_rate: None
        },
        MediaLimits::default()
    )
    .is_err());
    assert!(MediaInput::prepare(
        request(vec![PreparedPart::Audio { audio }]),
        support,
        MediaLimits::new(1, 1)?
    )
    .is_err());
    Ok(())
}

#[test]
fn test_projector_load_callback_cancel_and_null_fail_closed() {
    let signal = Arc::new(AtomicBool::new(false));
    let data = Arc::as_ptr(&signal).cast_mut().cast();
    // SAFETY: signal retains valid callback data during both synchronous calls.
    unsafe {
        assert!(load_progress(0.5, data));
        signal.store(true, Ordering::Release);
        assert!(!load_progress(0.5, data));
        assert!(!load_progress(0.5, std::ptr::null_mut()));
    }
}

#[test]
fn test_removing_old_media_turn_preserves_system_current_pixels_and_original_identity() -> Result<()>
{
    let image = cluaiz_core::media::DecodedImage::new(1, 1, Arc::from([12, 34, 56]))?;
    let mut envelope = request(vec![]);
    envelope.messages = vec![
        ChatMessage {
            role: "system".into(),
            content: vec![PreparedPart::Text {
                text: "system".into(),
            }],
        },
        ChatMessage {
            role: "user".into(),
            content: vec![PreparedPart::Image {
                image: image.clone(),
            }],
        },
        ChatMessage {
            role: "assistant".into(),
            content: vec![PreparedPart::Text {
                text: "old answer".into(),
            }],
        },
        ChatMessage {
            role: "user".into(),
            content: vec![PreparedPart::Image { image }],
        },
    ];
    let (text, mut input) = MediaInput::prepare(
        envelope,
        ProjectorSupport {
            vision: true,
            audio_sample_rate: None,
        },
        MediaLimits::default(),
    )?;
    let mut messages: Vec<_> = text
        .messages
        .into_iter()
        .map(|m| (m.role, m.content))
        .collect();
    let sizes: Vec<_> = messages
        .iter()
        .map(|(role, _)| (role.as_str(), 1))
        .collect();
    let (n_drop, _) = cluaiz_core::oldest_turns_to_drop(&sizes, 1);
    assert_eq!(n_drop, 2);
    input.remove_history(&mut messages, 1..1 + n_drop)?;
    assert_eq!(messages[0], ("system".into(), "system".into()));
    assert_eq!(messages.len(), 2);
    assert_eq!(input.targets, [(3, 0)]);
    assert_eq!(input.bitmaps.len(), 1);
    assert_eq!(marker_ranges(&messages[1].1, &input.markers)?.len(), 1);
    // SAFETY: retained native bitmap owns its pixel storage.
    unsafe {
        assert_eq!(
            std::slice::from_raw_parts(mtmd::mtmd_bitmap_get_data(input.bitmaps[0].0.as_ptr()), 3),
            &[12, 34, 56]
        );
    }
    let sizes: Vec<_> = messages
        .iter()
        .map(|(role, _)| (role.as_str(), 1))
        .collect();
    assert_eq!(cluaiz_core::oldest_turns_to_drop(&sizes, 1).0, 0);
    messages.push(("assistant".into(), "tool call".into()));
    messages.push(("tool".into(), "tool result".into()));
    assert!(input.remove_history(&mut messages, 1..3).is_err());
    assert_eq!(input.targets, [(3, 0)]);
    assert_eq!(input.bitmaps.len(), 1);
    messages.push(("user".into(), "current source text".into()));
    input.remove_history(&mut messages, 1..4)?;
    assert!(input.bitmaps.is_empty());
    assert!(input.markers.is_empty());
    assert!(input.targets.is_empty());
    assert_eq!(
        messages,
        [
            ("system".into(), "system".into()),
            ("user".into(), "current source text".into())
        ]
    );
    Ok(())
}
