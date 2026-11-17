use serde_json::{json, Value};
use sha2::{Digest, Sha256};

pub const MAX_HEADER: usize = 32 * 1024 * 1024;
pub const DEFAULT_BLOCK_SIZE: usize = 4 * 1024 * 1024;

pub fn sha256_hex(data: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(data);
    hex::encode(hasher.finalize())
}

#[derive(Debug, Clone)]
pub struct AsarArchive {
    raw_header: Vec<u8>,
    header: Value,
    body: Vec<u8>,
}

impl AsarArchive {
    /// Parse an ASAR binary archive and validate its Chromium pickle header bounds.
    pub fn parse(data: &[u8]) -> Result<Self, String> {
        if data.len() < 16 {
            return Err("ASAR header is truncated".to_string());
        }
        let size = u32::from_le_bytes(data[0..4].try_into().unwrap());
        let header_size = u32::from_le_bytes(data[4..8].try_into().unwrap());
        let payload_size = u32::from_le_bytes(data[8..12].try_into().unwrap());
        let json_size = u32::from_le_bytes(data[12..16].try_into().unwrap());

        let header_size_usize = header_size as usize;
        if size != 4
            || header_size < 8
            || header_size_usize > MAX_HEADER
            || payload_size.checked_add(4) != Some(header_size)
            || json_size > payload_size.saturating_sub(4)
            || 8usize
                .checked_add(header_size_usize)
                .map_or(true, |end| end > data.len())
        {
            return Err("Invalid ASAR header".to_string());
        }

        let json_end = 16usize + (json_size as usize);
        let raw_header = data[16..json_end].to_vec();
        let header: Value = serde_json::from_slice(&raw_header)
            .map_err(|e| format!("Invalid ASAR header JSON: {e}"))?;
        if !header.get("files").is_some_and(Value::is_object) {
            return Err("Missing ASAR file table".to_string());
        }
        let body = data[8 + header_size_usize..].to_vec();

        Ok(Self {
            raw_header,
            header,
            body,
        })
    }

    pub fn raw_header_sha256(&self) -> String {
        sha256_hex(&self.raw_header)
    }

    #[cfg(test)]
    pub(crate) fn header_mut(&mut self) -> &mut Value {
        &mut self.header
    }

    #[cfg(test)]
    pub(crate) fn set_body(&mut self, body: Vec<u8>) {
        self.body = body;
    }

    fn validate_parts(name: &str) -> Result<Vec<&str>, String> {
        if name.is_empty() {
            return Err("Invalid archive path".to_string());
        }
        let parts: Vec<&str> = name.split('/').collect();
        if parts
            .iter()
            .any(|part| part.is_empty() || *part == "." || *part == "..")
        {
            return Err("Invalid archive path".to_string());
        }
        Ok(parts)
    }

