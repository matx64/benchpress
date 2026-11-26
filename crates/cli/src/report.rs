//! Final report: a table plus hints drawn only from measured facts.

use benchpress_core::RegisteredSuite;
use benchpress_core::format::{bytes, duration_ms, percent};
use benchpress_core::result::BenchResult;
use benchpress_core::suite::scenario_label;
use benchpress_core::validate::Flag;
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
    for h in benchpress_core::hints::hints(result, flags) {
        output::hint(h);
    }
}
