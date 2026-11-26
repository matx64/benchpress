//! `benchpress run` and the interactive picker.

use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{Context, Result};
use benchpress_core::validate::validate_result;
use benchpress_core::{RegisteredSuite, Registry, Tier};

use crate::output::{self, Role, paint};
use crate::runner::{self, RunOptions};
use crate::{paths, probe, report, submit};

/// `benchpress run` arguments.
#[derive(clap::Args, Debug, Clone)]
pub struct Args {
    /// quick, standard, heavy, or a suite id
    #[arg(default_value = "standard")]
    pub suite: String,
    /// Measured runs per scenario (the suite's default is required for publishing)
    #[arg(long)]
    pub runs: Option<u32>,
    /// Seconds to rest between runs
    #[arg(long, default_value_t = 10)]
    pub cooldown: u32,
    /// Build directory (default: outside your home, on disk)
    #[arg(long, value_parser = crate::parse_path)]
    pub workdir: Option<PathBuf>,
    /// Keep the build directory afterwards
    #[arg(long)]
    pub keep: bool,
    /// Don't ask before starting on battery or low power
    #[arg(long, short)]
    pub yes: bool,
    /// Don't offer to publish the result
    #[arg(long)]
    pub no_submit: bool,
    /// Print only the result JSON to stdout
    #[arg(long)]
    pub json: bool,
    /// Run a suite manifest from a file instead of a builtin one
    #[arg(long, hide = true, value_parser = crate::parse_path)]
    pub suite_file: Option<PathBuf>,
    /// Use this source tree instead of downloading the pinned archive
    #[arg(long, hide = true, value_parser = crate::parse_path)]
    pub source_dir: Option<PathBuf>,
}

fn pick_suite(args: &Args, registry: &Registry) -> Result<RegisteredSuite> {
    if let Some(file) = &args.suite_file {
        let text =
            std::fs::read_to_string(file).with_context(|| format!("reading {}", file.display()))?;
        let name = file
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let mut reg = Registry::default();
        return Ok(reg.insert(&name, &text)?.clone());
    }
    let found = match args.suite.parse::<Tier>() {
        Ok(tier) => registry.by_tier(tier),
        Err(_) => registry.latest(&args.suite),
    };
    found.cloned().with_context(|| {
        format!(
            "no suite `{}`. Run `benchpress list` to see them.",
            args.suite
        )
    })
}

/// Execute `benchpress run`.
pub fn execute(args: &Args) -> Result<()> {
    output::set_json_mode(args.json);
    let registry = Registry::builtin();
    let suite = pick_suite(args, &registry)?;
    if args.runs.is_some_and(|r| r < suite.manifest.runs.count) {
        output::warn(format!(
            "fewer than {} runs per scenario: the result can be saved but not published.",
            suite.manifest.runs.count
        ));
    }
    let probe: Arc<dyn probe::Probe + Send + Sync> = Arc::from(probe::current());
    let stamp = time::OffsetDateTime::now_utc().format(time::macros::format_description!(
        "[year][month][day]-[hour][minute][second]"
    ))?;
    let opts = RunOptions {
        runs: args.runs,
        cooldown: args.cooldown,
        workdir: args.workdir.clone(),
        keep: args.keep,
        yes: args.yes,
        source_dir: args.source_dir.clone(),
    };
    let workdir = runner::preflight(&suite, &opts, probe.as_ref(), &stamp)?;
    let hw = probe.hardware(&workdir);
    let machine = format!(
        "{} · {} cores · {} GB",
        hw.machine
            .family
            .clone()
            .unwrap_or_else(|| benchpress_core::hardware::cpu_family(&hw.cpu.brand)),
        hw.cpu.logical_cores,
        benchpress_core::hardware::memory_bucket(hw.memory.total_bytes)
    );
    let machine = if hw.machine.family.is_some() {
        format!(
            "{machine} · {}",
            benchpress_core::hardware::cpu_family(&hw.cpu.brand)
        )
    } else {
        machine
    };
    report::header(&suite, &machine);
    let est = suite.manifest.requirements.estimated_minutes;
    output::field(
        "Estimate",
        format!(
            "{} to {} minutes, {} GB disk",
            est.min, est.max, suite.manifest.requirements.disk_gb
        ),
    );
    output::field("Workdir", workdir.display().to_string());
    if cfg!(target_os = "macos") {
        output::hint(
            "Tip: add your terminal under System Settings → Privacy & Security → Developer Tools so macOS doesn't scan every build script.",
        );
    }
    output::blank();

    let outcome = match runner::run(&suite, &opts, &probe, &workdir, &stamp) {
        Ok(o) => o,
        Err(e) => {
            runner::report_failure(&e, &workdir);
            return Err(e);
        }
    };
    let flags = if args.suite_file.is_some() {
        Ok(Vec::new())
    } else {
        validate_result(&outcome.result, &registry)
    };
    report::print(&outcome.result, flags.as_deref().unwrap_or_default());
    output::field("Saved", paths::display(&outcome.path));
    output::field("Id", outcome.local_id.clone());
    if args.json {
        output::data(&serde_json::to_string_pretty(&outcome.result)?);
        return Ok(());
    }
    match flags {
        Err(r) => output::line(paint(
            Role::Muted,
            &format!("Not publishable: {}", r.message),
        )),
        Ok(_) if args.no_submit || args.suite_file.is_some() || !output::is_interactive() => {
            output::line(paint(
                Role::Muted,
                &format!(
                    "Publish it later {}.",
                    output::later(&format!("submit {}", outcome.local_id), "Your results")
                ),
            ));
        }
        Ok(_) => {
            output::blank();
            let host = submit::server_url()
                .trim_start_matches("https://")
                .to_owned();
            if output::confirm(&format!("Publish anonymously to {host}?"), false) {
                submit::submit(&outcome.path, false)?;
            } else {
                output::line(paint(
                    Role::Muted,
                    &format!(
                        "Publish it later {}.",
                        output::later(&format!("submit {}", outcome.local_id), "Your results")
                    ),
                ));
            }
        }
    }
    super::data::note_downloads();
    Ok(())
}

/// Pick a suite, confirm time and disk, run. Backing out returns without running.
pub fn pick_and_run() -> Result<()> {
    let registry = Registry::builtin();
    let suites = registry.latest_all();
    let items: Vec<String> = suites
        .iter()
        .map(|s| {
            let m = &s.manifest;
            let e = m.requirements.estimated_minutes;
            format!(
                "{} {:<9} {}  {}",
                output::plate(m.tier),
                m.display_name,
                paint(
                    Role::Muted,
                    &format!("{}–{} min, {} GB", e.min, e.max, m.requirements.disk_gb)
                ),
                paint(Role::Muted, m.project())
            )
        })
        .collect();
    let default = suites
        .iter()
        .position(|s| s.manifest.tier == Tier::Quick)
        .unwrap_or(0);
    let Some(choice) = output::choose("Pick a suite", &items, default)? else {
        return Ok(());
    };
    let s = suites[choice];
    let e = s.manifest.requirements.estimated_minutes;
    if !output::confirm(
        &format!(
            "Run {} now? It takes about {} to {} minutes and {} GB of disk.",
            s.manifest.display_name, e.min, e.max, s.manifest.requirements.disk_gb
        ),
        true,
    ) {
        return Ok(());
    }
    execute(&Args {
        suite: s.manifest.id.clone(),
        runs: None,
        cooldown: 10,
        workdir: None,
        keep: false,
        yes: false,
        no_submit: false,
        json: false,
        suite_file: None,
        source_dir: None,
    })
}