    fn lookup_entry<'a>(root: &'a Value, name: &str) -> Result<&'a Value, String> {
        let parts = Self::validate_parts(name)?;
        let mut item = root;
        for part in parts {
            item = item
                .get("files")
                .and_then(|files| files.get(part))
                .ok_or_else(|| format!("Missing ASAR entry: {name}"))?;
        }
        if item.get("unpacked").and_then(Value::as_bool).unwrap_or(false)
            || item.get("link").is_some()
            || item.get("files").is_some()
        {
            return Err("Expected a packed regular file".to_string());
        }
        Ok(item)
    }

    fn lookup_entry_mut<'a>(root: &'a mut Value, name: &str) -> Result<&'a mut Value, String> {
        let parts = Self::validate_parts(name)?;
        let mut item = root;
        for part in parts {
            item = item
                .get_mut("files")
                .and_then(|files| files.get_mut(part))
                .ok_or_else(|| format!("Missing ASAR entry: {name}"))?;
        }
        if item.get("unpacked").and_then(Value::as_bool).unwrap_or(false)
            || item.get("link").is_some()
            || item.get("files").is_some()
        {
            return Err("Expected a packed regular file".to_string());
        }
        Ok(item)
    }

    pub fn entry(&self, name: &str) -> Result<&Value, String> {
        Self::lookup_entry(&self.header, name)
    }

    pub fn has_regular_file(&self, name: &str) -> bool {
        self.entry(name).is_ok()
    }

    pub fn read(&self, name: &str) -> Result<Vec<u8>, String> {
        let entry = self.entry(name)?;
        let offset = parse_offset(entry.get("offset"))?;
        let size = entry
            .get("size")
            .and_then(Value::as_u64)
            .and_then(|v| usize::try_from(v).ok())
            .ok_or_else(|| "Invalid ASAR entry size".to_string())?;
        let end = offset
            .checked_add(size)
            .ok_or_else(|| "Entry outside ASAR bounds".to_string())?;
        if end > self.body.len() {
            return Err("Entry outside ASAR bounds".to_string());
        }
        let slice = &self.body[offset..end];
        if let Some(integrity) = entry.get("integrity") {
            let algo = integrity.get("algorithm").and_then(Value::as_str);
            let expected_hash = integrity.get("hash").and_then(Value::as_str);
            if algo != Some("SHA256") || expected_hash != Some(sha256_hex(slice).as_str()) {
                return Err("Entry checksum mismatch".to_string());
            }
        }
        Ok(slice.to_vec())
    }

    #[allow(dead_code)]
    pub fn replace(&self, name: &str, data: Vec<u8>) -> Result<Vec<u8>, String> {
        self.replace_many(&[(name.to_string(), data)])
    }

    /// Replace one or more packed entries and recompute ASAR pickle header & SHA-256 blocks.
    pub fn replace_many(&self, updates: &[(String, Vec<u8>)]) -> Result<Vec<u8>, String> {
        let mut header = self.header.clone();
        let mut body = self.body.clone();

        for (name, data) in updates {
            // Validate bounds and original checksum before modifying header metadata.
            self.read(name)?;
            let item = Self::lookup_entry_mut(&mut header, name)?;
            let block_size = match item.get("integrity").and_then(|i| i.get("blockSize")) {
                None => DEFAULT_BLOCK_SIZE,
                Some(v) => {
                    let bs = v
                        .as_u64()
                        .and_then(|n| usize::try_from(n).ok())
                        .ok_or_else(|| "Invalid integrity block size".to_string())?;
                    if bs == 0 || bs > MAX_HEADER {
                        return Err("Invalid integrity block size".to_string());
                    }
                    bs
                }
            };

            let offset = body.len();
            body.extend_from_slice(data);
            let blocks: Vec<String> = data.chunks(block_size).map(sha256_hex).collect();
            let obj = item
                .as_object_mut()
                .ok_or_else(|| "Expected object ASAR entry".to_string())?;
            obj.insert("size".to_string(), json!(data.len()));
            obj.insert("offset".to_string(), json!(offset.to_string()));
            obj.insert(
                "integrity".to_string(),
                json!({
                    "algorithm": "SHA256",
                    "hash": sha256_hex(data),
                    "blockSize": block_size,
                    "blocks": blocks,
                }),
            );
        }

        serialize_asar(&header, &body)
    }
}

fn parse_offset(val: Option<&Value>) -> Result<usize, String> {
    match val {
        Some(Value::String(s)) => s
            .parse::<usize>()
            .map_err(|_| "Entry outside ASAR bounds".to_string()),
        Some(Value::Number(n)) => n
            .as_u64()
            .and_then(|v| usize::try_from(v).ok())
            .ok_or_else(|| "Entry outside ASAR bounds".to_string()),
        _ => Err("Invalid ASAR entry offset".to_string()),
    }
}

pub fn serialize_asar(header: &Value, body: &[u8]) -> Result<Vec<u8>, String> {
    let raw = serde_json::to_vec(header).map_err(|e| format!("Failed to encode ASAR header: {e}"))?;
    let pad_len = (4 - (raw.len() % 4)) % 4;
    let payload_size = 4u32
        .checked_add(u32::try_from(raw.len()).map_err(|_| "ASAR header too large")?)
        .and_then(|n| n.checked_add(pad_len as u32))
        .ok_or_else(|| "ASAR header too large".to_string())?;
    let header_size = payload_size
        .checked_add(4)
        .ok_or_else(|| "ASAR header too large".to_string())?;
    if (header_size as usize) > MAX_HEADER {
        return Err("ASAR header exceeds maximum size".to_string());
    }
    let json_size = u32::try_from(raw.len()).map_err(|_| "ASAR header too large")?;

    let mut out = Vec::with_capacity(8 + (header_size as usize) + body.len());
    out.extend_from_slice(&4u32.to_le_bytes());
    out.extend_from_slice(&header_size.to_le_bytes());
    out.extend_from_slice(&payload_size.to_le_bytes());
    out.extend_from_slice(&json_size.to_le_bytes());
    out.extend_from_slice(&raw);
    out.resize(out.len() + pad_len, 0u8);
    out.extend_from_slice(body);
    Ok(out)
}

