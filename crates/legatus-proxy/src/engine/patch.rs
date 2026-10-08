//! Node patches (contract C64): the edits that set and remove top-level request keys, the path
//! rule, and the load-time check. The splice itself is `splice_top_level` of the protocol module;
//! there is no second one. Everything the patch does not name stays byte for byte as sent.
use crate::engine::EngineAdapter;
use crate::protocol::paths::RouteKind;
use crate::protocol::rewrite::{peek_request, splice_top_level, RequestPeek, TopLevelEdit};
use bytes::Bytes;
use legatus_common::patch::{NodePatch, PatchError, PatchOp, FORBIDDEN_PATCH_KEYS};
use serde_json::Value;

/// Only chat and Messages requests are patched; embeddings, audio, images, Responses and the
/// model list never are.
pub fn patch_applies_to(kind: RouteKind) -> bool {
    matches!(kind, RouteKind::Chat | RouteKind::Messages)
}

/// Every mistake of a patch, in registry order (set keys sorted, then remove keys sorted). The
/// text of a mistake names the key, never a value.
pub fn validate_patch(patch: &NodePatch, adapter: &dyn EngineAdapter) -> Result<(), Vec<PatchError>> {
    let mut errors = Vec::new();
    let mut check = |op: PatchOp, key: &str| {
        if key.is_empty() {
            errors.push(PatchError::EmptyKey { op });
        } else if FORBIDDEN_PATCH_KEYS.contains(&key) {
            errors.push(PatchError::ForbiddenKey { op, key: key.to_string() });
        } else if adapter.forbidden_added_fields().contains(&key) {
            errors.push(PatchError::AdapterForbiddenKey { op, key: key.to_string() });
        }
    };
    for (key, _) in &patch.set {
        check(PatchOp::Set, key);
    }
    for key in &patch.remove {
        check(PatchOp::Remove, key);
    }
    for (key, _) in &patch.set {
        if patch.remove.contains(key) {
            errors.push(PatchError::SetRemoveClash { key: key.clone() });
        }
    }
    if errors.is_empty() { Ok(()) } else { Err(errors) }
}

fn raw(value: &Value) -> Bytes {
    Bytes::from(serde_json::to_vec(value).unwrap_or_default())
}

/// The value to write for a `set`: a map merges one level into an existing map member (the patch
/// wins a clash, a deeper map is replaced whole); anything else replaces the member.
fn merged(existing: Option<&[u8]>, wanted: &Value) -> Value {
    if let (Some(bytes), Value::Object(patch_map)) = (existing, wanted) {
        if let Ok(Value::Object(mut current)) = serde_json::from_slice::<Value>(bytes) {
            for (k, v) in patch_map {
                current.insert(k.clone(), v.clone());
            }
            return Value::Object(current);
        }
    }
    wanted.clone()
}

/// The edits for `splice_top_level`. `body` is the body after the model rewrite and `peek` the
/// reading of that same body. A duplicate key sent by the harness is written once: the extra
/// occurrences are removed. New members are appended in sorted key order.
pub fn patch_edits(patch: &NodePatch, peek: &RequestPeek, body: &Bytes) -> Result<Vec<TopLevelEdit>, PatchError> {
    if !body.trim_ascii_start().starts_with(b"{") {
        return Err(PatchError::NotAnObject);
    }
    let mut edits = Vec::new();
    for key in &patch.remove {
        for _ in peek.value_spans(key) {
            edits.push(TopLevelEdit::Remove { key: key.clone() });
        }
    }
    for (key, wanted) in &patch.set {
        let spans = peek.value_spans(key);
        for _ in 1..spans.len() {
            edits.push(TopLevelEdit::Remove { key: key.clone() });
        }
        // After the extra copies are removed the last one stays, so its value is the one merged.
        let existing = spans.last().map(|(start, end)| &body[*start..*end]);
        let value = raw(&merged(existing, wanted));
        if spans.is_empty() {
            edits.push(TopLevelEdit::Insert { key: key.clone(), raw_value: value });
        } else {
            edits.push(TopLevelEdit::Replace { key: key.clone(), raw_value: value });
        }
    }
    Ok(edits)
}

/// The body with the patch applied. An empty patch returns the same bytes (a node without a patch
/// pays nothing). Applying twice gives what applying once gives.
pub fn apply_patch(body: &Bytes, peek: &RequestPeek, patch: &NodePatch) -> Result<Bytes, PatchError> {
    if patch.is_empty() {
        return Ok(body.clone());
    }
    let edits = patch_edits(patch, peek, body)?;
    Ok(splice_top_level(body, peek, &edits))
}

/// Patch a body that has just had its model rewritten: read it again (the offsets of the first
/// reading no longer match), then apply.
pub fn patch_rewritten_body(body: &Bytes, patch: &NodePatch) -> Result<Bytes, PatchError> {
    if patch.is_empty() {
        return Ok(body.clone());
    }
    let fresh = peek_request(body).map_err(|_| PatchError::NotAnObject)?;
    apply_patch(body, &fresh, patch)
}

/// Writes one warning event with the code `patch_changed` and the node name for every node whose
/// patch a reload changed (added, changed or removed). The cache of that node breaks once.
pub struct PatchChangeObserver {
    sink: std::sync::Arc<dyn crate::obs::log_sink::LogSink>,
}

impl PatchChangeObserver {
    pub fn new(sink: std::sync::Arc<dyn crate::obs::log_sink::LogSink>) -> PatchChangeObserver {
        PatchChangeObserver { sink }
    }
}

impl crate::config::reload::ReloadObserver for PatchChangeObserver {
    fn on_reload(&self, old: &crate::config::typed::Registry, new: &crate::config::typed::Registry, _diff: &crate::config::diff::RegistryDiff) {
        use crate::obs::log_sink::{LogRecord, SystemRecord};
        for node in &new.nodes {
            let Some(before) = old.node(&node.name) else { continue };
            if before.patch != node.patch {
                let record = SystemRecord { patch_changed: Some(node.name.clone()), ..SystemRecord::new(legatus_common::event::SystemEventKind::Warning) };
                let _ = self.sink.offer(LogRecord::System(record));
            }
        }
    }
}
