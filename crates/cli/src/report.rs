//! Final report: a table plus hints drawn only from measured facts.

use benchpress_core::format::{bytes, duration_ms, percent};
use benchpress_core::result::{BenchResult, Thermal};
use benchpress_core::suite::scenario_label;
use benchpress_core::validate::Flag;
use benchpress_core::{RegisteredSuite, hardware};
use comfy_table::presets::NOTHING;
use comfy_table::{CellAlignment, Table};

use crate::output::{self, Role, paint};

/// Header with suite and machine summary.
pub fn header(suite: &RegisteredSuite, machine: &str) {
    let m = &suite.manifest;
    output::blank();
    output::line(format!(
        "{} {} {}",
        output::plate(m.tier),
        paint(Role::Strong, &format!("{} suite", m.display_name)),
        paint(Role::Muted, &format!("{}@{}", m.id, m.version))
    ));
    output::line(paint(Role::Muted, &m.description));
    output::line(paint(Role::Muted, machine));
    output::blank();
}

/// Print the results table and hints.
pub fn print(result: &BenchResult, flags: &[Flag]) {
    let mut table = Table::new();
    table.load_style(NOTHING);
    table.set_header(
        ["Scenario", "Median", "Range", "CPU", "Peak memory"].map(|h| paint(Role::Muted, h)),
    );
    for s in &result.scenarios {
        table.add_row([
            scenario_label(&s.id),
            paint(Role::Strong, &duration_ms(s.summary.median_wall_ms)),
            paint(
                Role::Muted,
                &format!(
                    "{} to {}",
                    duration_ms(s.summary.min_wall_ms),
                    duration_ms(s.summary.max_wall_ms)
                ),
            ),
            s.cpu_util().map(percent).unwrap_or_default(),
            format!("+{}", bytes(s.peak_mem_delta())),
        ]);
    }
    for i in 1..5 {
        if let Some(c) = table.column_mut(i) {
            c.set_cell_alignment(CellAlignment::Right);
        }
    }
    output::blank();
    output::line(table.to_string());
    output::blank();
    for h in hints(result, flags) {
        output::hint(h);
    }
}

/// Plain-language observations backed by the measurements.
pub fn hints(result: &BenchResult, flags: &[Flag]) -> Vec<String> {
    let mut out = Vec::new();
    let total = result.hardware.memory.total_bytes;
    if let Some(s) = result.scenarios.iter().max_by_key(|s| s.peak_swap_delta())
        && s.peak_swap_delta() > 256 * 1024 * 1024
    {
        out.push(format!(
            "Swap was used during {} ({} peak): more RAM would likely speed up this build.",
            scenario_label(&s.id).to_lowercase(),
            bytes(s.peak_swap_delta())
        ));
    }
    let psi = result
        .scenarios
        .iter()
        .filter_map(|s| {
            s.runs
                .iter()
                .filter_map(|r| r.mem_pressure.some_avg10_max)
                .reduce(f64::max)
                .map(|p| (s, p))
        })
        .max_by(|a, b| a.1.total_cmp(&b.1));
    if let Some((s, p)) = psi
        && p >= 10.0
    {
        out.push(format!(
            "Processes waited on memory {p:.0}% of the time during {}: the build was memory-bound at times.",
            scenario_label(&s.id).to_lowercase()
        ));
    }
    if let Some(s) = result.scenarios.iter().max_by_key(|s| {
        s.runs
            .iter()
            .map(|r| r.peak_mem_used_bytes)
            .max()
            .unwrap_or(0)
    }) {
        let peak = s
            .runs
            .iter()
            .map(|r| r.peak_mem_used_bytes)
            .max()
            .unwrap_or(0);
        if total > 0 && peak as f64 > total as f64 * 0.9 {
            out.push(format!(
                "Memory in use peaked at {} of {} GB during {}.",
                percent(peak as f64 / total as f64),
                hardware::memory_bucket(total),
                scenario_label(&s.id).to_lowercase()
            ));
        }
    }
    if let Some(util) = result
        .scenario("clean-release")
        .and_then(benchpress_core::result::ScenarioResult::cpu_util)
    {
        if util >= 0.85 {
            out.push(format!("Average CPU utilization {} during clean release: this build is CPU-bound; more or faster cores help.", percent(util)));
        } else if util < 0.5 {
            out.push(format!(
                "Average CPU utilization {} during clean release: much of the build runs on few cores (large crates, linking), so faster cores help more than more cores.",
                percent(util)
            ));
        }
    }
    let hot = result
        .scenarios
        .iter()
        .flat_map(|s| &s.runs)
        .filter_map(|r| r.thermal_end)
        .max();
    if hot.is_some_and(|t| t >= Thermal::Serious) {
        out.push("The OS reported serious thermal pressure during the runs: the CPU was likely throttling.".into());
    }
    for f in flags {
        out.push(match f {
            Flag::Battery => "Running on battery: results flagged and excluded from comparisons.".into(),
            Flag::LowPower => "Low power mode was on: results flagged and excluded from comparisons.".into(),
            Flag::Virtualized => "Running in a virtual machine or container: results flagged and excluded from comparisons.".into(),
            Flag::HighVariance => "Run times varied by more than 10%: close other apps and keep the machine idle for steadier numbers.".into(),
        });
    }
    out
}
