//! Result validation shared by the CLI (before submitting) and the server (on receipt).

use serde::{Deserialize, Serialize};

use crate::registry::Registry;
use crate::result::{BenchResult, PowerSource, RESULT_SCHEMA_VERSION, Summary};

/// Largest accepted payload.
pub const MAX_PAYLOAD_BYTES: usize = 64 * 1024;

/// Oldest client release the server accepts.
pub const MIN_CLIENT_VERSION: &str = "0.1.0";

/// Coefficient of variation above which a result is flagged.
pub const HIGH_CV: f64 = 0.1;

/// Why a result was rejected. `code` is stable and machine-readable.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Rejection {
    /// Stable machine code, e.g. `suite_hash_mismatch`.
    pub code: String,
    /// Human explanation.
    pub message: String,
}

impl Rejection {
    fn new(code: &str, message: impl Into<String>) -> Self {
        Rejection {
            code: code.to_owned(),
            message: message.into(),
        }
    }
}

impl std::fmt::Display for Rejection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} ({})", self.message, self.code)
    }
}

impl std::error::Error for Rejection {}

/// A reason an accepted result is excluded from aggregates by default.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Flag {
    /// Ran on battery power.
    Battery,
    /// Low power mode or a power-saver profile was active.
    LowPower,
    /// Ran inside a VM or container.
    Virtualized,
    /// Runs varied by more than [`HIGH_CV`].
    HighVariance,
}

impl Flag {
    /// Stable identifier.
    pub fn as_str(self) -> &'static str {
        match self {
            Flag::Battery => "battery",
            Flag::LowPower => "low_power",
            Flag::Virtualized => "virtualized",
            Flag::HighVariance => "high_variance",
        }
    }

    /// Plain-language explanation.
    pub fn explain(self) -> &'static str {
        match self {
            Flag::Battery => "Ran on battery power, which often lowers CPU clocks.",
            Flag::LowPower => "Low power mode or a power-saving profile was active.",
            Flag::Virtualized => {
                "Ran inside a virtual machine or container, so hardware may be shared."
            }
            Flag::HighVariance => {
                "Run times varied by more than 10%, so something else was likely competing for the machine."
            }
        }
    }
}

/// Flags raised by validation. Any flag excludes the result from aggregates by default.
pub type Flags = Vec<Flag>;

/// Reject oversized payloads before parsing.
pub fn validate_payload_size(len: usize) -> Result<(), Rejection> {
    if len > MAX_PAYLOAD_BYTES {
        return Err(Rejection::new(
            "payload_too_large",
            format!("payload is {len} bytes; the limit is {MAX_PAYLOAD_BYTES}"),
        ));
    }
    Ok(())
}

