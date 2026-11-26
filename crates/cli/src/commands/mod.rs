//! Small subcommands. `run` lives in its own module.

pub mod run;

use anyhow::Result;
use benchpress_core::Registry;
use benchpress_core::format::{bytes, duration_ms};
use benchpress_core::result::BenchResult;
use benchpress_core::validate::validate_result;

use crate::output::{self, Role, paint};
use crate::{paths, probe, report, runner, submit, toolchain};

/// `benchpress list`.
pub fn list() -> Result<()> {
    for s in Registry::builtin().latest_all() {
        let m = &s.manifest;
        let e = m.requirements.estimated_minutes;
        output::line(format!(
            "{} {:<9} {:<16} {}",
            output::plate(m.tier),
            m.display_name,
            paint(Role::Muted, &m.key()),
            paint(
                Role::Muted,
                &format!(
                    "{}–{} min · {} GB disk · Rust {}",
                    e.min, e.max, m.requirements.disk_gb, m.toolchain.channel
                )
            )
        ));
        output::line(format!("  {}", m.description));
    }
    Ok(())
}

fn load(path: &std::path::Path) -> Result<BenchResult> {
    Ok(serde_json::from_str(&std::fs::read_to_string(path)?)?)
}

/// `benchpress results`.
pub fn results_list() -> Result<()> {
    let all = submit::list_local()?;
    if all.is_empty() {
        output::line("No results yet. Run `benchpress run quick` to make one.");
        return Ok(());
    }
    for (id, path) in all {
        let Ok(r) = load(&path) else { continue };
        let headline = r
            .scenario("clean-release")
            .or(r.scenarios.first())
            .map(|s| duration_ms(s.summary.median_wall_ms))
            .unwrap_or_default();
        let published = submit::load_submission(&path)
            .map(|s| paint(Role::You, &s.url))
            .unwrap_or_default();
        output::line(format!(
            "{id:<34} {:>9}  {}  {published}",
            headline,
            paint(Role::Muted, &r.machine_line())
        ));
    }
    Ok(())
}

/// `benchpress results show <id>`.
pub fn results_show(id: &str, json: bool) -> Result<()> {
    let path = submit::resolve(id)?;
    let r = load(&path)?;
    if json {
        output::data(&serde_json::to_string_pretty(&r)?);
        return Ok(());
    }
    let registry = Registry::builtin();
    let flags = validate_result(&r, &registry);
    output::line(paint(
        Role::Strong,
        &format!("{}@{} · {}", r.suite.id, r.suite.version, r.started_at),
    ));
    output::line(paint(Role::Muted, &r.machine_line()));
    report::print(&r, flags.as_deref().unwrap_or_default());
    if let Err(rej) = flags {
        output::line(paint(
            Role::Muted,
            &format!("Not publishable: {}", rej.message),
        ));
    }
    if let Some(s) = submit::load_submission(&path) {
        output::field("Published", s.url);
    }
    Ok(())
}

/// `benchpress doctor`.
pub fn doctor() -> Result<()> {
    let p = probe::current();
    let root = paths::work_root();
    let hw = p.hardware(&root);
    let env = p.environment();
    let ok = |s: &str| paint(Role::Success, s);
    let bad = |s: &str| paint(Role::Warning, s);
    output::line(paint(Role::Strong, "Machine"));
    output::field(
        "CPU",
        format!("{} ({} logical cores)", hw.cpu.brand, hw.cpu.logical_cores),
    );
    if let (Some(pc), Some(ec)) = (hw.cpu.performance_cores, hw.cpu.efficiency_cores) {
        output::field("Cores", format!("{pc} performance, {ec} efficiency"));
    }
    output::field(
        "Memory",
        format!(
            "{} GB",
            benchpress_core::hardware::memory_bucket(hw.memory.total_bytes)
        ),
    );
    output::field(
        "Machine",
        [hw.machine.vendor.clone(), hw.machine.family.clone()]
            .into_iter()
            .flatten()
            .collect::<Vec<_>>()
            .join(" "),
    );
    output::field(
        "Storage",
        format!(
            "{} {}",
            hw.storage.kind.clone().unwrap_or_else(|| "unknown".into()),
            hw.storage.filesystem.clone().unwrap_or_default()
        ),
    );
    output::field(
        "OS",
        format!(
            "{} {}",
            env.os.name,
            env.os.version.clone().unwrap_or_default()
        ),
    );
    output::blank();
    output::line(paint(Role::Strong, "Checks"));
    match runner::check_c_toolchain() {
        Ok(()) => output::field("C compiler", ok("found")),
        Err(e) => output::field("C compiler", bad(&e.to_string())),
    }
    let power = match env.power.source {
        Some(benchpress_core::result::PowerSource::Ac) => ok("AC power"),
        Some(benchpress_core::result::PowerSource::Battery) => {
            bad("on battery: results will be flagged")
        }
        None => paint(Role::Muted, "unknown"),
    };
    output::field("Power", power);
    if env.power.low_power_mode == Some(true) {
        output::field("Low power", bad("on: results will be flagged"));
    }
    if let Some(v) = &env.virtualization {
        output::field("Virtualized", bad(&format!("{v}: results will be flagged")));
    }
    let configs = paths::cargo_configs_above(&root);
    if configs.is_empty() {
        output::field(
            "Cargo config",
            ok(&format!("none above {}", root.display())),
        );
    } else {
        output::field(
            "Cargo config",
            bad(&format!(
                "found {}; pass --workdir elsewhere",
                configs[0].display()
            )),
        );
    }
    if let Some(free) = paths::free_bytes(&root) {
        output::field(
            "Free disk",
            format!("{} near {}", bytes(free), root.display()),
        );
    }
    if hw.storage.kind.as_deref() == Some("ram") {
        output::field(
            "Workdir disk",
            bad("RAM-backed (tmpfs); pass --workdir on a real disk"),
        );
    }
    if !env.overrides_cleared.is_empty() {
        output::field(
            "Shell vars",
            format!("{} (ignored during runs)", env.overrides_cleared.join(", ")),
        );
    }
    if let Some(linux) = &env.linux
        && let Some(g) = &linux.governor
    {
        output::field("Governor", g.clone());
    }
    if cfg!(target_os = "macos") {
        output::blank();
        output::hint(
            "Add your terminal app under System Settings → Privacy & Security → Developer Tools. macOS otherwise scans each new build script and proc-macro, which slows Rust builds.",
        );
    }
    output::hint(
        "For steady numbers: plug in, close heavy apps, and leave the machine idle while it runs.",
    );
    Ok(())
}

/// `benchpress clean`.
pub fn clean() -> Result<()> {
    toolchain::remove_all()?;
    for dir in [
        paths::sources_dir()?,
        paths::build_cargo_home()?,
        paths::work_root(),
    ] {
        if dir.exists() {
            std::fs::remove_dir_all(&dir)?;
            output::line(format!("Removed {}", dir.display()));
        }
    }
    output::line("Cached toolchains, sources and build directories removed. Results were kept.");
    Ok(())
}
