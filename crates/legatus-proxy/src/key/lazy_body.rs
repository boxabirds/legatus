//! The body as a key source: parsed on first use only, so a header key never costs a parse.
//! The message-derived key is story 156 and 170; until then `derived` offers nothing.
use crate::key::hasher::ConversationKey;
use crate::key::sources::BodyKeySource;
use bytes::Bytes;
use serde_json::Value;
use std::cell::OnceCell;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

/// Counts body parses made for a key, for tests.
#[derive(Clone, Debug, Default)]
pub struct KeyProbe(Arc<AtomicUsize>);

impl KeyProbe {
    pub fn parses(&self) -> usize {
        self.0.load(Ordering::SeqCst)
    }
}

pub struct LazyBody<'a> {
    body: &'a Bytes,
    parsed: OnceCell<Option<Value>>,
    probe: Option<KeyProbe>,
}

impl<'a> LazyBody<'a> {
    pub fn new(body: &'a Bytes, probe: Option<KeyProbe>) -> LazyBody<'a> {
        LazyBody { body, parsed: OnceCell::new(), probe }
    }

    fn value(&self) -> Option<&Value> {
        self.parsed
            .get_or_init(|| {
                if let Some(probe) = &self.probe {
                    probe.0.fetch_add(1, Ordering::SeqCst);
                }
                serde_json::from_slice(self.body).ok()
            })
            .as_ref()
    }
}

impl BodyKeySource for LazyBody<'_> {
    fn body_field(&self, name: &str) -> Option<Vec<u8>> {
        match self.value()?.get(name)? {
            Value::String(text) => Some(text.as_bytes().to_vec()),
            Value::Number(number) => Some(number.to_string().into_bytes()),
            _ => None,
        }
    }

    fn derived(&self) -> Option<ConversationKey> {
        None
    }
}
