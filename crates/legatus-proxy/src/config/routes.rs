//! Routes and their key maps (story 165 owns this part of the registry; story 180 cites it).
//! The meaning of a key source belongs to story 126; this file checks the kind names and the shape.
use crate::config::registry::*;
use crate::config::schema::join_path;
use crate::config::validate::RegistryCheck;
use legatus_common::ids::AliasName;
use yaml_serde::{Mapping, Value};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KeySourceKind {
    Header,
    HeaderPair,
    BodyField,
    BodyHash,
    Credential,
    None,
}

impl KeySourceKind {
    fn parse(word: &str) -> Option<KeySourceKind> {
        match word {
            "header" => Some(KeySourceKind::Header),
            "header_pair" => Some(KeySourceKind::HeaderPair),
            "body_field" => Some(KeySourceKind::BodyField),
            "body_hash" => Some(KeySourceKind::BodyHash),
            "credential" => Some(KeySourceKind::Credential),
            "none" => Some(KeySourceKind::None),
            _ => None,
        }
    }
}

/// One source of a key map: the kind and the header or field names it needs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KeyMapEntry {
    pub kind: KeySourceKind,
    pub names: Vec<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum RouteScope {
    #[default]
    Alias,
    AliasPlusCredential,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RouteSpec {
    pub path: String,
    pub alias: Option<AliasName>,
    pub key_map: Vec<KeyMapEntry>,
    pub scope: RouteScope,
}

pub const TEXT_SOURCE_NOT_LISTED: &str = "Source kind is not in the list.";
pub const TEXT_SOURCE_SHAPE: &str = "a source with its names";
const HEADER_PAIR_LEN: usize = 2;

fn read_entry(entry: &Value, path: &str, out: &mut ValidationReport) -> Option<KeyMapEntry> {
    match entry {
        Value::String(word) => match KeySourceKind::parse(word) {
            Some(kind @ (KeySourceKind::BodyHash | KeySourceKind::Credential | KeySourceKind::None)) => Some(KeyMapEntry { kind, names: Vec::new() }),
            Some(_) => {
                out.errors.push(error_wrong_type(path, TEXT_SOURCE_SHAPE, FoundKind::Str));
                None
            }
            None => {
                out.errors.push(error_fixed(ErrorCode::UnknownField, path, TEXT_SOURCE_NOT_LISTED));
                None
            }
        },
        Value::Mapping(map) if map.len() == 1 => read_named_entry(map, path, out),
        other => {
            out.errors.push(error_wrong_type(path, TEXT_SOURCE_SHAPE, FoundKind::of(other)));
            None
        }
    }
}

fn read_named_entry(map: &Mapping, path: &str, out: &mut ValidationReport) -> Option<KeyMapEntry> {
    let (key, value) = map.iter().next()?;
    let name = key.as_str()?;
    let source_path = join_path(path, name);
    let Some(kind) = KeySourceKind::parse(name) else {
        out.errors.push(error_fixed(ErrorCode::UnknownField, &source_path, TEXT_SOURCE_NOT_LISTED));
        return None;
    };
    match (kind, value) {
        (KeySourceKind::Header | KeySourceKind::BodyField, Value::String(n)) => Some(KeyMapEntry { kind, names: vec![n.clone()] }),
        (KeySourceKind::HeaderPair, Value::Sequence(items)) if items.len() == HEADER_PAIR_LEN && items.iter().all(Value::is_string) => {
            Some(KeyMapEntry { kind, names: items.iter().filter_map(|i| i.as_str().map(str::to_string)).collect() })
        }
        _ => {
            out.errors.push(error_wrong_type(&source_path, TEXT_SOURCE_SHAPE, FoundKind::of(value)));
            None
        }
    }
}

fn read_route(index: usize, route: &Mapping, alias_names: &[String], out: &mut ValidationReport) -> Option<RouteSpec> {
    let errors_before = out.errors.len();
    let path = join_path("routes", &index.to_string());
    let route_path = route.get("path").and_then(Value::as_str)?;
    let alias = route.get("alias").and_then(Value::as_str).map(|name| {
        if !alias_names.iter().any(|a| a == name) {
            out.errors.push(error_fixed(ErrorCode::UnknownRef, &join_path(&path, "alias"), TEXT_ALIAS_MISSING));
        }
        AliasName(name.to_string())
    });
    let scope = match route.get("scope").and_then(Value::as_str) {
        None => RouteScope::Alias,
        Some("alias") => RouteScope::Alias,
        Some("alias_plus_credential") => RouteScope::AliasPlusCredential,
        Some(_) => {
            out.errors.push(error_fixed(ErrorCode::BadType, &join_path(&path, "scope"), crate::config::node::TEXT_NOT_ALLOWED_VALUE));
            RouteScope::Alias
        }
    };
    let mut key_map = Vec::new();
    if let Some(list) = route.get("key_map").and_then(Value::as_sequence) {
        for (j, entry) in list.iter().enumerate() {
            if let Some(e) = read_entry(entry, &join_path(&join_path(&path, "key_map"), &j.to_string()), out) {
                key_map.push(e);
            }
        }
    }
    (out.errors.len() == errors_before).then(|| RouteSpec { path: route_path.to_string(), alias, key_map, scope })
}

pub const TEXT_ALIAS_MISSING: &str = "Name does not exist.";

/// Read every route. Errors are appended; the list holds only routes without an error.
pub fn read_routes(tree: Option<&Value>, alias_names: &[String], out: &mut ValidationReport) -> Vec<RouteSpec> {
    let Some(list) = tree.and_then(Value::as_sequence) else { return Vec::new() };
    list.iter().enumerate().filter_map(|(i, r)| r.as_mapping().and_then(|m| read_route(i, m, alias_names, out))).collect()
}

/// The routes check of the registry check list.
pub struct RouteCheck;

impl RegistryCheck for RouteCheck {
    fn check(&self, doc: &RawRegistry, out: &mut ValidationReport) {
        let root = doc.0.as_mapping();
        let aliases: Vec<String> = root.and_then(|r| r.get("aliases")).and_then(Value::as_mapping).map(|m| m.keys().filter_map(|k| k.as_str().map(str::to_string)).collect()).unwrap_or_default();
        read_routes(root.and_then(|r| r.get("routes")), &aliases, out);
    }
}
