//! The shared reader of the reused-token count in a reply. No adapter reads it any other way.
//! A small scanner walks the bytes of the tail and borrows the value it wants, so a reading
//! allocates nothing, good or bad.
use legatus_common::engine::{ReuseFieldName, ReuseReading, UnknownReason, UsageView};

/// A server-sent event line starts with this; the JSON follows.
const SSE_DATA_PREFIX: &[u8] = b"data:";

fn path_of(field: ReuseFieldName) -> &'static [&'static str] {
    match field {
        ReuseFieldName::TimingsCacheN => &["timings", "cache_n"],
        ReuseFieldName::PromptTokensDetailsCachedTokens => &["usage", "prompt_tokens_details", "cached_tokens"],
        ReuseFieldName::InputTokensDetailsCachedTokens => &["usage", "input_tokens_details", "cached_tokens"],
    }
}

fn skip_ws(b: &[u8], i: &mut usize) {
    while b.get(*i).is_some_and(u8::is_ascii_whitespace) {
        *i += 1;
    }
}

/// `*i` is at the opening quote; leaves it after the closing quote.
fn skip_string(b: &[u8], i: &mut usize) -> Option<()> {
    *i += 1;
    loop {
        match *b.get(*i)? {
            b'"' => {
                *i += 1;
                return Some(());
            }
            b'\\' => *i += 2,
            _ => *i += 1,
        }
    }
}

/// Skip one JSON value of any kind. Containers are skipped by counting brackets outside strings.
fn skip_value(b: &[u8], i: &mut usize) -> Option<()> {
    match *b.get(*i)? {
        b'"' => skip_string(b, i),
        b'{' | b'[' => {
            let mut depth = 0usize;
            loop {
                match *b.get(*i)? {
                    b'"' => {
                        skip_string(b, i)?;
                        continue;
                    }
                    b'{' | b'[' => depth += 1,
                    b'}' | b']' => {
                        depth -= 1;
                        if depth == 0 {
                            *i += 1;
                            return Some(());
                        }
                    }
                    _ => {}
                }
                *i += 1;
            }
        }
        _ => {
            let start = *i;
            while b.get(*i).is_some_and(|c| !matches!(c, b',' | b'}' | b']') && !c.is_ascii_whitespace()) {
                *i += 1;
            }
            (*i > start).then_some(())
        }
    }
}

/// The raw bytes of the value of `key` in the object `object`, or `None` when it is not an
/// object, the key is missing or the object is cut off.
fn member<'a>(object: &'a [u8], key: &str) -> Option<&'a [u8]> {
    let mut i = 0;
    skip_ws(object, &mut i);
    if *object.get(i)? != b'{' {
        return None;
    }
    i += 1;
    loop {
        skip_ws(object, &mut i);
        match *object.get(i)? {
            b'}' => return None,
            b',' => {
                i += 1;
                continue;
            }
            b'"' => {}
            _ => return None,
        }
        let key_start = i + 1;
        skip_string(object, &mut i)?;
        let found = &object[key_start..i - 1] == key.as_bytes();
        skip_ws(object, &mut i);
        if *object.get(i)? != b':' {
            return None;
        }
        i += 1;
        skip_ws(object, &mut i);
        let value_start = i;
        skip_value(object, &mut i)?;
        if found {
            return Some(&object[value_start..i]);
        }
    }
}

/// Read the named field. Absent gives `Unknown(FieldAbsent)`; a value that is not an unsigned
/// whole number (string, negative, fractional, null) gives `Unknown(FieldMalformed)`; 0 is a
/// valid reading (a cold turn). A tail that is not JSON holds no field.
pub fn extract_reuse(field: ReuseFieldName, view: &UsageView<'_>) -> ReuseReading {
    let tail = view.json_tail.trim_ascii();
    let mut at = tail.strip_prefix(SSE_DATA_PREFIX).unwrap_or(tail);
    for key in path_of(field) {
        match member(at, key) {
            Some(next) => at = next,
            None => return ReuseReading::Unknown(UnknownReason::FieldAbsent),
        }
    }
    match std::str::from_utf8(at).ok().and_then(|text| text.parse::<u64>().ok()) {
        Some(cached_tokens) => ReuseReading::Reused { cached_tokens, field },
        None => ReuseReading::Unknown(UnknownReason::FieldMalformed),
    }
}
