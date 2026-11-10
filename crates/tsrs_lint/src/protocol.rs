//! Framed headless output, ported from tsgolint's `cmd/tsgolint/headless.go`.

use std::io::{self, Read, Write};

use tsrs_core::json::{self, JsonMap, Value};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum MessageType {
    Error = 0,
    Diagnostic = 1,
    Timing = 2,
    /// The answer to a [`InputType::Request`] under `--serve`: `{ "id", "result" }` or `{ "id", "error" }`.
    Response = 3,
}

/// Frames oxlint writes to stdin under `--serve`, with the output's framing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum InputType {
    /// The lint payload, once, first.
    Payload = 0,
    /// A query on the programs the payload built: `{ "id", "method", ... }`.
    Request = 1,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Range {
    pub pos: u32,
    pub end: u32,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RuleMessage {
    pub id: String,
    pub description: String,
    pub help: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Fix {
    pub text: String,
    pub range: Range,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Suggestion {
    pub message: RuleMessage,
    pub fixes: Vec<Fix>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LabeledRange {
    pub label: String,
    pub range: Range,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Diagnostic {
    pub range: Option<Range>,
    pub message: RuleMessage,
    pub file_path: Option<String>,
    pub labeled_ranges: Vec<LabeledRange>,
    pub rule: Option<String>,
    pub fixes: Vec<Fix>,
    pub suggestions: Vec<Suggestion>,
}

pub fn object(fields: impl IntoIterator<Item = (&'static str, Value)>) -> Value {
    Value::Object(fields.into_iter().map(|(key, value)| (key.to_owned(), value)).collect())
}

impl Range {
    pub fn to_json(self) -> Value {
        object([("pos", Value::Integer(self.pos.into())), ("end", Value::Integer(self.end.into()))])
    }
}

impl RuleMessage {
    pub fn to_json(&self) -> Value {
        let mut fields = JsonMap::from_iter([
            ("id".into(), Value::String(self.id.clone())),
            ("description".into(), Value::String(self.description.clone())),
        ]);
        if !self.help.is_empty() {
            fields.insert("help", Value::String(self.help.clone()));
        }
        Value::Object(fields)
    }
}

impl Fix {
    pub fn to_json(&self) -> Value {
        object([("text", Value::String(self.text.clone())), ("range", self.range.to_json())])
    }
}

impl Diagnostic {
    pub fn to_json(&self) -> Value {
        let mut fields = JsonMap::from_iter([
            ("kind".into(), Value::Integer(i64::from(self.rule.is_none()))),
            ("message".into(), self.message.to_json()),
            (
                "file_path".into(),
                self.file_path.as_ref().map_or(Value::Null, |s| Value::String(s.clone())),
            ),
        ]);
        if let Some(range) = self.range {
            fields.insert("range", range.to_json());
        }
        if let Some(rule) = &self.rule {
            fields.insert("rule", Value::String(rule.clone()));
        }
        if !self.labeled_ranges.is_empty() {
            fields.insert(
                "labeled_ranges",
                Value::Array(
                    self.labeled_ranges
                        .iter()
                        .map(|label| {
                            object([
                                ("label", Value::String(label.label.clone())),
                                ("range", label.range.to_json()),
                            ])
                        })
                        .collect(),
                ),
            );
        }
        if !self.fixes.is_empty() {
            fields.insert("fixes", Value::Array(self.fixes.iter().map(Fix::to_json).collect()));
        }
        if !self.suggestions.is_empty() {
            fields.insert(
                "suggestions",
                Value::Array(
                    self.suggestions
                        .iter()
                        .map(|suggestion| {
                            object([
                                ("message", suggestion.message.to_json()),
                                (
                                    "fixes",
                                    Value::Array(
                                        suggestion.fixes.iter().map(Fix::to_json).collect(),
                                    ),
                                ),
                            ])
                        })
                        .collect(),
                ),
            );
        }
        Value::Object(fields)
    }
}

pub fn write_message(
    writer: &mut dyn Write,
    message_type: MessageType,
    payload: &Value,
) -> io::Result<()> {
    let bytes = json::marshal(payload).map_err(io::Error::other)?.into_bytes();
    let length = u32::try_from(bytes.len()).map_err(io::Error::other)?;
    writer.write_all(&length.to_le_bytes())?;
    writer.write_all(&[message_type as u8])?;
    writer.write_all(&bytes)
}

pub fn write_error(writer: &mut dyn Write, error: &str) -> io::Result<()> {
    write_message(writer, MessageType::Error, &object([("error", Value::String(error.into()))]))
}

/// Reads one frame written as [`write_message`] writes it. `None` at end of input between frames.
pub fn read_frame(reader: &mut dyn Read) -> io::Result<Option<(u8, Vec<u8>)>> {
    let mut header = [0u8; 5];
    let mut filled = 0;
    while filled < header.len() {
        match reader.read(&mut header[filled..]) {
            Ok(0) if filled == 0 => return Ok(None),
            Ok(0) => return Err(io::ErrorKind::UnexpectedEof.into()),
            Ok(n) => filled += n,
            Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
            Err(error) => return Err(error),
        }
    }
    let length = u32::from_le_bytes([header[0], header[1], header[2], header[3]]) as usize;
    let mut payload = vec![0u8; length];
    reader.read_exact(&mut payload)?;
    Ok(Some((header[4], payload)))
}
