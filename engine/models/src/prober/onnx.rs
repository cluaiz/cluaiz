//! Bounded ONNX protobuf signature inspection; skips tensor payloads without loading weights.
//! Field numbers follow https://github.com/onnx/onnx/blob/main/onnx/onnx.proto.
use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeSet,
    fs::File,
    io::{Read, Seek, SeekFrom},
    path::Path,
};

const MAX_METADATA_BYTES: u64 = 4 * 1024 * 1024;
const MAX_FIELDS: usize = 1_000_000;
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct OnnxTensorSignature {
    pub name: String,
    /// ONNX TensorProto.DataType numeric value; zero is unknown/non-tensor.
    pub element_type: u64,
    /// Unknown or symbolic dimensions are None, never guessed constants.
    pub dimensions: Vec<Option<u64>>,
    pub dimension_symbols: Vec<String>,
}
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct OnnxProbeResult {
    pub file: String,
    pub inputs: Vec<OnnxTensorSignature>,
    pub outputs: Vec<OnnxTensorSignature>,
    pub external_files: Vec<String>,
    pub is_tts: bool,
    pub is_asr: bool,
    pub is_embedding: bool,
    pub is_vision: bool,
    pub tts_family: Option<String>,
}
impl OnnxProbeResult {
    pub fn has_input(&self, name: &str) -> bool {
        self.inputs.iter().any(|v| v.name == name)
    }
    pub fn has_output(&self, name: &str) -> bool {
        self.outputs.iter().any(|v| v.name == name)
    }
}
pub struct OnnxProber;
impl OnnxProber {
    /// Compatibility discovery preserves unknown on malformed files, never filename guesses.
    pub fn probe_file(path: &Path) -> OnnxProbeResult {
        Self::read_signature(path).unwrap_or_default()
    }
    pub fn read_signature(path: &Path) -> Result<OnnxProbeResult> {
        let file = File::open(path)?;
        let length = file.metadata()?.len();
        let mut reader = Reader {
            file,
            metadata: 0,
            fields: 0,
        };
        let mut result = OnnxProbeResult {
            file: path
                .file_name()
                .context("Graph filename missing")?
                .to_string_lossy()
                .into_owned(),
            ..Default::default()
        };
        let mut graph = false;
        while let Some(field) = reader.field(length)? {
            if field.number == 7 && field.wire == 2 {
                if graph {
                    bail!("Multiple ONNX main graphs");
                }
                graph = true;
                reader.graph(field.end, &mut result)?;
            }
            reader.finish(&field)?;
        }
        if !graph || result.inputs.is_empty() || result.outputs.is_empty() {
            bail!("ONNX graph has no usable signature");
        }
        result.is_vision = result.has_input("pixel_values");
        result.is_asr = result.has_input("input_features")
            || result.has_input("input_values")
            || result.has_input("encoder_hidden_states");
        result.is_tts = (result.has_input("input_lengths") && result.has_input("scales"))
            || (result.has_input("tokens")
                && result.has_input("style")
                && result.has_input("speed"));
        result.is_embedding = result.has_output("image_embeds")
            || result.has_output("text_embeds")
            || result.has_output("sentence_embedding")
            || result.has_output("last_hidden_state");
        Ok(result)
    }
}
struct Field {
    number: u64,
    wire: u8,
    value: u64,
    start: u64,
    end: u64,
}
struct Reader {
    file: File,
    metadata: u64,
    fields: usize,
}
impl Reader {
    fn varint(&mut self, end: u64) -> Result<u64> {
        let mut value = 0u64;
        for shift in (0..70).step_by(7) {
            if self.file.stream_position()? >= end {
                bail!("Truncated protobuf varint");
            }
            let mut byte = [0];
            self.file.read_exact(&mut byte)?;
            if shift == 63 && byte[0] > 1 {
                bail!("Protobuf varint overflow");
            }
            value |= u64::from(byte[0] & 0x7f) << shift;
            if byte[0] & 0x80 == 0 {
                return Ok(value);
            }
        }
        bail!("Invalid protobuf varint")
    }
    fn field(&mut self, end: u64) -> Result<Option<Field>> {
        if self.file.stream_position()? == end {
            return Ok(None);
        }
        self.fields += 1;
        if self.fields > MAX_FIELDS {
            bail!("ONNX signature field limit exceeded");
        }
        let tag = self.varint(end)?;
        let number = tag >> 3;
        let wire = (tag & 7) as u8;
        if number == 0 {
            bail!("Invalid protobuf field number");
        }
        let value = if wire == 0 { self.varint(end)? } else { 0 };
        let length = match wire {
            0 => 0,
            1 => 8,
            2 => self.varint(end)?,
            5 => 4,
            _ => bail!("Unsupported protobuf wire type"),
        };
        let start = self.file.stream_position()?;
        let field_end = start
            .checked_add(length)
            .filter(|v| *v <= end)
            .context("Protobuf field exceeds message bounds")?;
        Ok(Some(Field {
            number,
            wire,
            value,
            start,
            end: field_end,
        }))
    }
    fn finish(&mut self, field: &Field) -> Result<()> {
        if self.file.stream_position()? > field.end {
            bail!("Protobuf parser exceeded field bounds");
        }
        self.file.seek(SeekFrom::Start(field.end))?;
        Ok(())
    }
    fn text(&mut self, field: &Field) -> Result<String> {
        if field.wire != 2 {
            bail!("Expected protobuf string");
        }
        let length = field.end - field.start;
        self.metadata = self
            .metadata
            .checked_add(length)
            .context("Metadata size overflow")?;
        if self.metadata > MAX_METADATA_BYTES {
            bail!("ONNX signature metadata limit exceeded");
        }
        let mut data = vec![0; usize::try_from(length)?];
        self.file.read_exact(&mut data)?;
        Ok(String::from_utf8(data)?)
    }
    fn graph(&mut self, end: u64, result: &mut OnnxProbeResult) -> Result<()> {
        let mut initializers = BTreeSet::new();
        let mut external = BTreeSet::new();
        while let Some(field) = self.field(end)? {
            if field.wire == 2 {
                match field.number {
                    5 => self.initializer(field.end, &mut initializers, &mut external)?,
                    11 | 12 => {
                        if result.inputs.len() + result.outputs.len() >= 4096 {
                            bail!("ONNX signature value limit exceeded");
                        }
                        let value = self.value_info(field.end)?;
                        if field.number == 11 {
                            result.inputs.push(value);
                        } else {
                            result.outputs.push(value);
                        }
                    }
                    _ => (),
                }
            }
            self.finish(&field)?;
        }
        result
            .inputs
            .retain(|value| !initializers.contains(&value.name));
        result.external_files = external.into_iter().collect();
        Ok(())
    }
    fn initializer(
        &mut self,
        end: u64,
        names: &mut BTreeSet<String>,
        external: &mut BTreeSet<String>,
    ) -> Result<()> {
        while let Some(field) = self.field(end)? {
            match (field.number, field.wire) {
                (8, 2) => {
                    names.insert(self.text(&field)?);
                }
                (13, 2) => {
                    let (mut key, mut value) = (String::new(), String::new());
                    while let Some(entry) = self.field(field.end)? {
                        if entry.number == 1 {
                            key = self.text(&entry)?;
                        }
                        if entry.number == 2 {
                            value = self.text(&entry)?;
                        }
                        self.finish(&entry)?;
                    }
                    if key == "location" {
                        if value.is_empty()
                            || value.contains('\\')
                            || value.contains(':')
                            || Path::new(&value)
                                .components()
                                .any(|p| !matches!(p, std::path::Component::Normal(_)))
                        {
                            bail!("External tensor location must stay within its model package");
                        }
                        external.insert(value);
                    }
                }
                _ => (), // Raw and packed tensor payloads are skipped by seek, not allocated.
            }
            self.finish(&field)?;
        }
        Ok(())
    }
    fn value_info(&mut self, end: u64) -> Result<OnnxTensorSignature> {
        let mut tensor = OnnxTensorSignature::default();
        while let Some(field) = self.field(end)? {
            match (field.number, field.wire) {
                (1, 2) => tensor.name = self.text(&field)?,
                (2, 2) => self.value_type(field.end, &mut tensor)?,
                _ => (),
            }
            self.finish(&field)?;
        }
        if tensor.name.is_empty() {
            bail!("Empty ONNX value name");
        }
        Ok(tensor)
    }
    fn value_type(&mut self, end: u64, tensor: &mut OnnxTensorSignature) -> Result<()> {
        while let Some(field) = self.field(end)? {
            if field.number == 1 && field.wire == 2 {
                while let Some(value) = self.field(field.end)? {
                    match (value.number, value.wire) {
                        (1, 0) => tensor.element_type = value.value,
                        (2, 2) => self.shape(value.end, tensor)?,
                        _ => (),
                    }
                    self.finish(&value)?;
                }
            }
            self.finish(&field)?;
        }
        Ok(())
    }
    fn shape(&mut self, end: u64, tensor: &mut OnnxTensorSignature) -> Result<()> {
        while let Some(field) = self.field(end)? {
            if field.number == 1 && field.wire == 2 {
                if tensor.dimensions.len() >= 32 {
                    bail!("ONNX signature rank limit exceeded");
                }
                let (mut size, mut symbol) = (None, String::new());
                while let Some(dim) = self.field(field.end)? {
                    match (dim.number, dim.wire) {
                        (1, 0) => size = Some(dim.value),
                        (2, 2) => symbol = self.text(&dim)?,
                        _ => (),
                    }
                    self.finish(&dim)?;
                }
                tensor.dimensions.push(size);
                tensor.dimension_symbols.push(symbol);
            }
            self.finish(&field)?;
        }
        Ok(())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_corrupt_and_model_names_do_not_invent_capabilities() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let path = dir.path().join("clip-whisper-piper.onnx");
        std::fs::write(&path, [0xffu8; 12])?;
        assert!(OnnxProber::read_signature(&path).is_err());
        let result = OnnxProber::probe_file(&path);
        assert!(!result.is_tts && !result.is_asr && !result.is_embedding && !result.is_vision);
        Ok(())
    }

    #[test]
    fn test_signature_is_content_based_and_external_paths_cannot_escape() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let path = dir.path().join("unrelated.onnx");
        crate::test_fixtures::onnx(
            &path,
            "input_ids",
            "sentence_embedding",
            Some("weights/model.data"),
        );
        let graph = OnnxProber::read_signature(&path)?;
        assert!(graph.is_embedding && !graph.is_vision);
        assert_eq!(graph.inputs[0].element_type, 1);
        assert_eq!(graph.inputs[0].dimensions, [Some(1)]);
        assert_eq!(graph.external_files, ["weights/model.data"]);
        for invalid in [
            "../outside.data",
            "C:/outside.data",
            "/outside.data",
            "bad\\file",
        ] {
            crate::test_fixtures::onnx(&path, "x", "y", Some(invalid));
            assert!(OnnxProber::read_signature(&path).is_err());
        }
        Ok(())
    }
}
