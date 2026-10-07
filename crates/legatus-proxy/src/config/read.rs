//! Read the registry file from the start path. Never writes it.
use crate::config::registry::*;
use crate::config::validate::validate_registry;
use std::io::Write;
use std::path::Path;

fn read_failure(path: &Path, kind: std::io::ErrorKind) -> RegistryError {
    let text = match kind {
        std::io::ErrorKind::NotFound => TEXT_FILE_MISSING,
        std::io::ErrorKind::PermissionDenied => TEXT_FILE_NOT_READABLE,
        _ => TEXT_FILE_CANNOT_READ,
    };
    error_fixed(ErrorCode::FileUnreadable, &path.display().to_string(), text)
}

/// Parse registry text. A parse error gives only the line and column, never the text.
pub fn parse_registry_text(text: &str, path_label: &str) -> Result<RawRegistry, RegistryError> {
    yaml_serde::from_str::<yaml_serde::Value>(text).map(RawRegistry).map_err(|error| {
        let place = error.location().map(|l| format!(" At line {}, column {}.", l.line(), l.column())).unwrap_or_default();
        RegistryError { code: ErrorCode::FileUnparsable, path: path_label.to_string(), text: format!("{TEXT_FILE_NOT_YAML}{place}") }
    })
}

/// Read, parse and check the whole file. `Ok` only when there is no error.
pub fn read_registry(path: &Path) -> Result<LoadedRegistry, Vec<RegistryError>> {
    let bytes = crate::net::file_system::read_file_bytes(path).map_err(|e| vec![read_failure(path, e.kind())])?;
    let label = path.display().to_string();
    let Ok(text) = String::from_utf8(bytes) else {
        return Err(vec![error_fixed(ErrorCode::FileUnparsable, &label, TEXT_FILE_NOT_UTF8)]);
    };
    let raw = parse_registry_text(&text, &label).map_err(|e| vec![e])?;
    let report = validate_registry(&raw);
    if report.errors.is_empty() {
        Ok(LoadedRegistry { raw, warnings: report.warnings })
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
