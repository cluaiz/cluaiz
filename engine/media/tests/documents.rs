mod support;
use anyhow::Result;
use base64::Engine;
use cluaiz_core::{
    media::{InputPart, PreparedPart},
    media_limits::MediaLimits,
    ChatEnvelope, ChatMessage,
};
use cluaiz_media::{documents, formats::FormatRegistry};

fn extract(bytes: &[u8], limit: u64) -> Result<documents::ExtractedDocument> {
    let format = FormatRegistry::default().detect(bytes, None)?;
    documents::extract(
        bytes,
        &format,
        MediaLimits::new(64 * cluaiz_core::MIB, limit)?,
    )
}

#[test]
fn test_real_pdf_pages_order_partial_and_corruption() -> Result<()> {
    let limits = MediaLimits::default();
    let bytes = support::pdf(&[Some("Order code ALPHA731"), Some("Second page BETA942")])?;
    let doc = extract(&bytes, limits.max_decoded_bytes())?;
    assert!(doc.complete());
    assert_eq!(
        doc.parts.iter().map(|part| part.page).collect::<Vec<_>>(),
        [Some(1), Some(2)]
    );
    let text = doc.text(limits, true)?;
    assert!(text.find("ALPHA731") < text.find("BETA942"));
    let mixed = support::pdf(&[Some("Known text"), None])?;
    let doc = extract(&mixed, limits.max_decoded_bytes())?;
    assert!(!doc.complete());
    assert!(doc.parts[1].text.trim().is_empty());
    assert!(!doc.parts[1].omissions.is_empty());
    assert!(doc.text(limits, true).is_err());
    assert!(doc.text(limits, false)?.contains("Known text"));
    assert!(extract(b"%PDF-1.4\ncorrupt", limits.max_decoded_bytes()).is_err());
    assert!(extract(&bytes, 1).is_err());
    println!("REAL DOCUMENT PDF: page order/content, mixed-page omissions, chat refusal, corruption and limits passed");
    Ok(())
}

#[test]
fn test_real_docx_unicode_tables_entities_and_zip_limits() -> Result<()> {
    let bytes = support::docx(support::DOCX_XML)?;
    let doc = extract(&bytes, MediaLimits::default().max_decoded_bytes())?;
    assert!(doc.complete());
    let text = doc.text(MediaLimits::default(), true)?;
    assert!(text.contains("Hello & नमस्ते"));
    assert!(text.find("First") < text.find("Second") && text.find("Second") < text.find("Final"));
    assert_eq!(doc.parts[0].page, None);
    assert!(extract(&bytes, 32).is_err());
    let invalid = support::docx("<document><t>foreign namespace</t></document>")?;
    assert!(extract(&invalid, 8192).is_err());
    let truncated = support::docx(&support::DOCX_XML.replace("</a:document>", ""))?;
    assert!(extract(&truncated, 8192).is_err());
    let entity = support::docx(&format!(
        "<!DOCTYPE document [<!ENTITY secret SYSTEM 'file:///private'>]>{}",
        support::DOCX_XML
    ))?;
    assert!(extract(&entity, 8192).is_err());
    let visual = support::docx(
        &support::DOCX_XML.replace("</a:body>", "<a:p><a:r><a:drawing/></a:r></a:p></a:body>"),
    )?;
    let visual = extract(&visual, 8192)?;
    assert!(!visual.complete());
    assert!(visual.text(MediaLimits::default(), true).is_err());
    let duplicate = support::docx(&format!("{}{}", support::DOCX_XML, support::DOCX_XML))?;
    assert!(extract(&duplicate, 8192).is_err());
    assert!(extract(b"binary\0data", 8192).is_err());
    let code = b"const order = 23;";
    let hint = FormatRegistry::default().detect(code, Some("video/mp2t"))?;
    assert!(documents::extract(code, &hint, MediaLimits::default())?
        .text(MediaLimits::default(), true)?
        .contains("order = 23"));
    println!(
        "REAL DOCUMENT DOCX: UTF-8/entity/table order, XML rejection and expansion limits passed"
    );
    Ok(())
}

#[tokio::test]
async fn test_document_request_prepares_real_text_in_order_and_drops_source() -> Result<()> {
    let dir = tempfile::tempdir()?;
    let bytes = support::pdf(&[Some("Actual attachment code CODE923")])?;
    let source = format!(
        "data:application/pdf;base64,{}",
        base64::engine::general_purpose::STANDARD.encode(bytes)
    );
    let request = ChatEnvelope {
        messages: vec![ChatMessage {
            role: "user".into(),
            content: vec![
                InputPart::Text {
                    text: "Before\n".into(),
                },
                InputPart::Document { source },
                InputPart::Text {
                    text: "After".into(),
                },
            ],
        }],
        samplers: Default::default(),
        think_mode: "off".into(),
        reasoning_effort: Default::default(),
        reasoning_budget_message: None,
        chat_template_kwargs: None,
        grammar: None,
        response_format: None,
    };
    let resolver = cluaiz_media::source::SourceResolver::new(
        dir.path().to_path_buf(),
        Default::default(),
        Default::default(),
    )?;
    let prepared = cluaiz_media::request::prepare_chat(request, &resolver, |_| {
        anyhow::bail!("Inline input must not read local paths")
    })
    .await?;
    assert_eq!(prepared.messages[0].content.len(), 3);
    assert!(
        matches!(&prepared.messages[0].content[1], PreparedPart::Text {text} if text.contains("CODE923") && !text.contains("base64"))
    );
    assert_eq!(std::fs::read_dir(dir.path())?.count(), 0);
    Ok(())
}