/// Check a result against every server rule. Returns flags on success.
pub fn validate_result(result: &BenchResult, registry: &Registry) -> Result<Flags, Rejection> {
    if result.schema_version != RESULT_SCHEMA_VERSION {
        return Err(Rejection::new(
            "unsupported_schema",
            format!(
                "result schema_version {} is not supported",
                result.schema_version
            ),
        ));
    }
    let min = semver::Version::parse(MIN_CLIENT_VERSION).expect("valid constant");
    match semver::Version::parse(&result.client.version) {
        Ok(v) if v >= min => {}
        _ => {
            return Err(Rejection::new(
                "unsupported_client",
                format!(
                    "client version `{}` is not supported; update benchpress to {MIN_CLIENT_VERSION} or newer",
                    result.client.version
                ),
            ));
        }
    }
    let Some(suite) = registry.get(&result.suite.id, result.suite.version) else {
        return Err(Rejection::new(
            "unknown_suite",
            format!(
                "suite {}@{} is not known to this server",
                result.suite.id, result.suite.version
            ),
        ));
    };
    if suite.hash != result.suite.hash {
        return Err(Rejection::new(
            "suite_hash_mismatch",
            format!(
                "suite {}@{} hash does not match the released manifest",
                result.suite.id, result.suite.version
            ),
        ));
    }
    if time::OffsetDateTime::parse(
        &result.started_at,
        &time::format_description::well_known::Rfc3339,
    )
    .is_err()
    {
        return Err(Rejection::new(
            "invalid_timestamp",
            "started_at must be an RFC 3339 timestamp",
        ));
    }
    let manifest = &suite.manifest;
    let cpu = &result.hardware.cpu;
    let mem = &result.hardware.memory;
    if cpu.logical_cores == 0 || cpu.brand.trim().is_empty() || mem.total_bytes == 0 {
        return Err(Rejection::new(
            "missing_hardware",
            "cpu brand, logical cores and total memory are required",
        ));
    }
    if result.environment.os.name != "macOS" && result.environment.os.name != "Linux" {
        return Err(Rejection::new(
            "unsupported_os",
            "only macOS and Linux results are accepted",
        ));
    }
    if result.scenarios.len() != manifest.scenarios.len() {
        return Err(Rejection::new(
            "scenario_mismatch",
            format!(
                "expected {} scenarios, got {}",
                manifest.scenarios.len(),
                result.scenarios.len()
            ),
        ));
    }
    let mem_ceiling = mem
        .total_bytes
        .saturating_add(mem.swap_total_bytes.unwrap_or(0));
    let mut max_cv: f64 = 0.0;
    for expected in &manifest.scenarios {
        let Some(scen) = result.scenario(&expected.id) else {
            return Err(Rejection::new(
                "scenario_mismatch",
                format!("scenario `{}` is missing", expected.id),
            ));
        };
        if scen.runs.len() < manifest.runs.count as usize {
            return Err(Rejection::new(
                "missing_runs",
                format!(
                    "scenario `{}` has {} runs; the suite requires {}",
                    scen.id,
                    scen.runs.len(),
                    manifest.runs.count
                ),
            ));
        }
        for run in &scen.runs {
            if !run.exit_ok {
                return Err(Rejection::new(
                    "run_failed",
                    format!("a run of `{}` did not exit successfully", scen.id),
                ));
            }
            if run.wall_ms < expected.min_wall_ms {
                return Err(Rejection::new(
                    "implausible_wall_time",
                    format!(
                        "`{}` took {} ms, below the {} ms floor for this suite",
                        scen.id, run.wall_ms, expected.min_wall_ms
                    ),
                ));
            }
            let cpu_ceiling = run.wall_ms as f64 * f64::from(cpu.logical_cores) * 1.1;
            if (run.user_ms + run.sys_ms) as f64 > cpu_ceiling {
                return Err(Rejection::new(
                    "implausible_cpu_time",
                    format!(
                        "`{}` reports more CPU time than {} cores can provide",
                        scen.id, cpu.logical_cores
                    ),
                ));
            }
            if run.peak_mem_used_bytes > mem_ceiling
                || run.baseline_mem_used_bytes > mem_ceiling
                || run.peak_swap_used_bytes > mem_ceiling
            {
                return Err(Rejection::new(
                    "implausible_memory",
                    format!(
                        "`{}` reports more memory in use than RAM plus swap",
                        scen.id
                    ),
                ));
            }
            if !run.cpu_util_avg.is_finite() || run.cpu_util_avg < 0.0 || run.cpu_util_avg > 1.1 {
                return Err(Rejection::new(
                    "implausible_cpu_util",
                    format!("`{}` CPU utilization is out of range", scen.id),
                ));
            }
            if run
                .mem_pressure
                .some_avg10_max
                .is_some_and(|p| !(0.0..=100.0).contains(&p))
            {
                return Err(Rejection::new(
                    "implausible_pressure",
                    format!("`{}` memory pressure is out of range", scen.id),
                ));
            }
        }
        let recomputed = Summary::from_runs(&scen.runs).expect("runs checked non-empty");
        if recomputed.median_wall_ms != scen.summary.median_wall_ms
            || recomputed.min_wall_ms != scen.summary.min_wall_ms
            || recomputed.max_wall_ms != scen.summary.max_wall_ms
            || (recomputed.cv - scen.summary.cv).abs() > 1e-3
        {
            return Err(Rejection::new(
                "summary_mismatch",
                format!("summary of `{}` does not match its runs", scen.id),
            ));
        }
        max_cv = max_cv.max(recomputed.cv);
    }

    let env = &result.environment;
    let mut flags = Flags::new();
    if env.power.source == Some(PowerSource::Battery) {
        flags.push(Flag::Battery);
    }
    if env.power.low_power_mode == Some(true) {
        flags.push(Flag::LowPower);
    }
    if env.virtualization.is_some() {
        flags.push(Flag::Virtualized);
    }
    if max_cv > HIGH_CV {
        flags.push(Flag::HighVariance);
    }
    Ok(flags)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::result::*;

    pub(crate) fn sample(registry: &Registry) -> BenchResult {
        let suite = registry.latest("rust-quick").expect("builtin quick suite");
        let run = |wall_ms| Run {
            wall_ms,
            user_ms: wall_ms * 8,
            sys_ms: wall_ms / 2,
            baseline_mem_used_bytes: 4_000_000_000,
            peak_mem_used_bytes: 9_000_000_000,
            baseline_swap_used_bytes: 0,
            peak_swap_used_bytes: 0,
            mem_pressure: MemPressure {
                some_avg10_max: Some(0.0),
            },
            cpu_util_avg: 0.71,
            thermal_start: Some(Thermal::Nominal),
            thermal_end: Some(Thermal::Fair),
            exit_ok: true,
        };
        let scenarios = suite
            .manifest
            .scenarios
            .iter()
            .map(|s| {
                let runs = vec![run(41_000), run(41_200), run(40_900)];
                ScenarioResult {
                    id: s.id.clone(),
                    summary: Summary::from_runs(&runs).expect("runs"),
                    runs,
                }
            })
            .collect();
        BenchResult {
            schema_version: RESULT_SCHEMA_VERSION,
            client: Client {
                version: crate::VERSION.into(),
                target: "aarch64-apple-darwin".into(),
            },
            suite: SuiteRef {
                id: suite.manifest.id.clone(),
                version: suite.manifest.version,
                hash: suite.hash.clone(),
            },
            started_at: "2026-11-26T14:03:00Z".into(),
            settings: Settings {
                runs: 3,
                cooldown_secs: 10,
            },
            hardware: Hardware {
                cpu: Cpu {
                    brand: "Apple M3 Pro".into(),
                    vendor: Some("Apple".into()),
                    arch: "aarch64".into(),
                    physical_cores: Some(12),
                    logical_cores: 12,
                    performance_cores: Some(6),
                    efficiency_cores: Some(6),
                    base_mhz: None,
                    max_mhz: None,
                },
                memory: Memory {
                    total_bytes: 38_654_705_664,
                    swap_total_bytes: Some(0),
                    unified: true,
                },
                storage: Storage {
                    kind: Some("nvme".into()),
                    model: Some("APPLE SSD AP0512Z".into()),
                    filesystem: Some("apfs".into()),
                },
                machine: Machine {
                    vendor: Some("Apple".into()),
                    model_id: Some("Mac15,6".into()),
                    family: Some("MacBook Pro".into()),
                    form_factor: Some("laptop".into()),
                },
            },
            environment: Environment {
                os: Os {
                    name: "macOS".into(),
                    version: Some("15.1".into()),
                    kernel: Some("24.1.0".into()),
                },
                power: Power {
                    source: Some(PowerSource::Ac),
                    low_power_mode: Some(false),
                    profile: None,
                },
                thermal_at_start: Some(Thermal::Nominal),
                virtualization: None,
                mac: Some(MacEnv {
                    spotlight_excluded: true,
                    developer_tools_terminal: "unknown".into(),
                }),
                linux: None,
                overrides_cleared: vec!["RUSTC_WRAPPER".into(), "RUSTFLAGS".into()],
            },
            scenarios,
            samples_ref: None,
        }
    }

    fn resummarize(r: &mut BenchResult) {
        for s in &mut r.scenarios {
            s.summary = Summary::from_runs(&s.runs).expect("runs");
        }
    }

    #[test]
    fn rejections_and_flags() {
        type Mutate = fn(&mut BenchResult);
        let reg = Registry::builtin();
        let cases: &[(&str, Mutate, Result<&[Flag], &str>)] = &[
            ("valid", |_| {}, Ok(&[])),
            (
                "schema",
                |r| r.schema_version = 99,
                Err("unsupported_schema"),
            ),
            (
                "client too old",
                |r| r.client.version = "0.0.9".into(),
                Err("unsupported_client"),
            ),
            (
                "client not semver",
                |r| r.client.version = "dev".into(),
                Err("unsupported_client"),
            ),
            (
                "unknown suite",
                |r| r.suite.version = 999,
                Err("unknown_suite"),
            ),
            (
                "hash",
                |r| r.suite.hash = "sha256:00".into(),
                Err("suite_hash_mismatch"),
            ),
            (
                "timestamp",
                |r| r.started_at = "yesterday".into(),
                Err("invalid_timestamp"),
            ),
            (
                "no cores",
                |r| r.hardware.cpu.logical_cores = 0,
                Err("missing_hardware"),
            ),
            (
                "windows",
                |r| r.environment.os.name = "Windows".into(),
                Err("unsupported_os"),
            ),
            (
                "scenario dropped",
                |r| {
                    r.scenarios.pop();
                },
                Err("scenario_mismatch"),
            ),
            (
                "scenario renamed",
                |r| r.scenarios[0].id = "other".into(),
                Err("scenario_mismatch"),
            ),
            (
                "runs short",
                |r| {
                    r.scenarios[0].runs.pop();
                    resummarize(r);
                },
                Err("missing_runs"),
            ),
            (
                "exit failed",
                |r| r.scenarios[1].runs[0].exit_ok = false,
                Err("run_failed"),
            ),
            (
                "too fast",
                |r| {
                    r.scenarios[0].runs[0].wall_ms = 5;
                    resummarize(r);
                },
                Err("implausible_wall_time"),
            ),
            (
                "cpu time",
                |r| r.scenarios[0].runs[0].user_ms = 41_000 * 14,
                Err("implausible_cpu_time"),
            ),
            (
                "memory",
                |r| r.scenarios[0].runs[0].peak_mem_used_bytes = 40_000_000_000,
                Err("implausible_memory"),
            ),
            (
                "util",
                |r| r.scenarios[0].runs[0].cpu_util_avg = f64::NAN,
                Err("implausible_cpu_util"),
            ),
            (
                "pressure",
                |r| r.scenarios[0].runs[0].mem_pressure.some_avg10_max = Some(-1.0),
                Err("implausible_pressure"),
            ),
            (
                "summary",
                |r| r.scenarios[0].summary.median_wall_ms = 1,
                Err("summary_mismatch"),
            ),
            (
                "battery",
                |r| r.environment.power.source = Some(PowerSource::Battery),
                Ok(&[Flag::Battery]),
            ),
            (
                "low power",
                |r| r.environment.power.low_power_mode = Some(true),
                Ok(&[Flag::LowPower]),
            ),
            (
                "vm",
                |r| r.environment.virtualization = Some("kvm".into()),
                Ok(&[Flag::Virtualized]),
            ),
            (
                "variance",
                |r| {
                    r.scenarios[2].runs[0].wall_ms = 60_000;
                    resummarize(r);
                },
                Ok(&[Flag::HighVariance]),
            ),
        ];
        for (name, mutate, expected) in cases {
            let mut r = sample(&reg);
            mutate(&mut r);
            let got = validate_result(&r, &reg).map_err(|e| e.code);
            match expected {
                Ok(flags) => assert_eq!(got.as_deref(), Ok(*flags), "{name}"),
                Err(code) => assert_eq!(got.as_ref().map_err(String::as_str), Err(*code), "{name}"),
            }
        }
        assert!(validate_payload_size(MAX_PAYLOAD_BYTES).is_ok());
        assert_eq!(
            validate_payload_size(MAX_PAYLOAD_BYTES + 1).map_err(|e| e.code),
            Err("payload_too_large".into())
        );
    }
}
