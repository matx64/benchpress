//! The run pipeline: preflight, workdir, toolchain, source, dependencies, scenarios, teardown.

pub mod env;
pub mod measure;

use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result, bail};
use benchpress_core::RegisteredSuite;
use benchpress_core::result::{
    BenchResult, Client, RESULT_SCHEMA_VERSION, ScenarioResult, Settings, SuiteRef, Summary,
};
use benchpress_core::suite::ScenarioKind;

use crate::output::{self, Role, paint};
use crate::probe::Probe;
use crate::{fetch, interrupt, paths, progress, toolchain};
use env::Isolation;

/// Options for one run.
#[derive(Debug, Clone)]
pub struct RunOptions {
    /// Measured runs per scenario (defaults to the manifest).
    pub runs: Option<u32>,
    /// Seconds between runs.
    pub cooldown: u32,
    /// Build tree location.
    pub workdir: Option<PathBuf>,
    /// Keep the build tree afterwards.
    pub keep: bool,
    /// Skip confirmations.
    pub yes: bool,
    /// Use an already-extracted source tree instead of downloading (tests and suite authors).
    pub source_dir: Option<PathBuf>,
}

/// A finished run.
pub struct Outcome {
    /// The result document.
    pub result: BenchResult,
    /// Local id, e.g. `20261126-140300-rust-quick`.
    pub local_id: String,
    /// Where the result JSON was written.
    pub path: PathBuf,
}

/// Restores a touched file when dropped, including on Ctrl+C and errors.
struct TouchGuard {
    path: PathBuf,
    original: Vec<u8>,
}

impl TouchGuard {
    fn new(path: PathBuf) -> Result<Self> {
        let original =
            std::fs::read(&path).with_context(|| format!("reading {}", path.display()))?;
        Ok(TouchGuard { path, original })
    }

    fn bump(&self, n: u32) -> Result<()> {
        let mut f = std::fs::OpenOptions::new().append(true).open(&self.path)?;
        writeln!(f, "// benchpress {n}")?;
        Ok(())
    }
}

impl Drop for TouchGuard {
    fn drop(&mut self) {
        if let Err(e) = std::fs::write(&self.path, &self.original) {
            output::warn(format!("could not restore {}: {e}", self.path.display()));
        }
    }
}

/// Removes the workdir when dropped unless disarmed.
struct WorkdirGuard {
    path: PathBuf,
    remove: bool,
}

impl Drop for WorkdirGuard {
    fn drop(&mut self) {
        if self.remove && self.path.exists() {
            let _ = std::fs::remove_dir_all(&self.path);
        }
    }
}

/// Check everything that can fail before spending time. Returns the workdir.
pub fn preflight(
    suite: &RegisteredSuite,
    opts: &RunOptions,
    probe: &dyn Probe,
    stamp: &str,
) -> Result<PathBuf> {
    let m = &suite.manifest;
    let workdir = opts
        .workdir
        .clone()
        .unwrap_or_else(|| paths::default_workdir(&m.id, stamp));
    let configs = paths::cargo_configs_above(&workdir);
    if !configs.is_empty() {
        let list: Vec<String> = configs.iter().map(|p| p.display().to_string()).collect();
        bail!(
            "cargo would apply config from {} to the benchmark build. Pass --workdir with a directory outside that tree (for example under /var/tmp).",
            list.join(", ")
        );
    }
    let home = std::env::var_os("HOME").map(PathBuf::from);
    if let Some(home) = home.filter(|h| workdir.starts_with(h)) {
        output::warn(format!(
            "the workdir is inside {}. Benchpress builds outside your home directory by default to avoid user cargo config.",
            home.display()
        ));
    }
    if cfg!(target_os = "linux") && workdir.starts_with("/tmp") {
        output::warn(
            "/tmp is often RAM-backed (tmpfs), which makes builds unrepresentative of a real disk.",
        );
    }
    let need = u64::from(m.requirements.disk_gb) * 1_000_000_000;
    if let Some(free) = paths::free_bytes(&workdir)
        && free < need
    {
        bail!(
            "{} needs {} GB free near {}, but only {} is available. Free some space or pass --workdir on another disk.",
            m.display_name,
            m.requirements.disk_gb,
            workdir.display(),
            benchpress_core::format::bytes(free)
        );
    }
    check_c_toolchain()?;
    let power = probe.power();
    let on_battery = power.source == Some(benchpress_core::result::PowerSource::Battery);
    let low_power = power.low_power_mode == Some(true);
    if on_battery || low_power {
        let what = if on_battery {
            "on battery power"
        } else {
            "in low power mode"
        };
        output::warn(format!(
            "this machine is running {what}. Results will be flagged and excluded from comparisons."
        ));
        if !opts.yes && !output::confirm("Continue anyway?", false) {
            bail!("stopped before starting. Plug in or turn off low power mode, then run again.");
        }
    }
    Ok(workdir)
}

