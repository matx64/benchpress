//! In-memory snapshot of visible results and the aggregates computed from it.
//! Rebuilt on every insert or delete; at thousands of rows that takes milliseconds.

use std::collections::BTreeMap;

use benchpress_core::validate::is_outlier;

use crate::db::ResultRow;

/// All visible results, newest first.
#[derive(Debug, Default)]
pub struct Snapshot {
    pub rows: Vec<ResultRow>,
}

/// Filters for configuration listings.
#[derive(Debug, Clone, Default)]
pub struct Filter {
    pub os: Option<String>,
    pub vendor: Option<String>,
    pub mem_gb: Option<u32>,
    pub form_factor: Option<String>,
    pub include_flagged: bool,
}

impl Filter {
    fn matches(&self, r: &ResultRow) -> bool {
        (self.include_flagged || !r.excluded)
            && self.os.as_ref().is_none_or(|o| &r.os_name == o)
            && self
                .vendor
                .as_ref()
                .is_none_or(|v| r.cpu_vendor.as_ref() == Some(v))
            && self.mem_gb.is_none_or(|m| r.mem_gb == m)
            && self
                .form_factor
                .as_ref()
                .is_none_or(|f| r.form_factor.as_ref() == Some(f))
    }
}

/// `(suite_id@version, cpu_family, mem_gb, os_name)`.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ConfigKey {
    pub cpu_family: String,
    pub mem_gb: u32,
    pub os_name: String,
}

impl ConfigKey {
    /// URL token, e.g. `Apple M3 Pro~36~macOS`.
    pub fn token(&self) -> String {
        format!("{}~{}~{}", self.cpu_family, self.mem_gb, self.os_name)
    }

    /// Parse a URL token.
    pub fn parse(s: &str) -> Option<ConfigKey> {
        let mut parts = s.rsplitn(3, '~');
        let os_name = parts.next()?.to_owned();
        let mem_gb = parts.next()?.parse().ok()?;
        let cpu_family = parts.next()?.to_owned();
        Some(ConfigKey {
            cpu_family,
            mem_gb,
            os_name,
        })
    }

    /// `Apple M3 Pro · 36 GB · macOS`.
    pub fn label(&self) -> String {
        format!(
            "{} · {} GB · {}",
            self.cpu_family, self.mem_gb, self.os_name
        )
    }
}

/// Aggregate over one configuration for one scenario.
#[derive(Debug, Clone)]
pub struct ConfigStats {
    pub key: ConfigKey,
    pub n: usize,
    pub median: u64,
    pub p25: u64,
    pub p75: u64,
    pub vendor: Option<String>,
    pub logical_cores: Option<u32>,
    pub families: Vec<String>,
    pub swap_share: f64,
    pub result_ids: Vec<String>,
}

/// Hold back results far from their peers: the other unflagged results of the same suite
/// version and configuration. Marked results are excluded like flagged ones.
pub fn mark_outliers(rows: &mut [ResultRow]) {
    let key = |r: &ResultRow| {
        (
            r.suite_id.clone(),
            r.suite_version,
            r.cpu_family.clone(),
            r.mem_gb,
            r.os_name.clone(),
        )
    };
    let mut groups: BTreeMap<_, Vec<usize>> = BTreeMap::new();
    for (i, r) in rows.iter().enumerate() {
        if !r.excluded {
            groups.entry(key(r)).or_default().push(i);
        }
    }
    let mut marked = Vec::new();
    for members in groups.values() {
        let mut values: BTreeMap<&str, Vec<u64>> = BTreeMap::new();
        for &i in members {
            for sc in &rows[i].scenarios {
                values
                    .entry(sc.scenario_id.as_str())
                    .or_default()
                    .push(sc.median_wall_ms);
            }
        }
        for v in values.values_mut() {
            v.sort_unstable();
        }
        for &i in members {
            if rows[i]
                .scenarios
                .iter()
                .any(|sc| is_outlier(sc.median_wall_ms, &values[sc.scenario_id.as_str()]))
            {
                marked.push(i);
            }
        }
    }
    for i in marked {
        rows[i].outlier = true;
        rows[i].excluded = true;
    }
}

/// Linear-interpolated percentile of sorted values.
pub fn percentile(sorted: &[u64], p: f64) -> u64 {
    match sorted.len() {
        0 => 0,
        1 => sorted[0],
        n => {
            let rank = p * (n - 1) as f64;
            let lo = rank.floor() as usize;
            let hi = rank.ceil() as usize;
            let frac = rank - lo as f64;
            (sorted[lo] as f64 + (sorted[hi] as f64 - sorted[lo] as f64) * frac).round() as u64
        }
    }
}

