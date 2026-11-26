//! Where Benchpress keeps caches, results and build trees.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

/// `~/.cache/benchpress` or `~/Library/Caches/benchpress`.
pub fn cache_dir() -> Result<PathBuf> {
    if let Some(dir) = std::env::var_os("BENCHPRESS_CACHE_DIR") {
        return Ok(PathBuf::from(dir));
    }
    Ok(dirs::cache_dir()
        .context("no cache directory for this user")?
        .join("benchpress"))
}

/// `~/.local/share/benchpress/results` or `~/Library/Application Support/benchpress/results`.
pub fn results_dir() -> Result<PathBuf> {
    if let Some(dir) = std::env::var_os("BENCHPRESS_DATA_DIR") {
        return Ok(PathBuf::from(dir).join("results"));
    }
    Ok(dirs::data_dir()
        .context("no data directory for this user")?
        .join("benchpress")
        .join("results"))
}

/// Rustup home owned by Benchpress, shared across runs.
pub fn rustup_home() -> Result<PathBuf> {
    Ok(cache_dir()?.join("rustup"))
}

/// Cargo home used only to run `rustup-init`.
pub fn rustup_cargo_home() -> Result<PathBuf> {
    Ok(cache_dir()?.join("rustup-cargo"))
}

/// Cargo home for benchmark builds: crate downloads cached across runs, never user config.
pub fn build_cargo_home() -> Result<PathBuf> {
    Ok(cache_dir()?.join("cargo-home"))
}

/// Cached source tarballs.
pub fn sources_dir() -> Result<PathBuf> {
    Ok(cache_dir()?.join("sources"))
}

/// Parent of all default build trees. Outside `$HOME` and on a real disk.
pub fn work_root() -> PathBuf {
    if cfg!(target_os = "macos") {
        std::env::temp_dir().join("benchpress")
    } else {
        PathBuf::from(format!("/var/tmp/benchpress-{}", nix::unistd::getuid()))
    }
}

/// Default build tree for one run.
pub fn default_workdir(suite_id: &str, stamp: &str) -> PathBuf {
    work_root().join(format!("{suite_id}-{stamp}"))
}

/// Cargo config files on the path from `dir` up to `/`.
pub fn cargo_configs_above(dir: &Path) -> Vec<PathBuf> {
    dir.ancestors()
        .flat_map(|d| [d.join(".cargo/config.toml"), d.join(".cargo/config")])
        .filter(|p| p.is_file())
        .collect()
}

/// Free bytes on the filesystem holding `path` (or its nearest existing ancestor).
pub fn free_bytes(path: &Path) -> Option<u64> {
    let existing = path.ancestors().find(|p| p.exists())?;
    let st = nix::sys::statvfs::statvfs(existing).ok()?;
    #[allow(clippy::useless_conversion)]
    Some(u64::from(st.blocks_available()) * u64::from(st.fragment_size()))
}
