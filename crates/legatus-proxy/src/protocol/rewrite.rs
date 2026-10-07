//! Byte-faithful edits of the top-level members of a JSON request body (story 121).
//! One forward scan with depth and string state locates the members. No value is parsed into a
//! tree and none is re-serialised, so every byte outside an edited span is identical.
use bytes::{Bytes, BytesMut};

/// Why a body cannot be used.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RewriteError {
    InvalidJson,
    NotObject,
    ModelMissing,
    ModelNotString,
    /// Two top-level `model` keys (PROPOSED: refused, because a node and the proxy could read different ones).
    DuplicateModel,
}

/// One edit of a top-level member. `raw_value` is JSON text and is written as given.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TopLevelEdit {
    Replace { key: String, raw_value: Bytes },
    Insert { key: String, raw_value: Bytes },
    Remove { key: String },
}

/// Where one top-level member sits in the body.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Member {
    /// The decoded key.
    key: String,
    /// Start of the key's opening quote.
    key_start: usize,
    /// Value span, end exclusive.
    value_start: usize,
    value_end: usize,
}

/// Offsets of the top-level members of the body it was made from. After an edit the caller
/// makes a new peek; offsets of an old peek are not valid for the new body.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RequestPeek {
    members: Vec<Member>,
    /// Offset just after the opening brace.
    open_end: usize,
    /// The decoded value of `model`, when the body has exactly one string `model`.
    model: Option<String>,
    model_span: Option<(usize, usize)>,
    stream: Option<bool>,
}

impl RequestPeek {
    /// The decoded `model` value (the alias for a request from a harness).
    pub fn model(&self) -> Option<&str> {
        self.model.as_deref()
    }

    /// The `stream` flag: `Some(true)`, `Some(false)`, or `None` when absent or not a boolean.
    pub fn stream(&self) -> Option<bool> {
        self.stream
    }
}

struct Scanner<'a> {
    bytes: &'a [u8],
    at: usize,
}

const OBJECT_OPEN: u8 = b'{';
const OBJECT_CLOSE: u8 = b'}';
const ARRAY_OPEN: u8 = b'[';
const ARRAY_CLOSE: u8 = b']';
const QUOTE: u8 = b'"';
const BACKSLASH: u8 = b'\\';
const COMMA: u8 = b',';
const COLON: u8 = b':';
const MODEL_KEY: &str = "model";
const STREAM_KEY: &str = "stream";
const HEX_RADIX: u32 = 16;
const UNICODE_ESCAPE_DIGITS: usize = 4;
const HIGH_SURROGATE: std::ops::RangeInclusive<u32> = 0xD800..=0xDBFF;
const LOW_SURROGATE: std::ops::RangeInclusive<u32> = 0xDC00..=0xDFFF;
const SURROGATE_BASE: u32 = 0x10000;
const SURROGATE_SHIFT: u32 = 10;
const SURROGATE_MASK: u32 = 0x3FF;
const FIRST_PRINTABLE: u8 = 0x20;

impl<'a> Scanner<'a> {
    fn skip_ws(&mut self) {
        while self.at < self.bytes.len() && matches!(self.bytes[self.at], b' ' | b'\t' | b'\n' | b'\r') {
            self.at += 1;
        }
    }

    fn peek(&self) -> Option<u8> {
        self.bytes.get(self.at).copied()
    }

    /// Scan a string token from its opening quote; returns the decoded text and leaves `at` after the closing quote.
    fn string(&mut self) -> Result<String, RewriteError> {
        if self.peek() != Some(QUOTE) {
            return Err(RewriteError::InvalidJson);
        }
        self.at += 1;
        let mut out: Vec<u8> = Vec::new();
        loop {
            let b = *self.bytes.get(self.at).ok_or(RewriteError::InvalidJson)?;
            self.at += 1;
            match b {
                QUOTE => return String::from_utf8(out).map_err(|_| RewriteError::InvalidJson),
                BACKSLASH => {
                    let e = *self.bytes.get(self.at).ok_or(RewriteError::InvalidJson)?;
                    self.at += 1;
                    match e {
                        b'"' | b'\\' | b'/' => out.push(e),
                        b'b' => out.push(0x08),
                        b'f' => out.push(0x0C),
                        b'n' => out.push(b'\n'),
                        b'r' => out.push(b'\r'),
                        b't' => out.push(b'\t'),
                        b'u' => {
                            let ch = self.unicode_escape()?;
                            let mut buf = [0u8; 4];
                            out.extend_from_slice(ch.encode_utf8(&mut buf).as_bytes());
                        }
                        _ => return Err(RewriteError::InvalidJson),
                    }
                }
                b if b < FIRST_PRINTABLE => return Err(RewriteError::InvalidJson),
                b => out.push(b),
            }
        }
    }

