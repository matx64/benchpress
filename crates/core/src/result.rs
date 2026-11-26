//! The result document produced by the CLI and stored by the server.

use serde::{Deserialize, Serialize};

/// Schema version of result documents produced by this release.
pub const RESULT_SCHEMA_VERSION: u32 = 1;

/// One complete benchmark result.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BenchResult {
    /// Result schema version.
    pub schema_version: u32,
    /// The CLI that produced this result.
    pub client: Client,
    /// Which suite version was run.
    pub suite: SuiteRef,
    /// RFC 3339 UTC start time.
    pub started_at: String,
    /// Run settings that can affect results.
    pub settings: Settings,
    /// Hardware model information.
    pub hardware: Hardware,
    /// OS and machine state.
    pub environment: Environment,
    /// Measurements per scenario, in run order.
    pub scenarios: Vec<ScenarioResult>,
    /// Reserved for uploaded time-series samples.
    pub samples_ref: Option<String>,
}

/// CLI identity.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Client {
    /// Semver version of the CLI.
    pub version: String,
    /// Target triple the CLI was built for.
    pub target: String,
}

/// Reference to a suite version.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SuiteRef {
    /// Suite id.
    pub id: String,
    /// Suite version.
    pub version: u32,
    /// `sha256:` hash of the manifest.
    pub hash: String,
}

/// Settings chosen for this run.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Settings {
    /// Measured runs per scenario.
    pub runs: u32,
    /// Seconds slept between runs.
    pub cooldown_secs: u32,
}

/// Hardware model information. Never serials or identifiers.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Hardware {
    /// Processor.
    pub cpu: Cpu,
    /// Memory.
    pub memory: Memory,
    /// Disk holding the build directory.
    pub storage: Storage,
    /// Machine model.
    pub machine: Machine,
}

/// Processor details.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Cpu {
    /// Marketing brand string.
    pub brand: String,
    /// Vendor such as `Apple`, `AMD`, `Intel`.
    pub vendor: Option<String>,
    /// `aarch64` or `x86_64`.
    pub arch: String,
    /// Physical cores.
    pub physical_cores: Option<u32>,
    /// Logical cores (hardware threads).
    pub logical_cores: u32,
    /// Performance cores on hybrid designs.
    pub performance_cores: Option<u32>,
    /// Efficiency cores on hybrid designs.
    pub efficiency_cores: Option<u32>,
    /// Base frequency.
    pub base_mhz: Option<u32>,
    /// Maximum frequency.
    pub max_mhz: Option<u32>,
}

/// Memory details.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Memory {
    /// Installed RAM.
    pub total_bytes: u64,
    /// Configured swap.
    pub swap_total_bytes: Option<u64>,
    /// Unified memory shared with the GPU (Apple Silicon).
    pub unified: bool,
}

/// Storage details.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Storage {
    /// `nvme`, `ssd`, `hdd` or another transport.
    pub kind: Option<String>,
    /// Drive model name.
    pub model: Option<String>,
    /// Filesystem of the build directory.
    pub filesystem: Option<String>,
}

/// Machine model details.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Machine {
    /// Manufacturer.
    pub vendor: Option<String>,
    /// Model identifier such as `Mac15,6`.
    pub model_id: Option<String>,
    /// Product family such as `MacBook Pro`.
    pub family: Option<String>,
    /// `laptop`, `desktop`, `server` or `vm`.
    pub form_factor: Option<String>,
}

/// OS and machine state during the run.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Environment {
    /// Operating system.
    pub os: Os,
    /// Power state at start.
    pub power: Power,
    /// Thermal state at start.
    pub thermal_at_start: Option<Thermal>,
    /// Hypervisor or container, when detected.
    pub virtualization: Option<String>,
    /// macOS-specific facts.
    pub mac: Option<MacEnv>,
    /// Linux-specific facts.
    pub linux: Option<LinuxEnv>,
    /// Names (never values) of cargo/rustc variables found in the user's shell.
    pub overrides_cleared: Vec<String>,
}

/// Operating system.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Os {
    /// `macOS` or `Linux`.
    pub name: String,
    /// Product version or distribution name.
    pub version: Option<String>,
    /// Kernel release.
    pub kernel: Option<String>,
}

/// Power state.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Power {
    /// `ac`, `battery`, or unknown.
    pub source: Option<PowerSource>,
    /// macOS Low Power Mode, or a Linux power-saver profile.
    pub low_power_mode: Option<bool>,
    /// Platform profile or governor, e.g. `balanced`.
    pub profile: Option<String>,
}

/// Power source.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PowerSource {
    /// Mains power.
    Ac,
    /// Running on battery.
    Battery,
}

/// Thermal pressure, as reported by macOS.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Thermal {
    /// No throttling.
    Nominal,
    /// Slightly elevated.
    Fair,
    /// Throttling likely.
    Serious,
    /// Heavy throttling.
    Critical,
}

/// macOS facts.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MacEnv {
    /// Whether the workdir was excluded from Spotlight.
    pub spotlight_excluded: bool,
    /// Whether the terminal is listed under Developer Tools. Unknowable without privileges.
    pub developer_tools_terminal: String,
}

/// Linux facts.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LinuxEnv {
    /// CPU frequency governor.
    pub governor: Option<String>,
    /// Whether `/proc/pressure/memory` was available.
    pub psi_available: bool,
}