#[cfg(test)]
pub(crate) fn build_test_archive(files: &[(&str, &[u8])]) -> Vec<u8> {
    build_test_archive_with_block_size(files, 4)
}

#[cfg(test)]
pub(crate) fn build_test_archive_with_block_size(
    files: &[(&str, &[u8])],
    block_size: usize,
) -> Vec<u8> {
    let mut root = json!({ "files": {} });
    let mut body = Vec::new();
    for (name, data) in files {
        let parts: Vec<&str> = name.split('/').collect();
        let mut cursor = root.get_mut("files").unwrap();
        for part in &parts[..parts.len() - 1] {
            if cursor.get(*part).is_none() {
                cursor
                    .as_object_mut()
                    .unwrap()
                    .insert((*part).to_string(), json!({ "files": {} }));
            }
            cursor = cursor.get_mut(*part).unwrap().get_mut("files").unwrap();
        }
        let blocks: Vec<String> = data.chunks(block_size).map(sha256_hex).collect();
        let offset = body.len();
        body.extend_from_slice(data);
        cursor.as_object_mut().unwrap().insert(
            parts[parts.len() - 1].to_string(),
            json!({
                "size": data.len(),
                "offset": offset.to_string(),
                "integrity": {
                    "algorithm": "SHA256",
                    "hash": sha256_hex(data),
                    "blockSize": block_size,
                    "blocks": blocks,
                }
            }),
        );
    }
    serialize_asar(&root, &body).unwrap()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn replacement_retains_other_files_and_correct_integrity() {
        let raw = build_test_archive(&[
            ("dist/preload.js", b"hello"),
            ("asset", b"\x00\xffdata"),
        ]);
        let original = AsarArchive::parse(&raw).unwrap();
        let replacement = "中文 test".as_bytes().to_vec();
        let rebuilt_bytes = original
            .replace("dist/preload.js", replacement.clone())
            .unwrap();
        let rebuilt = AsarArchive::parse(&rebuilt_bytes).unwrap();
        assert_eq!(rebuilt.read("dist/preload.js").unwrap(), replacement);
        assert_eq!(rebuilt.read("asset").unwrap(), b"\x00\xffdata");
        let blocks = rebuilt
            .entry("dist/preload.js")
            .unwrap()
            .get("integrity")
            .unwrap()
            .get("blocks")
            .unwrap()
            .as_array()
            .unwrap();
        let expected: Vec<String> = replacement.chunks(4).map(sha256_hex).collect();
        let actual: Vec<String> = blocks
            .iter()
            .map(|v| v.as_str().unwrap().to_string())
            .collect();
        assert_eq!(actual, expected);
    }

    #[test]
    fn corrupt_header_bounds_and_checksums_rejected() {
        let mut oversized = Vec::new();
        oversized.extend_from_slice(&4u32.to_le_bytes());
        oversized.extend_from_slice(&(1u32 << 30).to_le_bytes());
        oversized.extend_from_slice(&0u32.to_le_bytes());
        oversized.extend_from_slice(&0u32.to_le_bytes());
        for bad in [&b""[..], &b"0000000000000000"[..], &oversized[..]] {
            assert!(AsarArchive::parse(bad).is_err());
        }

        let raw = build_test_archive(&[("file", b"foo")]);
        let mut bad_offset = AsarArchive::parse(&raw).unwrap();
        bad_offset.header_mut()["files"]["file"]["offset"] = json!("999999");
        assert!(bad_offset.read("file").is_err());

        let mut bad_body = AsarArchive::parse(&raw).unwrap();
        bad_body.set_body(b"bar".to_vec());
        assert!(bad_body.read("file").is_err());
    }

    #[test]
    fn rejects_unpacked_symlink_and_traversal() {
        let raw = build_test_archive(&[("file", b"foo")]);
        let normal = AsarArchive::parse(&raw).unwrap();
        assert!(normal.read("../file").is_err());
        assert!(normal.read("./file").is_err());
        assert!(normal.read("/file").is_err());

        let mut unpacked = AsarArchive::parse(&raw).unwrap();
        unpacked.header_mut()["files"]["file"]["unpacked"] = json!(true);
        assert!(unpacked.read("file").is_err());

        let mut linked = AsarArchive::parse(&raw).unwrap();
        linked.header_mut()["files"]["file"]["link"] = json!("other");
        assert!(linked.read("file").is_err());
    }
}