/// Fail early with an actionable message when there is no C compiler or linker.
pub fn check_c_toolchain() -> Result<()> {
    let found = std::process::Command::new("cc")
        .arg("--version")
        .env("PATH", "/usr/bin:/bin:/usr/sbin:/sbin")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .is_ok_and(|s| s.success());
    if !found {
        let fix = if cfg!(target_os = "macos") {
            "Install the Command Line Tools with `xcode-select --install`."
        } else {
            "Install a C compiler and linker, for example `sudo apt install build-essential` or `sudo dnf install gcc`."
        };
        bail!("no C compiler (`cc`) found. Rust needs it to link. {fix}");
    }
    Ok(())
}

/// Run the whole pipeline.
pub fn run(
    suite: &RegisteredSuite,
    opts: &RunOptions,
    probe: &Arc<dyn Probe + Send + Sync>,
    workdir: &Path,
    stamp: &str,
) -> Result<Outcome> {
    let _active = interrupt::RunActive::new();
    let _lock = paths::lock_runs(false)?;
    let m = &suite.manifest;
    let started_at = time::OffsetDateTime::now_utc()
        .replace_nanosecond(0)
        .unwrap_or_else(|_| time::OffsetDateTime::now_utc())
        .format(&time::format_description::well_known::Rfc3339)?;
    let runs = opts.runs.unwrap_or(m.runs.count).max(1);

    std::fs::create_dir_all(workdir).with_context(|| format!("creating {}", workdir.display()))?;
    let mut guard = WorkdirGuard {
        path: workdir.to_path_buf(),
        remove: !opts.keep,
    };
    let spotlight_excluded = cfg!(target_os = "macos")
        && std::fs::write(workdir.join(".metadata_never_index"), b"").is_ok();

    let hardware = probe.hardware(workdir);
    let mut environment = probe.environment();
    if let Some(mac) = environment.mac.as_mut() {
        mac.spotlight_excluded = spotlight_excluded;
    }

    let result = (|| -> Result<Vec<(ScenarioResult, Vec<Vec<crate::sampler::Sample>>)>> {
        let tc = progress::phase(
            "Toolchain",
            &format!("Rust {}", m.toolchain.channel),
            |set| toolchain::ensure(&m.toolchain, |s| set(s)),
        )?;

        let src_root = workdir.join("src");
        progress::phase(
            "Source",
            &format!("{} @ {}", m.project(), &m.source.commit[..7]),
            |set| prepare_source(suite, opts.source_dir.as_deref(), &src_root, set),
        )?;
        let project = if m.source.subdir.is_empty() {
            src_root.clone()
        } else {
            src_root.join(&m.source.subdir)
        };

        let iso = Isolation {
            toolchain_bin: tc.bin_dir.clone(),
            channel: tc.channel.clone(),
            rustup_home: tc.rustup_home.clone(),
            cargo_home: paths::build_cargo_home()?,
            target_dir: workdir.join("target"),
        };
        std::fs::create_dir_all(&iso.cargo_home)?;
        for name in ["config.toml", "config"] {
            let p = iso.cargo_home.join(name);
            if p.exists() {
                std::fs::remove_file(&p)?;
            }
        }
        progress::phase("Dependencies", "cargo fetch", |_| {
            let mut cmd = iso.cargo(&project);
            cmd.args(["fetch", "--locked"]);
            measure::run_untimed(cmd)
        })?;

        let mut out = Vec::new();
        let mut prepared_args: Option<Vec<String>> = None;
        let logical = hardware.cpu.logical_cores;
        let build_args = m.build.cargo_args();
        for scenario in &m.scenarios {
            let mut args = scenario.args.clone();
            args.extend(build_args.iter().cloned());
            let cargo_cmd = || {
                let mut c = iso.cargo(&project);
                c.args(&args);
                c
            };
            let label = scenario.label();
            let mut runs_out = Vec::new();
            let mut samples_out = Vec::new();
            match scenario.kind {
                ScenarioKind::Clean => {
                    if m.runs.warmup {
                        progress::phase(&label, "warm-up", |_| {
                            remove_dir(&iso.target_dir)?;
                            measure::run_untimed(cargo_cmd())
                        })?;
                    }
                    for i in 1..=runs {
                        remove_dir(&iso.target_dir)?;
                        cooldown(opts.cooldown)?;
                        let (run, samples) = progress::timed(&label, i, runs, || {
                            measure::measure(cargo_cmd(), probe, logical)
                        })?;
                        runs_out.push(run);
                        samples_out.push(samples);
                    }
                    prepared_args = Some(args.clone());
                }
                ScenarioKind::Incremental => {
                    let touch = project.join(scenario.touch.as_deref().unwrap_or_default());
                    if prepared_args.as_ref() != Some(&args) || !iso.target_dir.exists() {
                        progress::phase(&label, "preparing build", |_| {
                            measure::run_untimed(cargo_cmd())
                        })?;
                    }
                    let guard = TouchGuard::new(touch)?;
                    for i in 1..=runs {
                        guard.bump(i)?;
                        cooldown(opts.cooldown)?;
                        let (run, samples) = progress::timed(&label, i, runs, || {
                            measure::measure(cargo_cmd(), probe, logical)
                        })?;
                        runs_out.push(run);
                        samples_out.push(samples);
                    }
                    drop(guard);
                    prepared_args = None;
                }
            }
            let summary = Summary::from_runs(&runs_out).context("no runs recorded")?;
            out.push((
                ScenarioResult {
                    id: scenario.id.clone(),
                    runs: runs_out,
                    summary,
                },
                samples_out,
            ));
        }
        Ok(out)
    })();

    let scenarios = match result {
        Ok(s) => s,
        Err(e) => {
            if e.downcast_ref::<interrupt::Interrupted>().is_none() {
                guard.remove = false;
            }
            return Err(e);
        }
    };

    let result = BenchResult {
        schema_version: RESULT_SCHEMA_VERSION,
        client: Client {
            version: benchpress_core::VERSION.into(),
            target: env!("BENCHPRESS_TARGET").into(),
        },
        suite: SuiteRef {
            id: m.id.clone(),
            version: m.version,
            hash: suite.hash.clone(),
        },
        started_at,
        settings: Settings {
            runs,
            cooldown_secs: opts.cooldown,
        },
        hardware,
        environment,
        scenarios: scenarios.iter().map(|(s, _)| s.clone()).collect(),
        samples_ref: None,
    };

    let local_id = format!("{stamp}-{}", m.id);
    let dir = paths::results_dir()?;
    std::fs::create_dir_all(&dir)?;
    let path = dir.join(format!("{local_id}.json"));
    std::fs::write(&path, serde_json::to_string_pretty(&result)?)?;
    let mut samples = String::new();
    for (scen, runs) in &scenarios {
        for (i, run) in runs.iter().enumerate() {
            for s in run {
                let mut v = serde_json::to_value(s)?;
                v["scenario"] = scen.id.clone().into();
                v["run"] = (i + 1).into();
                samples.push_str(&v.to_string());
                samples.push('\n');
            }
        }
    }
    std::fs::write(dir.join(format!("{local_id}.samples.jsonl")), samples)?;
    if opts.keep {
        output::field("Kept", workdir.display().to_string());
    }
    Ok(Outcome {
        result,
        local_id,
        path,
    })
}

