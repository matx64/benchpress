//! The isolated environment every benchmark command runs in.

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Paths that define one isolated build.
#[derive(Debug, Clone)]
pub struct Isolation {
    /// Toolchain `bin` directory.
    pub toolchain_bin: PathBuf,
    /// Exact toolchain channel.
    pub channel: String,
    /// Benchpress rustup home.
    pub rustup_home: PathBuf,
    /// Cargo home owned by Benchpress.
    pub cargo_home: PathBuf,
    /// Target directory inside the workdir.
    pub target_dir: PathBuf,
}

const SYSTEM_PATH: &str = "/usr/bin:/bin:/usr/sbin:/sbin";

impl Isolation {
    /// `PATH` with the toolchain first and only system directories after it.
    pub fn path_var(&self) -> OsString {
        let mut p = OsString::from(self.toolchain_bin.as_os_str());
        p.push(":");
        p.push(SYSTEM_PATH);
        p
    }

    /// A command for `program` that starts from an empty environment plus the allowlist.
    pub fn command(&self, program: impl AsRef<std::ffi::OsStr>, cwd: &Path) -> Command {
        let mut cmd = Command::new(program);
        cmd.current_dir(cwd).env_clear();
        for (k, v) in self.vars() {
            cmd.env(k, v);
        }
        cmd
    }

    /// `cargo` from the pinned toolchain.
    pub fn cargo(&self, cwd: &Path) -> Command {
        self.command(self.toolchain_bin.join("cargo"), cwd)
    }

    /// Every variable passed to benchmark commands.
    pub fn vars(&self) -> Vec<(&'static str, OsString)> {
        let passthrough =
            |k: &str, fallback: &str| std::env::var_os(k).unwrap_or_else(|| fallback.into());
        vec![
            ("PATH", self.path_var()),
            ("HOME", passthrough("HOME", "/")),
            ("TMPDIR", passthrough("TMPDIR", "/tmp")),
            ("LANG", "C".into()),
            ("TERM", passthrough("TERM", "dumb")),
            ("CARGO_HOME", self.cargo_home.clone().into_os_string()),
            ("RUSTUP_HOME", self.rustup_home.clone().into_os_string()),
            ("RUSTUP_TOOLCHAIN", self.channel.clone().into()),
            ("CARGO_TARGET_DIR", self.target_dir.clone().into_os_string()),
            ("CARGO_TERM_COLOR", "never".into()),
            ("CARGO_TERM_PROGRESS_WHEN", "never".into()),
        ]
    }
}
