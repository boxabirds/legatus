//! Read the registry file from the start path. Never writes it.
use crate::config::alias::read_aliases;
use crate::config::node::{read_machines, read_nodes, TEXT_DUPLICATE_NAME};
use crate::config::registry::*;
use crate::config::routes::read_routes;
use crate::config::settings::read_settings;
use crate::config::validate::validate_registry;
use std::io::Write;
use std::path::Path;

/// Part of the parser message for a repeated key (yaml_serde 0.10.7).
const PARSER_DUPLICATE_MARK: &str = "duplicate entry";

fn read_failure(label: &str, kind: std::io::ErrorKind) -> RegistryError {
    let text = match kind {
        std::io::ErrorKind::NotFound => TEXT_FILE_MISSING,
        std::io::ErrorKind::PermissionDenied => TEXT_FILE_NOT_READABLE,
        _ => TEXT_FILE_CANNOT_READ,
    };
    error_fixed(ErrorCode::FileUnreadable, label, text)
}

/// Parse registry text. A parse error gives only the line and column, never the text.
pub fn parse_registry_text(text: &str, path_label: &str) -> Result<RawRegistry, RegistryError> {
    yaml_serde::from_str::<yaml_serde::Value>(text).map(RawRegistry).map_err(|error| {
        let place = error.location().map(|l| format!(" At line {}, column {}.", l.line(), l.column())).unwrap_or_default();
        // The parser refuses a repeated key in one map. Its message names the key, so only the
        // start of the message is used to choose the code and the key is never copied.
        let message = error.to_string();
        if let Some(at) = message.find(PARSER_DUPLICATE_MARK) {
            // For a nested map the parser starts its message with the path of that map.
            let parent = message[..at].trim_end_matches(": ");
            let path = if parent.is_empty() { path_label.to_string() } else { parent.to_string() };
            return RegistryError { code: ErrorCode::DuplicateName, path, text: format!("{TEXT_DUPLICATE_NAME}{place}") };
        }
        RegistryError { code: ErrorCode::FileUnparsable, path: path_label.to_string(), text: format!("{TEXT_FILE_NOT_YAML}{place}") }
    })
}

/// Where the registry bytes come from. The file is the real one; a test supplies text in memory,
/// which keeps the reload tests free of disk.
pub trait RegistrySource: Send + Sync {
    /// The name shown in errors (the path of the file).
    fn label(&self) -> String;
    fn read_bytes(&self) -> std::io::Result<Vec<u8>>;
}

/// The registry file on disk.
pub struct FileSource(pub std::path::PathBuf);

impl RegistrySource for FileSource {
    fn label(&self) -> String {
        self.0.display().to_string()
    }
    fn read_bytes(&self) -> std::io::Result<Vec<u8>> {
        crate::net::file_system::read_file_bytes(&self.0)
    }
}

/// Read, parse and check everything a source gives. `Ok` only when there is no error.
pub fn read_registry_from(source: &dyn RegistrySource) -> Result<LoadedRegistry, Vec<RegistryError>> {
    let label = source.label();
    let bytes = source.read_bytes().map_err(|e| vec![read_failure(&label, e.kind())])?;
    let Ok(text) = String::from_utf8(bytes) else {
        return Err(vec![error_fixed(ErrorCode::FileUnparsable, &label, TEXT_FILE_NOT_UTF8)]);
    };
    read_registry_text(&text, &label)
}

/// Read, parse and check the whole file. `Ok` only when there is no error.
pub fn read_registry(path: &Path) -> Result<LoadedRegistry, Vec<RegistryError>> {
    read_registry_from(&FileSource(path.to_path_buf()))
}

/// The same as `read_registry` for text that is already in memory (no file is read).
pub fn read_registry_text(text: &str, label: &str) -> Result<LoadedRegistry, Vec<RegistryError>> {
    let raw = parse_registry_text(text, label).map_err(|e| vec![e])?;
    let report = validate_registry(&raw);
    if report.errors.is_empty() {
        let mut scratch = ValidationReport::default();
        let nodes = read_nodes(&raw.0, &mut scratch);
        let machines = read_machines(&raw.0, &mut scratch);
        let aliases = read_aliases(&raw.0, &nodes, &mut scratch).unwrap_or_default();
        let root = raw.0.as_mapping();
        let settings = read_settings(root.and_then(|r| r.get("settings")), &aliases, &mut scratch).expect("a registry without errors has valid settings");
        let alias_names: Vec<String> = aliases.names().iter().map(|a| a.0.clone()).collect();
        let routes = read_routes(root.and_then(|r| r.get("routes")), &alias_names, &mut scratch);
        Ok(LoadedRegistry { raw, warnings: report.warnings, nodes, machines, aliases, settings, routes })
    } else {
        Err(report.errors)
    }
}

/// One line per error, then the count.
pub fn print_errors(errors: &[RegistryError], out: &mut dyn Write) {
    for error in errors {
        let _ = writeln!(out, "{error}");
    }
    let noun = if errors.len() == 1 { "error" } else { "errors" };
    let _ = writeln!(out, "registry rejected: {} {noun}", errors.len());
}
