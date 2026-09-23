use std::collections::BTreeMap;

/// Storage area selector for state persistence.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum StorageArea {
    /// `sessionStorage`: per-tab, survives reload.
    Session,
    /// `localStorage`: persists across sessions.
    Local,
}

/// Namespaced storage key helper.
///
/// State maps are flat (`key -> bytes`); hosts namespace them per component
/// (e.g. `"essenty:<component>:<key>"`) and encode bytes off-crate (base64
/// or similar) before writing to Web Storage, keeping this crate free of
/// encoding opinions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StorageKey {
    area: StorageArea,
    namespace: String,
}

impl StorageKey {
    /// Creates a namespaced key helper.
    #[must_use]
    pub fn new(area: StorageArea, namespace: impl Into<String>) -> Self {
        Self { area, namespace: namespace.into() }
    }

    /// Storage area.
    #[must_use]
    pub fn area(self) -> StorageArea {
        self.area
    }

    /// Fully qualified storage key for a state entry.
    #[must_use]
    pub fn qualified(&self, key: &str) -> String {
        format!("essenty:{}:{key}", self.namespace)
    }

    #[cfg(target_arch = "wasm32")]
    pub(crate) fn prefix(&self) -> String {
        format!("essenty:{}:", self.namespace)
    }

    /// Qualifies every entry of a saved state map.
    #[must_use]
    pub fn qualify_all(&self, saved: &BTreeMap<String, Vec<u8>>) -> BTreeMap<String, Vec<u8>> {
        saved.iter().map(|(key, value)| (self.qualified(key), value.clone())).collect()
    }
}

#[cfg(any(target_arch = "wasm32", test))]
pub(crate) fn encode_bytes(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut encoded = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        encoded.push(HEX[(byte >> 4) as usize] as char);
        encoded.push(HEX[(byte & 0x0f) as usize] as char);
    }
    encoded
}

#[cfg(any(target_arch = "wasm32", test))]
pub(crate) fn decode_bytes(encoded: &str) -> Option<Vec<u8>> {
    fn digit(byte: u8) -> Option<u8> {
        match byte {
            b'0'..=b'9' => Some(byte - b'0'),
            b'a'..=b'f' => Some(byte - b'a' + 10),
            _ => None,
        }
    }

    let mut bytes = Vec::with_capacity(encoded.len() / 2);
    let mut chunks = encoded.as_bytes().chunks_exact(2);
    for chunk in &mut chunks {
        bytes.push(digit(chunk[0])? << 4 | digit(chunk[1])?);
    }
    chunks.remainder().is_empty().then_some(bytes)
}
