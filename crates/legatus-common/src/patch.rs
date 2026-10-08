//! The node patch (contract C64): a fixed set of `set` and `remove` operations on top-level keys
//! of a chat or Messages request. It exists so that thinking and output settings differ by
//! engine while the prompt bytes stay identical on every turn.
use serde_json::Value;

/// Keys no patch may touch: they carry the prompt, the tools or the way the answer is delivered.
pub const FORBIDDEN_PATCH_KEYS: &[&str] = &["messages", "tools", "system", "model", "stream", "functions"];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PatchOp {
    Set,
    Remove,
}

impl PatchOp {
    pub fn as_str(&self) -> &'static str {
        match self {
            PatchOp::Set => "set",
            PatchOp::Remove => "remove",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PatchError {
    ForbiddenKey { op: PatchOp, key: String },
    AdapterForbiddenKey { op: PatchOp, key: String },
    EmptyKey { op: PatchOp },
    SetRemoveClash { key: String },
    NotAnObject,
}

/// Why the text of a patch could not be read.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PatchParseError {
    NotAMap,
    SetNotAMap,
    RemoveNotAList,
    RemoveItemNotText,
}

/// The patch of a node, kept in sorted key order so the same patch always gives the same bytes.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct NodePatch {
    pub set: Vec<(String, Value)>,
    pub remove: Vec<String>,
}

impl NodePatch {
    pub fn is_empty(&self) -> bool {
        self.set.is_empty() && self.remove.is_empty()
    }

    /// Read the canonical JSON text of the registry: `{"set": {key: value}, "remove": [key]}`.
    pub fn from_json(text: &str) -> Result<NodePatch, PatchParseError> {
        let value: Value = serde_json::from_str(text).map_err(|_| PatchParseError::NotAMap)?;
        let map = value.as_object().ok_or(PatchParseError::NotAMap)?;
        let mut patch = NodePatch::default();
        match map.get("set") {
            None | Some(Value::Null) => {}
            Some(Value::Object(set)) => patch.set = set.iter().map(|(k, v)| (k.clone(), v.clone())).collect(),
            Some(_) => return Err(PatchParseError::SetNotAMap),
        }
        match map.get("remove") {
            None | Some(Value::Null) => {}
            Some(Value::Array(items)) => {
                for item in items {
                    patch.remove.push(item.as_str().ok_or(PatchParseError::RemoveItemNotText)?.to_string());
                }
            }
            Some(_) => return Err(PatchParseError::RemoveNotAList),
        }
        patch.set.sort_by(|a, b| a.0.cmp(&b.0));
        patch.remove.sort();
        patch.remove.dedup();
        Ok(patch)
    }
}