fn prepare_source(
    suite: &RegisteredSuite,
    local: Option<&Path>,
    dest: &Path,
    set: &mut dyn FnMut(&str),
) -> Result<()> {
    remove_dir(dest)?;
    if let Some(local) = local {
        copy_tree(local, dest)?;
        return Ok(());
    }
    let src = &suite.manifest.source;
    let tarball = paths::sources_dir()?.join(format!("{}.tar.gz", src.archive_sha256));
    fetch::download_verified(
        &src.archive_url(),
        &tarball,
        &src.archive_sha256,
        |done, total| {
            let msg = match total {
                Some(t) => format!(
                    "downloading {} of {}",
                    benchpress_core::format::bytes(done),
                    benchpress_core::format::bytes(t)
                ),
                None => format!("downloading {}", benchpress_core::format::bytes(done)),
            };
            set(&msg);
        },
    )?;
    set("extracting");
    extract_tarball(&tarball, dest)
}

/// Extract a GitHub tarball, dropping its top-level directory.
fn extract_tarball(tarball: &Path, dest: &Path) -> Result<()> {
    std::fs::create_dir_all(dest)?;
    let file = std::fs::File::open(tarball)?;
    let mut archive = tar::Archive::new(flate2::read::GzDecoder::new(file));
    for entry in archive.entries()? {
        interrupt::check()?;
        let mut entry = entry?;
        let path = entry.path()?.into_owned();
        let stripped: PathBuf = path.components().skip(1).collect();
        if stripped.as_os_str().is_empty() {
            continue;
        }
        if stripped
            .components()
            .any(|c| !matches!(c, std::path::Component::Normal(_)))
        {
            bail!("refusing unsafe path {} in source archive", path.display());
        }
        entry.unpack(dest.join(stripped))?;
    }
    Ok(())
}

