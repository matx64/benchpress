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

/// Set by the `run.sh` launcher: the binary is deleted when it exits.
pub fn ephemeral() -> bool {
    std::env::var_os("BENCHPRESS_EPHEMERAL").is_some_and(|v| v == "1")
}

/// A path for display, with the home directory shortened to `~`.
pub fn display(path: &Path) -> String {
    if let Some(home) = std::env::var_os("HOME").map(PathBuf::from)
        && let Ok(rest) = path.strip_prefix(&home)
    {
        return Path::new("~").join(rest).display().to_string();
    }
    path.display().to_string()
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

/// Everything under the cache directory that Benchpress creates.
pub fn cache_entries() -> Result<Vec<PathBuf>> {
    let root = cache_dir()?;
    Ok([
        "rustup",
        "rustup-cargo",
        "cargo-home",
        "sources",
        "rustup-init",
    ]
    .iter()
    .map(|n| root.join(n))
    .collect())
}

/// Lock held by every run (shared) and by removal of downloads (exclusive, non-blocking).
/// `None` means another process holds a conflicting lock.
pub fn lock_runs(exclusive: bool) -> Result<Option<nix::fcntl::Flock<std::fs::File>>> {
    use nix::fcntl::{Flock, FlockArg};
    let dir = cache_dir()?;
    std::fs::create_dir_all(&dir).with_context(|| format!("creating {}", dir.display()))?;
    let file = std::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(dir.join("run.lock"))?;
    let arg = if exclusive {
        FlockArg::LockExclusiveNonblock
    } else {
        FlockArg::LockShared
    };
    match Flock::lock(file, arg) {
        Ok(lock) => Ok(Some(lock)),
        Err((_, nix::errno::Errno::EWOULDBLOCK)) => Ok(None),
        Err((_, e)) => Err(e).context("locking the Benchpress cache"),
    }
}

/// Total size of the files under `path`, not following symlinks.
pub fn size_of(path: &Path) -> u64 {
    let Ok(meta) = std::fs::symlink_metadata(path) else {
        return 0;
    };
    if !meta.is_dir() {
        return meta.len();
    }
    std::fs::read_dir(path).map_or(0, |rd| {
        rd.filter_map(Result::ok).map(|e| size_of(&e.path())).sum()
    })
}

/// Remove a file or directory if it exists. Returns whether anything was removed.
pub fn remove(path: &Path) -> Result<bool> {
    match std::fs::symlink_metadata(path) {
        Err(_) => Ok(false),
        Ok(m) if m.is_dir() => std::fs::remove_dir_all(path)
            .map(|()| true)
            .with_context(|| format!("removing {}", path.display())),
        Ok(_) => std::fs::remove_file(path)
            .map(|()| true)
            .with_context(|| format!("removing {}", path.display())),
    }
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
