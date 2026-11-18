//! Framed headless output, ported from tsgolint's `cmd/tsgolint/headless.go`.

use std::io::{self, Write};

use tsrs_core::json::{self, JsonMap, Value};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum MessageType {
    Error = 0,
    Diagnostic = 1,
    Timing = 2,
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
    // Type-check diagnostic category ("error", "warning", or "message"). Present only on `kind: 1` frames.
    pub category: Option<String>,
}

pub fn object(fields: impl IntoIterator<Item = (&'static str, Value)>) -> Value {
    Value::Object(fields.into_iter().map(|(key, value)| (key.to_owned(), value)).collect())
}

impl Range {
    pub fn to_json(self) -> Value {
        object([("pos", Value::Integer(self.pos.into())), ("end", Value::Integer(self.end.into()))])
    }

    fn write_json(self, out: &mut String) {
        let Self { pos, end } = self;
        out.push_str("{\"pos\":");
        json::write_compact_int(out, pos.into());
        out.push_str(",\"end\":");
        json::write_compact_int(out, end.into());
        out.push('}');
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

    fn write_json(&self, out: &mut String) {
        let Self { id, description, help } = self;
        out.push_str("{\"id\":");
        json::write_compact_string(out, id);
        out.push_str(",\"description\":");
        json::write_compact_string(out, description);
        if !help.is_empty() {
            out.push_str(",\"help\":");
            json::write_compact_string(out, help);
        }
        out.push('}');
    }
}

impl Fix {
    pub fn to_json(&self) -> Value {
        object([("text", Value::String(self.text.clone())), ("range", self.range.to_json())])
    }

    fn write_json(&self, out: &mut String) {
        let Self { text, range } = self;
        out.push_str("{\"text\":");
        json::write_compact_string(out, text);
        out.push_str(",\"range\":");
        range.write_json(out);
        out.push('}');
    }
}

impl Suggestion {
    fn write_json(&self, out: &mut String) {
        let Self { message, fixes } = self;
        out.push_str("{\"message\":");
        message.write_json(out);
        out.push_str(",\"fixes\":");
        write_array(out, fixes, Fix::write_json);
        out.push('}');
    }
}

impl LabeledRange {
    fn write_json(&self, out: &mut String) {
        let Self { label, range } = self;
        out.push_str("{\"label\":");
        json::write_compact_string(out, label);
        out.push_str(",\"range\":");
        range.write_json(out);
        out.push('}');
    }
}

fn write_array<T>(out: &mut String, items: &[T], write: impl Fn(&T, &mut String)) {
    out.push('[');
    for (index, item) in items.iter().enumerate() {
        if index != 0 {
            out.push(',');
        }
        write(item, out);
    }
    out.push(']');
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
        if self.rule.is_none()
            && let Some(category) = &self.category
        {
            fields.insert("category", Value::String(category.clone()));
        }
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

    fn write_json(&self, out: &mut String) {
        let Self { range, message, file_path, labeled_ranges, rule, fixes, suggestions, category } =
            self;
        out.push_str(if rule.is_none() {
            "{\"kind\":1,\"message\":"
        } else {
            "{\"kind\":0,\"message\":"
        });
        message.write_json(out);
        out.push_str(",\"file_path\":");
        if let Some(file_path) = file_path {
            json::write_compact_string(out, file_path);
        } else {
            out.push_str("null");
        }
        if rule.is_none()
            && let Some(category) = category
        {
            out.push_str(",\"category\":");
            json::write_compact_string(out, category);
        }
        if let Some(range) = range {
            out.push_str(",\"range\":");
            range.write_json(out);
        }
        if let Some(rule) = rule {
            out.push_str(",\"rule\":");
            json::write_compact_string(out, rule);
        }
        if !labeled_ranges.is_empty() {
            out.push_str(",\"labeled_ranges\":");
            write_array(out, labeled_ranges, LabeledRange::write_json);
        }
        if !fixes.is_empty() {
            out.push_str(",\"fixes\":");
            write_array(out, fixes, Fix::write_json);
        }
        if !suggestions.is_empty() {
            out.push_str(",\"suggestions\":");
            write_array(out, suggestions, Suggestion::write_json);
        }
        out.push('}');
    }
}

/// Writes a diagnostic frame directly from its borrowed fields.
pub fn write_diagnostic(writer: &mut dyn Write, diagnostic: &Diagnostic) -> io::Result<()> {
    let mut payload = String::new();
    diagnostic.write_json(&mut payload);
    write_frame(writer, MessageType::Diagnostic, payload.as_bytes())
}

pub fn write_message(
    writer: &mut dyn Write,
    message_type: MessageType,
    payload: &Value,
) -> io::Result<()> {
    let bytes = json::marshal(payload).map_err(io::Error::other)?.into_bytes();
    write_frame(writer, message_type, &bytes)
}

fn write_frame(writer: &mut dyn Write, message_type: MessageType, bytes: &[u8]) -> io::Result<()> {
    let length = u32::try_from(bytes.len()).map_err(io::Error::other)?;
    writer.write_all(&length.to_le_bytes())?;
    writer.write_all(&[message_type as u8])?;
    writer.write_all(bytes)
}

pub fn write_error(writer: &mut dyn Write, error: &str) -> io::Result<()> {
    write_message(writer, MessageType::Error, &object([("error", Value::String(error.into()))]))
}