fn copy_tree(from: &Path, to: &Path) -> Result<()> {
    std::fs::create_dir_all(to)?;
    for entry in std::fs::read_dir(from)? {
        let entry = entry?;
        let name = entry.file_name();
        if name == "target" {
            continue;
        }
        let ty = entry.file_type()?;
        if ty.is_dir() {
            copy_tree(&entry.path(), &to.join(&name))?;
        } else if ty.is_file() {
            std::fs::copy(entry.path(), to.join(&name))?;
        }
    }
    Ok(())
}

fn remove_dir(dir: &Path) -> Result<()> {
    if dir.exists() {
        std::fs::remove_dir_all(dir).with_context(|| format!("removing {}", dir.display()))?;
    }
    Ok(())
}

fn cooldown(secs: u32) -> Result<()> {
    if secs > 0 {
        progress::waiting(secs, || {
            interrupt::sleep(Duration::from_secs(u64::from(secs)))
        })?;
    }
    Ok(())
}

/// Print a failure with cargo's last stderr lines.
pub fn report_failure(err: &anyhow::Error, workdir: &Path) {
    if let Some(f) = err.downcast_ref::<measure::CommandFailed>() {
        output::error(format!("{} ({})", f.command, f.status));
        output::line(paint(
            Role::Muted,
            &format!("Last {} lines of stderr:", f.stderr_tail.len()),
        ));
        for l in &f.stderr_tail {
            output::line(format!("  {l}"));
        }
        output::field(
            "Workdir",
            format!("{} (kept for inspection)", workdir.display()),
        );
        output::line("This result was not saved and cannot be published.");
    }
}