    fn hex4(&mut self) -> Result<u32, RewriteError> {
        let digits = self.bytes.get(self.at..self.at + UNICODE_ESCAPE_DIGITS).ok_or(RewriteError::InvalidJson)?;
        let text = std::str::from_utf8(digits).map_err(|_| RewriteError::InvalidJson)?;
        let value = u32::from_str_radix(text, HEX_RADIX).map_err(|_| RewriteError::InvalidJson)?;
        self.at += UNICODE_ESCAPE_DIGITS;
        Ok(value)
    }

    fn unicode_escape(&mut self) -> Result<char, RewriteError> {
        let first = self.hex4()?;
        if HIGH_SURROGATE.contains(&first) {
            if self.peek() != Some(BACKSLASH) || self.bytes.get(self.at + 1) != Some(&b'u') {
                return Err(RewriteError::InvalidJson);
            }
            self.at += 2;
            let second = self.hex4()?;
            if !LOW_SURROGATE.contains(&second) {
                return Err(RewriteError::InvalidJson);
            }
            let code = SURROGATE_BASE + (((first & SURROGATE_MASK) << SURROGATE_SHIFT) | (second & SURROGATE_MASK));
            return char::from_u32(code).ok_or(RewriteError::InvalidJson);
        }
        char::from_u32(first).ok_or(RewriteError::InvalidJson)
    }

    /// Skip one value of any kind; returns nothing, `at` is after it.
    fn skip_value(&mut self) -> Result<(), RewriteError> {
        match self.peek().ok_or(RewriteError::InvalidJson)? {
            QUOTE => self.string().map(|_| ()),
            OBJECT_OPEN | ARRAY_OPEN => self.skip_container(),
            _ => self.skip_literal(),
        }
    }

    fn skip_literal(&mut self) -> Result<(), RewriteError> {
        let start = self.at;
        while self.at < self.bytes.len() && !matches!(self.bytes[self.at], b' ' | b'\t' | b'\n' | b'\r' | COMMA | OBJECT_CLOSE | ARRAY_CLOSE) {
            self.at += 1;
        }
        if self.at == start {
            return Err(RewriteError::InvalidJson);
        }
        let token = &self.bytes[start..self.at];
        let valid = matches!(token, b"true" | b"false" | b"null") || token.iter().all(|b| matches!(b, b'0'..=b'9' | b'-' | b'+' | b'.' | b'e' | b'E'));
        if valid {
            Ok(())
        } else {
            Err(RewriteError::InvalidJson)
        }
    }

    /// Skip an object or array by depth; strings are skipped whole so brackets inside them do not count.
    fn skip_container(&mut self) -> Result<(), RewriteError> {
        let mut stack: Vec<u8> = Vec::new();
        loop {
            let b = self.peek().ok_or(RewriteError::InvalidJson)?;
            match b {
                QUOTE => {
                    self.string()?;
                    continue;
                }
                OBJECT_OPEN => stack.push(OBJECT_CLOSE),
                ARRAY_OPEN => stack.push(ARRAY_CLOSE),
                OBJECT_CLOSE | ARRAY_CLOSE => {
                    if stack.pop() != Some(b) {
                        return Err(RewriteError::InvalidJson);
                    }
                    if stack.is_empty() {
                        self.at += 1;
                        return Ok(());
                    }
                }
                _ => {}
            }
            self.at += 1;
        }
    }
}

/// Scan the top-level object. The only structure checks are balanced brackets, closed strings,
/// `key: value` pairs and nothing after the closing brace.
fn scan(body: &[u8]) -> Result<RequestPeek, RewriteError> {
    let mut s = Scanner { bytes: body, at: 0 };
    s.skip_ws();
    match s.peek() {
        Some(OBJECT_OPEN) => {}
        Some(ARRAY_OPEN | QUOTE | b'0'..=b'9' | b'-') => return Err(RewriteError::NotObject),
        Some(b't' | b'f' | b'n') if s.skip_literal().is_ok() => return Err(RewriteError::NotObject),
        _ => return Err(RewriteError::InvalidJson),
    }
    s.at += 1;
    let open_end = s.at;
    let mut members: Vec<Member> = Vec::new();
    loop {
        s.skip_ws();
        match s.peek() {
            Some(OBJECT_CLOSE) if members.is_empty() => {
                s.at += 1;
                break;
            }
            Some(QUOTE) => {}
            _ => return Err(RewriteError::InvalidJson),
        }
        let key_start = s.at;
        let key = s.string()?;
        s.skip_ws();
        if s.peek() != Some(COLON) {
            return Err(RewriteError::InvalidJson);
        }
        s.at += 1;
        s.skip_ws();
        let value_start = s.at;
        s.skip_value()?;
        members.push(Member { key, key_start, value_start, value_end: s.at });
        s.skip_ws();
        match s.peek() {
            Some(COMMA) => s.at += 1,
            Some(OBJECT_CLOSE) => {
                s.at += 1;
                break;
            }
            _ => return Err(RewriteError::InvalidJson),
        }
    }
    s.skip_ws();
    if s.at != body.len() {
        return Err(RewriteError::InvalidJson);
    }
    let models: Vec<&Member> = members.iter().filter(|m| m.key == MODEL_KEY).collect();
    let (model, model_span) = match models.as_slice() {
        [one] if body[one.value_start] == QUOTE => {
            let mut inner = Scanner { bytes: body, at: one.value_start };
            (Some(inner.string()?), Some((one.value_start, one.value_end)))
        }
        _ => (None, None),
    };
    let stream = members.iter().find(|m| m.key == STREAM_KEY).and_then(|m| match &body[m.value_start..m.value_end] {
        b"true" => Some(true),
        b"false" => Some(false),
        _ => None,
    });
    Ok(RequestPeek { members, open_end, model, model_span, stream })
}

