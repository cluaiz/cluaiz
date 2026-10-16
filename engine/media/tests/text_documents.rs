// The shared fixture owner also contains PDF/DOCX builders used by other suites.
#[allow(dead_code)]
mod support;

use anyhow::Result;
use cluaiz_core::{
    media::{InputPart, PreparedPart},
    media_limits::MediaLimits,
    ChatEnvelope, ChatMessage,
};
use cluaiz_media::{documents, formats::FormatRegistry};

fn extract(bytes: &[u8], limits: MediaLimits) -> Result<documents::ExtractedDocument> {
    documents::extract(
        bytes,
        &FormatRegistry::default().detect(bytes, None)?,
        limits,
    )
}

#[test]
fn test_unicode_text_boms_content_and_byte_limits() -> Result<()> {
    let text = "नमस्ते 🦀\nconst answer = 42;\t\r\n";
    let limits = MediaLimits::new(8192, u64::try_from(text.len())?)?;
    let mut variants = vec![text.as_bytes().to_vec()];
    let mut utf8 = vec![0xef, 0xbb, 0xbf];
    utf8.extend_from_slice(text.as_bytes());
    variants.push(utf8);
    for little in [true, false] {
        let mut bytes = if little {
            vec![0xff, 0xfe]
        } else {
            vec![0xfe, 0xff]
        };
        for unit in text.encode_utf16() {
            bytes.extend_from_slice(&if little {
                unit.to_le_bytes()
            } else {
                unit.to_be_bytes()
            });
        }
        variants.push(bytes);
    }
    for bytes in variants {
        let document = extract(&bytes, limits)?;
        assert_eq!(document.parts[0].text, text);
        assert!(extract(
            &bytes,
            MediaLimits::new(8192, u64::try_from(text.len() - 1)?)?
        )
        .is_err());
        assert!(extract(&bytes, MediaLimits::new(1, 8192)?).is_err());
    }
    for invalid in [
        b"binary\0data".as_slice(),
        b"\xff",
        b"\xff\xfe\x00",
        b"\xff\xfe\x00\xd8",
        b"\xff\xfe\x00\x00",
        b" \t\n",
        b"\xef\xbb\xbf",
    ] {
        assert!(
            extract(invalid, MediaLimits::default()).is_err(),
            "{invalid:?}"
        );
    }
    Ok(())
}

#[tokio::test]
async fn test_all_requested_text_code_extensions_prepare_actual_text() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let text = "// actual UTF-8 source: नमस्ते\nconst attachment_value = 3187;\n";
    let resolver = cluaiz_media::source::SourceResolver::new(
        directory.path().join("temporary"),
        Default::default(),
        FormatRegistry::default(),
    )?;
    // Picker policy examples belong in tests; production detects actual content.
    for extension in [
        "txt", "md", "csv", "json", "toml", "yaml", "yml", "rs", "ts", "tsx", "js", "jsx", "py",
        "c", "cpp", "h", "go", "html", "css", "sql", "sh", "log",
    ] {
        let path = directory.path().join(format!("source.{extension}"));
        std::fs::write(&path, text)?;
        let envelope = ChatEnvelope {
            messages: vec![ChatMessage {
                role: "user".into(),
                content: vec![InputPart::Document {
                    source: path.to_string_lossy().into_owned(),
                }],
            }],
            samplers: Default::default(),
            think_mode: "off".into(),
            reasoning_effort: Default::default(),
            reasoning_budget_message: None,
            chat_template_kwargs: None,
            grammar: None,
            response_format: None,
        };
        let prepared = cluaiz_media::request::prepare_chat(envelope, &resolver, |_| Ok(())).await?;
        assert!(
            matches!(&prepared.messages[0].content[0], PreparedPart::Text { text: actual } if actual.trim_end() == text.trim_end()),
            "{extension}"
        );
    }
    Ok(())
}