/// Measurements for one scenario.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ScenarioResult {
    /// Scenario id from the manifest.
    pub id: String,
    /// Each measured run.
    pub runs: Vec<Run>,
    /// Statistics over `runs`.
    pub summary: Summary,
}

/// One measured run.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Run {
    /// Wall time, spawn to exit.
    pub wall_ms: u64,
    /// User CPU time of all child processes.
    pub user_ms: u64,
    /// System CPU time of all child processes.
    pub sys_ms: u64,
    /// System memory in use before the run.
    pub baseline_mem_used_bytes: u64,
    /// Peak system memory in use during the run.
    pub peak_mem_used_bytes: u64,
    /// Swap in use before the run.
    pub baseline_swap_used_bytes: u64,
    /// Peak swap in use during the run.
    pub peak_swap_used_bytes: u64,
    /// Memory pressure, where the OS reports it.
    pub mem_pressure: MemPressure,
    /// `(user + sys) / (wall × logical cores)`.
    pub cpu_util_avg: f64,
    /// Thermal state when the run started.
    pub thermal_start: Option<Thermal>,
    /// Thermal state when the run ended.
    pub thermal_end: Option<Thermal>,
    /// Whether cargo exited successfully.
    pub exit_ok: bool,
}

impl Run {
    /// Peak memory above the pre-run baseline.
    pub fn peak_mem_delta_bytes(&self) -> u64 {
        self.peak_mem_used_bytes
            .saturating_sub(self.baseline_mem_used_bytes)
    }

    /// Peak swap above the pre-run baseline.
    pub fn peak_swap_delta_bytes(&self) -> u64 {
        self.peak_swap_used_bytes
            .saturating_sub(self.baseline_swap_used_bytes)
    }
}

/// Memory pressure metrics.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MemPressure {
    /// Highest Linux PSI `some avg10` seen during the run.
    pub some_avg10_max: Option<f64>,
}

/// Statistics over a scenario's runs.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Summary {
    /// Median wall time.
    pub median_wall_ms: u64,
    /// Fastest run.
    pub min_wall_ms: u64,
    /// Slowest run.
    pub max_wall_ms: u64,
    /// Coefficient of variation (sample standard deviation / mean).
    pub cv: f64,
}

impl Summary {
    /// Compute statistics over wall times. Returns `None` for no runs.
    pub fn from_runs(runs: &[Run]) -> Option<Summary> {
        let walls: Vec<u64> = runs.iter().map(|r| r.wall_ms).collect();
        Summary::from_walls(&walls)
    }

    /// Compute statistics over raw wall times.
    pub fn from_walls(walls: &[u64]) -> Option<Summary> {
        if walls.is_empty() {
            return None;
        }
        let mut sorted = walls.to_vec();
        sorted.sort_unstable();
        let n = sorted.len();
        let median = if n % 2 == 1 {
            sorted[n / 2]
        } else {
            (sorted[n / 2 - 1] + sorted[n / 2]).div_ceil(2)
        };
        let mean = sorted.iter().map(|&w| w as f64).sum::<f64>() / n as f64;
        let cv = if n < 2 || mean == 0.0 {
            0.0
        } else {
            let var = sorted
                .iter()
                .map(|&w| (w as f64 - mean).powi(2))
                .sum::<f64>()
                / (n - 1) as f64;
            round4(var.sqrt() / mean)
        };
        Some(Summary {
            median_wall_ms: median,
            min_wall_ms: sorted[0],
            max_wall_ms: sorted[n - 1],
            cv,
        })
    }
}

fn round4(x: f64) -> f64 {
    (x * 10_000.0).round() / 10_000.0
}

impl ScenarioResult {
    /// Mean CPU utilization over runs.
    pub fn cpu_util(&self) -> Option<f64> {
        if self.runs.is_empty() {
            return None;
        }
        Some(self.runs.iter().map(|r| r.cpu_util_avg).sum::<f64>() / self.runs.len() as f64)
    }

    /// Largest memory increase over baseline across runs.
    pub fn peak_mem_delta(&self) -> u64 {
        self.runs
            .iter()
            .map(Run::peak_mem_delta_bytes)
            .max()
            .unwrap_or(0)
    }

    /// Largest swap increase over baseline across runs.
    pub fn peak_swap_delta(&self) -> u64 {
        self.runs
            .iter()
            .map(Run::peak_swap_delta_bytes)
            .max()
            .unwrap_or(0)
    }
}

impl BenchResult {
    /// Scenario by id.
    pub fn scenario(&self, id: &str) -> Option<&ScenarioResult> {
        self.scenarios.iter().find(|s| s.id == id)
    }

    /// One-line machine description, e.g. `MacBook Pro · Apple M3 Pro · 36 GB`.
    pub fn machine_line(&self) -> String {
        let mut parts = Vec::new();
        if let Some(f) = &self.hardware.machine.family {
            parts.push(f.clone());
        }
        parts.push(crate::hardware::cpu_family(&self.hardware.cpu.brand));
        parts.push(format!(
            "{} GB",
            crate::hardware::memory_bucket(self.hardware.memory.total_bytes)
        ));
        parts.join(" · ")
    }
}