/// Locate the members of the body. Errors if the body is not a JSON object with exactly one
/// string `model`.
pub fn peek_request(body: &Bytes) -> Result<RequestPeek, RewriteError> {
    let peek = scan(body)?;
    let models = peek.members.iter().filter(|m| m.key == MODEL_KEY).count();
    match models {
        0 => Err(RewriteError::ModelMissing),
        1 if peek.model.is_some() => Ok(peek),
        1 => Err(RewriteError::ModelNotString),
        _ => Err(RewriteError::DuplicateModel),
    }
}

/// The JSON text of a string: quotes, backslashes and control characters escaped, all else as is.
pub fn json_string(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 2);
    out.push('"');
    for c in text.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < u32::from(FIRST_PRINTABLE) => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// A new body in which only the `model` value is replaced by the JSON string of `node_model`.
/// The peek must be a peek of this body, as returned by `peek_request`.
pub fn rewrite_model(body: &Bytes, peek: &RequestPeek, node_model: &str) -> Bytes {
    let Some((start, end)) = peek.model_span else {
        return body.clone();
    };
    let replacement = json_string(node_model);
    let mut out = BytesMut::with_capacity(body.len() + replacement.len());
    out.extend_from_slice(&body[..start]);
    out.extend_from_slice(replacement.as_bytes());
    out.extend_from_slice(&body[end..]);
    out.freeze()
}

/// Apply edits to top-level members. Each edit is applied to the result of the one before, with a
/// fresh scan, so the peek only has to describe the starting body. A body that no longer scans
/// is returned as it was.
pub fn splice_top_level(body: &Bytes, peek: &RequestPeek, edits: &[TopLevelEdit]) -> Bytes {
    let mut current = body.clone();
    let mut view = peek.clone();
    for edit in edits {
        let next = apply_edit(&current, &view, edit);
        match scan(&next) {
            Ok(fresh) => {
                current = next;
                view = fresh;
            }
            Err(_) => return current,
        }
    }
    current
}

fn apply_edit(body: &Bytes, peek: &RequestPeek, edit: &TopLevelEdit) -> Bytes {
    let find = |key: &str| peek.members.iter().position(|m| m.key == key);
    let mut out = BytesMut::with_capacity(body.len() + 64);
    match edit {
        TopLevelEdit::Replace { key, raw_value } | TopLevelEdit::Insert { key, raw_value } => match find(key) {
            Some(i) => {
                let m = &peek.members[i];
                out.extend_from_slice(&body[..m.value_start]);
                out.extend_from_slice(raw_value);
                out.extend_from_slice(&body[m.value_end..]);
            }
            None => {
                let member = format!("{}:", json_string(key));
                match peek.members.last() {
                    Some(last) => {
                        out.extend_from_slice(&body[..last.value_end]);
                        out.extend_from_slice(b",");
                        out.extend_from_slice(member.as_bytes());
                        out.extend_from_slice(raw_value);
                        out.extend_from_slice(&body[last.value_end..]);
                    }
                    None => {
                        out.extend_from_slice(&body[..peek.open_end]);
                        out.extend_from_slice(member.as_bytes());
                        out.extend_from_slice(raw_value);
                        out.extend_from_slice(&body[peek.open_end..]);
                    }
                }
            }
        },
        TopLevelEdit::Remove { key } => match find(key) {
            Some(i) => {
                let m = &peek.members[i];
                let (from, to) = if i > 0 {
                    (peek.members[i - 1].value_end, m.value_end)
                } else if let Some(next) = peek.members.get(1) {
                    (m.key_start, next.key_start)
                } else {
                    (m.key_start, m.value_end)
                };
                out.extend_from_slice(&body[..from]);
                out.extend_from_slice(&body[to..]);
            }
            None => return body.clone(),
        },
    }
    out.freeze()
}
