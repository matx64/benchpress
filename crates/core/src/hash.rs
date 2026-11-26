//! Canonical JSON and content hashes.

use serde::Serialize;
use sha2::{Digest, Sha256};

use crate::Error;

/// Canonical JSON: object keys sorted, no whitespace.
pub fn canonical_json<T: Serialize>(value: &T) -> Result<String, Error> {
    let value = serde_json::to_value(value).map_err(|e| Error::Parse(e.to_string()))?;
    serde_json::to_string(&value).map_err(|e| Error::Parse(e.to_string()))
}

/// `sha256:<hex>` of a byte string.
pub fn sha256_prefixed(bytes: &[u8]) -> String {
    format!("sha256:{}", hex::encode(Sha256::digest(bytes)))
}

/// Hash of a suite manifest: TOML parsed, re-serialized as canonical JSON, hashed.
pub fn suite_hash(toml_text: &str) -> Result<String, Error> {
    let value: toml::Value = toml::from_str(toml_text).map_err(|e| Error::Parse(e.to_string()))?;
    Ok(sha256_prefixed(canonical_json(&value)?.as_bytes()))
}

/// Fingerprint of a result document, used to reject exact duplicates.
pub fn result_fingerprint(result: &crate::result::BenchResult) -> Result<String, Error> {
    Ok(sha256_prefixed(canonical_json(result)?.as_bytes()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn suite_hash_ignores_key_order_and_formatting() {
        let a = "id = \"x\"\nversion = 1\n[runs]\ncount = 3\nwarmup = false\n";
        let b = "version = 1\n  id = \"x\" # comment\n[runs]\nwarmup = false\ncount = 3\n";
        let c = "version = 2\nid = \"x\"\n[runs]\nwarmup = false\ncount = 3\n";
        assert_eq!(suite_hash(a).ok(), suite_hash(b).ok());
        assert_ne!(suite_hash(a).ok(), suite_hash(c).ok());
    }
}
