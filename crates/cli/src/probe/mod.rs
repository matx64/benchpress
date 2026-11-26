//! Best-effort hardware and environment probes. Every field falls back to `None`
//! on error; nothing here needs root.

use std::path::Path;
use std::process::{Command, Stdio};

use benchpress_core::result::{Environment, Hardware, Power, Thermal};

#[cfg(target_os = "linux")]
pub mod linux;
#[cfg(target_os = "macos")]
pub mod macos;

/// Platform probe. Windows can be added as another implementation.
pub trait Probe {
    /// Hardware model facts. `workdir` selects the disk to describe.
    fn hardware(&self, workdir: &Path) -> Hardware;
    /// OS and machine state.
    fn environment(&self) -> Environment;
    /// Current power state.
    fn power(&self) -> Power;
    /// Current thermal state, where the OS reports one.
    fn thermal(&self) -> Option<Thermal>;
    /// Linux memory pressure `some avg10`, where available.
    fn memory_pressure(&self) -> Option<f64> {
        None
    }
}

/// The probe for the current OS.
pub fn current() -> Box<dyn Probe + Send + Sync> {
    #[cfg(target_os = "linux")]
    return Box::new(linux::LinuxProbe);
    #[cfg(target_os = "macos")]
    return Box::new(macos::MacProbe);
}

/// Run a tool and return trimmed stdout when it exits successfully.
pub(crate) fn command_output(program: &str, args: &[&str]) -> Option<String> {
    let out = Command::new(program)
        .args(args)
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .map_err(|e| tracing::debug!("{program}: {e}"))
        .ok()?;
    if !out.status.success() {
        tracing::debug!("{program} exited with {}", out.status);
        return None;
    }
    Some(String::from_utf8_lossy(&out.stdout).trim().to_owned())
}

/// Read a small text file, trimmed; `None` when missing or empty.
#[cfg(target_os = "linux")]
pub(crate) fn read_trimmed(path: impl AsRef<Path>) -> Option<String> {
    let s = std::fs::read_to_string(path.as_ref()).ok()?;
    let s = s.trim();
    (!s.is_empty()).then(|| s.to_owned())
}

/// Names of cargo/rustc variables set in the user's shell. Values are never read.
pub fn cargo_overrides() -> Vec<String> {
    let mut names: Vec<String> = std::env::vars_os()
        .filter_map(|(k, _)| k.into_string().ok())
        .filter(|k| {
            k.starts_with("CARGO_")
                || k.starts_with("RUSTC")
                || k.starts_with("RUSTDOC")
                || k.starts_with("RUSTUP_")
                || matches!(
                    k.as_str(),
                    "RUSTFLAGS"
                        | "CC"
                        | "CXX"
                        | "LD"
                        | "CFLAGS"
                        | "CXXFLAGS"
                        | "LDFLAGS"
                        | "SCCACHE_DIR"
                )
        })
        .collect();
    names.sort();
    names
}

/// Architecture name used in results.
pub fn arch() -> String {
    match std::env::consts::ARCH {
        "aarch64" => "aarch64".into(),
        other => other.into(),
    }
}