impl Snapshot {
    /// Rows of one suite version matching `filter` that have `scenario`.
    pub fn rows_for<'a>(
        &'a self,
        suite: &'a str,
        version: u32,
        scenario: &'a str,
        filter: &'a Filter,
    ) -> impl Iterator<Item = &'a ResultRow> + 'a {
        self.rows.iter().filter(move |r| {
            r.suite_id == suite
                && r.suite_version == version
                && filter.matches(r)
                && r.scenario(scenario).is_some()
        })
    }

    /// Per-configuration aggregates, fastest median first.
    pub fn configs(
        &self,
        suite: &str,
        version: u32,
        scenario: &str,
        filter: &Filter,
    ) -> Vec<ConfigStats> {
        let mut groups: BTreeMap<ConfigKey, Vec<&ResultRow>> = BTreeMap::new();
        for r in self.rows_for(suite, version, scenario, filter) {
            let key = ConfigKey {
                cpu_family: r.cpu_family.clone(),
                mem_gb: r.mem_gb,
                os_name: r.os_name.clone(),
            };
            groups.entry(key).or_default().push(r);
        }
        let mut out: Vec<ConfigStats> = groups
            .into_iter()
            .map(|(key, rows)| {
                let mut medians: Vec<u64> = rows
                    .iter()
                    .filter_map(|r| r.scenario(scenario))
                    .map(|s| s.median_wall_ms)
                    .collect();
                medians.sort_unstable();
                let mut families: Vec<String> = rows
                    .iter()
                    .filter_map(|r| r.machine_family.clone())
                    .collect();
                families.sort();
                families.dedup();
                let swapped = rows
                    .iter()
                    .filter_map(|r| r.scenario(scenario))
                    .filter(|s| s.peak_swap.unwrap_or(0) > 256 * 1024 * 1024)
                    .count();
                ConfigStats {
                    n: medians.len(),
                    median: percentile(&medians, 0.5),
                    p25: percentile(&medians, 0.25),
                    p75: percentile(&medians, 0.75),
                    vendor: rows.iter().find_map(|r| r.cpu_vendor.clone()),
                    logical_cores: mode(rows.iter().filter_map(|r| r.logical_cores)),
                    families,
                    swap_share: swapped as f64 / rows.len() as f64,
                    result_ids: rows.iter().map(|r| r.id.clone()).collect(),
                    key,
                }
            })
            .collect();
        out.sort_by(|a, b| a.median.cmp(&b.median).then_with(|| a.key.cmp(&b.key)));
        out
    }

    /// Result by id.
    pub fn row(&self, id: &str) -> Option<&ResultRow> {
        self.rows.iter().find(|r| r.id == id)
    }
}

fn mode(values: impl Iterator<Item = u32>) -> Option<u32> {
    let mut counts: BTreeMap<u32, usize> = BTreeMap::new();
    for v in values {
        *counts.entry(v).or_default() += 1;
    }
    counts
        .into_iter()
        .max_by_key(|&(v, c)| (c, std::cmp::Reverse(v)))
        .map(|(v, _)| v)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::ScenarioRow;

    fn row(id: &str, version: u32, mem_gb: u32, excluded: bool, ms: u64) -> ResultRow {
        ResultRow {
            id: id.into(),
            suite_id: "rust-quick".into(),
            suite_version: version,
            cpu_family: "Apple M3 Pro".into(),
            mem_gb,
            os_name: "macOS".into(),
            excluded,
            scenarios: vec![ScenarioRow {
                scenario_id: "clean-release".into(),
                median_wall_ms: ms,
                ..ScenarioRow::default()
            }],
            ..ResultRow::default()
        }
    }

    #[test]
    fn outliers_compare_only_unflagged_peers_of_one_configuration() {
        let peers = || (0..4).map(|i| row(&format!("p{i}"), 1, 36, false, 40_000 + i * 500));
        let cases: &[(&str, ResultRow, bool)] = &[
            ("fast", row("x", 1, 36, false, 20_000), true),
            ("agrees", row("x", 1, 36, false, 41_000), false),
            ("other version", row("x", 2, 36, false, 20_000), false),
            ("other memory", row("x", 1, 18, false, 20_000), false),
            ("already flagged", row("x", 1, 36, true, 20_000), false),
        ];
        for (name, candidate, outlier) in cases {
            let mut rows: Vec<ResultRow> = peers().collect();
            rows.push(candidate.clone());
            mark_outliers(&mut rows);
            let x = rows.iter().find(|r| r.id == "x").unwrap();
            assert_eq!(x.outlier, *outlier, "{name}");
            assert!(
                rows.iter().filter(|r| r.id != "x").all(|r| !r.outlier),
                "{name}"
            );
        }
        let mut flagged_peers: Vec<ResultRow> = peers()
            .map(|mut r| {
                r.excluded = true;
                r
            })
            .collect();
        flagged_peers.push(row("x", 1, 36, false, 20_000));
        mark_outliers(&mut flagged_peers);
        assert!(
            !flagged_peers[4].outlier,
            "flagged results counted as peers"
        );
    }
}
