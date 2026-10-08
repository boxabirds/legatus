//! A registry source held in memory, for the reload tests: the text can be replaced, cut off or
//! removed between reloads without a disk.
use legatus_proxy::config::read::RegistrySource;
use std::io::{Error, ErrorKind};
use std::sync::Mutex;

pub struct MemorySource {
    label: String,
    bytes: Mutex<Option<Vec<u8>>>,
}

impl MemorySource {
    pub fn new(label: &str, text: &str) -> MemorySource {
        MemorySource { label: label.to_string(), bytes: Mutex::new(Some(text.as_bytes().to_vec())) }
    }

    pub fn set_text(&self, text: &str) {
        self.set_bytes(Some(text.as_bytes().to_vec()));
    }

    pub fn set_bytes(&self, bytes: Option<Vec<u8>>) {
        if let Ok(mut held) = self.bytes.lock() {
            *held = bytes;
        }
    }

    /// The file is gone: reading it fails as a missing file does.
    pub fn remove(&self) {
        self.set_bytes(None);
    }
}

impl RegistrySource for MemorySource {
    fn label(&self) -> String {
        self.label.clone()
    }

    fn read_bytes(&self) -> std::io::Result<Vec<u8>> {
        match self.bytes.lock() {
            Ok(held) => held.clone().ok_or_else(|| Error::new(ErrorKind::NotFound, "missing")),
            Err(_) => Err(Error::other("poisoned")),
        }
    }
}
