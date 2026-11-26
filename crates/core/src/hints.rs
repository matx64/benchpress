//! Plain-language observations drawn only from measured facts. Shared by the CLI report
//! and the result page.

use crate::format::{bytes, percent};
use crate::hardware;
use crate::result::{BenchResult, ScenarioResult, Thermal};
use crate::suite::scenario_label;
use crate::validate::Flag;

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
        .and_then(ScenarioResult::cpu_util)
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
            Flag::Outlier => "Far from other results of this configuration: held back from comparisons until more results agree.".into(),
        });
    }
    out
}
