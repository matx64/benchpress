//! Pinned Rust toolchains installed into Benchpress's own rustup home.
//! The user's `~/.rustup` and `~/.cargo` are never touched.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use anyhow::{Context, Result, bail};
use benchpress_core::suite::Toolchain;

use crate::{fetch, paths};

/// An installed toolchain.
#[derive(Debug, Clone)]
pub struct Installed {
    /// Exact channel, e.g. `1.99.1`.
    pub channel: String,
    /// Rustup home containing it.
    pub rustup_home: PathBuf,
    /// Directory with `cargo` and `rustc`.
    pub bin_dir: PathBuf,
}

/// Host triple used for rustup and the toolchain.
pub fn host_triple() -> Result<&'static str> {
    Ok(match (std::env::consts::OS, std::env::consts::ARCH) {
        ("macos", "aarch64") => "aarch64-apple-darwin",
        ("linux", "x86_64") if is_musl_host() => "x86_64-unknown-linux-musl",
        ("linux", "aarch64") if is_musl_host() => "aarch64-unknown-linux-musl",
        ("linux", "x86_64") => "x86_64-unknown-linux-gnu",
        ("linux", "aarch64") => "aarch64-unknown-linux-gnu",
        (os, arch) => bail!(
            "{os} on {arch} is not supported. Benchpress runs on Linux (x86_64, aarch64) and Apple Silicon Macs."
        ),
    })
}

fn is_musl_host() -> bool {
    std::fs::read_dir("/lib").is_ok_and(|rd| {
        rd.filter_map(Result::ok)
            .any(|e| e.file_name().to_string_lossy().starts_with("ld-musl-"))
    })
}

/// Install `toolchain` if missing. `status` receives short progress messages.
pub fn ensure(toolchain: &Toolchain, mut status: impl FnMut(&str)) -> Result<Installed> {
    let host = host_triple()?;
    let rustup_home = paths::rustup_home()?;
    let rustup_cargo = paths::rustup_cargo_home()?;
    let bin_dir = rustup_home
        .join("toolchains")
        .join(format!("{}-{host}", toolchain.channel))
        .join("bin");
    let installed = Installed {
        channel: toolchain.channel.clone(),
        rustup_home: rustup_home.clone(),
        bin_dir: bin_dir.clone(),
    };
    if bin_dir.join("cargo").is_file() && bin_dir.join("rustc").is_file() {
        return Ok(installed);
    }
    let rustup = rustup_cargo.join("bin/rustup");
    if rustup.is_file() {
        status(&format!("installing Rust {}", toolchain.channel));
        run_rustup(
            Command::new(&rustup).args([
                "toolchain",
                "install",
                &toolchain.channel,
                "--profile",
                &toolchain.profile,
                "--no-self-update",
            ]),
            &rustup_home,
            &rustup_cargo,
        )?;
    } else {
        status("downloading rustup-init");
        let base = format!("https://static.rust-lang.org/rustup/dist/{host}/rustup-init");
        let sums = fetch::get_text(&format!("{base}.sha256"))?;
        let sha = sums
            .split_whitespace()
            .next()
            .context("empty rustup-init.sha256")?
            .to_owned();
        let init = paths::cache_dir()?.join("rustup-init");
        fetch::download_verified(&base, &init, &sha, |_, _| {})?;
        set_executable(&init)?;
        status(&format!("installing Rust {}", toolchain.channel));
        run_rustup(
            Command::new(&init).args([
                "-y",
                "--no-modify-path",
                "--no-update-default-toolchain",
                "--profile",
                &toolchain.profile,
                "--default-toolchain",
                &toolchain.channel,
            ]),
            &rustup_home,
            &rustup_cargo,
        )?;
    }
    if !bin_dir.join("cargo").is_file() {
        bail!("rustup finished but {} has no cargo", bin_dir.display());
    }
    Ok(installed)
}

fn run_rustup(cmd: &mut Command, rustup_home: &Path, cargo_home: &Path) -> Result<()> {
    std::fs::create_dir_all(rustup_home)?;
    std::fs::create_dir_all(cargo_home)?;
    let out = cmd
        .env_clear()
        .env("PATH", "/usr/bin:/bin:/usr/sbin:/sbin")
        .env("HOME", std::env::var_os("HOME").unwrap_or_default())
        .env("RUSTUP_HOME", rustup_home)
        .env("CARGO_HOME", cargo_home)
        .env("RUSTUP_INIT_SKIP_PATH_CHECK", "yes")
        .stdin(Stdio::null())
        .output()
        .context("running rustup")?;
    if !out.status.success() {
        bail!("rustup failed:\n{}", String::from_utf8_lossy(&out.stderr));
    }
    Ok(())
}

fn set_executable(path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755))?;
    Ok(())
}

/// Remove every cached toolchain.
pub fn remove_all() -> Result<()> {
    for dir in [paths::rustup_home()?, paths::rustup_cargo_home()?] {
        if dir.exists() {
            std::fs::remove_dir_all(&dir).with_context(|| format!("removing {}", dir.display()))?;
        }
    }
    Ok(())
}
