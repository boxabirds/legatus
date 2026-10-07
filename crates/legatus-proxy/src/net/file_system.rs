//! The one place the proxy reads a file from disk (the virtual tier forbids file I/O elsewhere).
use std::path::Path;

/// Read a whole file. The proxy never writes the registry file.
pub fn read_file_bytes(path: &Path) -> std::io::Result<Vec<u8>> {
    std::fs::read(path)
}