#[test]
fn test_legacy_doc_actual_unicode_ansi_tables_corruption_and_limits() -> Result<()> {
    for (text, compressed, table1) in [
        ("Actual order ALPHA731\r", true, false),
        ("नमस्ते 🦀\rSecond line BETA942\r", false, true),
    ] {
        let bytes = support::doc(text, compressed, table1)?;
        let hinted = FormatRegistry::default().detect(&bytes, Some("application/msword"))?;
        assert!(documents::extract(&bytes, &hinted, MediaLimits::default())?.complete());
        let document = extract(&bytes, MediaLimits::default())?;
        assert_eq!(document.parts[0].method, "doc_binary");
        assert!(document.complete(), "{:?}", document.parts);
        let actual = document.text(MediaLimits::default(), true)?;
        assert!(
            actual.contains(text.split('\r').next().unwrap()),
            "{actual}"
        );
        assert!(extract(&bytes, MediaLimits::new(8192, 4)?).is_err());
        assert!(extract(&bytes[..512], MediaLimits::default()).is_err());
    }
    let bytes = support::doc("Known text\r", false, false)?;
    for (offset, value) in [(0x0a, 0x0104u16), (0, 0u16)] {
        let mut container = cfb::CompoundFile::open(std::io::Cursor::new(bytes.clone()))?;
        use std::io::{Seek, Write};
        let mut stream = container.open_stream("/WordDocument")?;
        stream.seek(std::io::SeekFrom::Start(offset))?;
        stream.write_all(&value.to_le_bytes())?;
        drop(stream);
        assert!(extract(&container.into_inner().into_inner(), MediaLimits::default()).is_err());
    }
    let mut container = cfb::CompoundFile::open(std::io::Cursor::new(bytes))?;
    use std::io::{Seek, Write};
    let mut stream = container.open_stream("/WordDocument")?;
    stream.seek(std::io::SeekFrom::Start(0x4c))?;
    stream.write_all(&999u32.to_le_bytes())?;
    drop(stream);
    let document = extract(&container.into_inner().into_inner(), MediaLimits::default())?;
    assert!(!document.complete());
    assert!(document.text(MediaLimits::default(), true).is_err());
    assert!(document
        .text(MediaLimits::default(), false)?
        .contains("Known text"));
    Ok(())
}

#[test]
fn test_legacy_doc_subdocument_order_and_non_word_container_rejection() -> Result<()> {
    use std::io::{Cursor, Seek, SeekFrom, Write};
    let body = "Body नमस्ते\r";
    let note = "Footnote BETA942\r";
    let bytes = support::doc(&format!("{body}{note}"), false, false)?;
    let mut container = cfb::CompoundFile::open(Cursor::new(bytes))?;
    let mut stream = container.open_stream("/WordDocument")?;
    stream.seek(SeekFrom::Start(0x4c))?;
    stream.write_all(&u32::try_from(body.encode_utf16().count())?.to_le_bytes())?;
    stream.write_all(&u32::try_from(note.encode_utf16().count())?.to_le_bytes())?;
    drop(stream);
    let document = extract(&container.into_inner().into_inner(), MediaLimits::default())?;
    let text = document.text(MediaLimits::default(), true)?;
    assert!(
        text.find("Body").unwrap() < text.find("Footnote").unwrap(),
        "{text}"
    );
    assert!(text.contains("नमस्ते") && text.contains("BETA942"));
    let mut other = cfb::CompoundFile::create(Cursor::new(Vec::new()))?;
    other
        .create_stream("/Workbook")?
        .write_all(b"Not a Word document")?;
    assert!(extract(&other.into_inner().into_inner(), MediaLimits::default()).is_err());
    Ok(())
}

#[test]
fn test_document_extraction_timings_actual_content() -> Result<()> {
    let text = "const actual_value = 3187; // नमस्ते\n".repeat(4096);
    let doc = support::doc(&text.replace('\n', "\r"), false, false)?;
    for (name, bytes) in [("UTF-8", text.as_bytes()), ("DOC", doc.as_slice())] {
        for run in 1..=3 {
            let started = std::time::Instant::now();
            let document = extract(bytes, MediaLimits::default())?;
            assert!(document.parts[0].text.contains("actual_value = 3187"));
            println!(
                "EXTRACTION {name} run={run} input_bytes={} output_bytes={} elapsed_us={}",
                bytes.len(),
                document.parts[0].text.len(),
                started.elapsed().as_micros()
            );
        }
    }
    Ok(())
}
