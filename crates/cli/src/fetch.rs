//! HTTPS downloads with checksum verification.

use std::io::{Read, Write};
use std::path::Path;

use anyhow::{Context, Result, bail};
use sha2::{Digest, Sha256};

use crate::interrupt;

/// User agent for every request.
pub fn user_agent() -> String {
    format!("benchpress/{}", benchpress_core::VERSION)
}

/// Shared HTTP agent.
pub fn agent() -> ureq::Agent {
    ureq::Agent::config_builder()
        .user_agent(user_agent())
        .timeout_connect(Some(std::time::Duration::from_secs(20)))
        .http_status_as_error(false)
        .build()
        .into()
}

/// GET a small text resource.
pub fn get_text(url: &str) -> Result<String> {
    let mut resp = agent()
        .get(url)
        .call()
        .with_context(|| format!("requesting {url}"))?;
    if !resp.status().is_success() {
        bail!("{url} returned HTTP {}", resp.status());
    }
    resp.body_mut()
        .read_to_string()
        .with_context(|| format!("reading {url}"))
}

/// Download `url` to `dest`, verifying its SHA-256. Reuses `dest` if it already matches.
/// `progress` receives `(downloaded, total)`.
pub fn download_verified(
    url: &str,
    dest: &Path,
    sha256_hex: &str,
    mut progress: impl FnMut(u64, Option<u64>),
) -> Result<()> {
    if dest.is_file() && file_sha256(dest)? == sha256_hex {
        return Ok(());
    }
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let partial = dest.with_extension("partial");
    let mut resp = agent()
        .get(url)
        .call()
        .with_context(|| format!("downloading {url}"))?;
    if !resp.status().is_success() {
        bail!("{url} returned HTTP {}", resp.status());
    }
    let total = resp.body().content_length();
    let mut reader = resp
        .body_mut()
        .with_config()
        .limit(2 * 1024 * 1024 * 1024)
        .reader();
    let mut file = std::fs::File::create(&partial)?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; 256 * 1024];
    let mut done = 0u64;
    loop {
        interrupt::check()?;
        let n = reader
            .read(&mut buf)
            .with_context(|| format!("downloading {url}"))?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
        file.write_all(&buf[..n])?;
        done += n as u64;
        progress(done, total);
    }
    file.sync_all()?;
    let got = hex::encode(hasher.finalize());
    if got != sha256_hex {
        let _ = std::fs::remove_file(&partial);
        bail!(
            "checksum mismatch for {url}: expected {sha256_hex}, got {got}. The download was discarded."
        );
    }
    std::fs::rename(&partial, dest)?;
    Ok(())
}

/// SHA-256 of a file as lowercase hex.
pub fn file_sha256(path: &Path) -> Result<String> {
    let mut f = std::fs::File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; 256 * 1024];
    loop {
        let n = f.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(hex::encode(hasher.finalize()))
}
