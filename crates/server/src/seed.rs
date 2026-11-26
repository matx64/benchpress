//! Fake results for local design work. They carry client version `*-seed`, the site
//! shows a "seed data" banner while any exist, and none of this runs in production.

use anyhow::Result;
use benchpress_core::result::{
    BenchResult, Client, Cpu, Environment, Hardware, LinuxEnv, MacEnv, MemPressure, Memory, Os,
    Power, PowerSource, RESULT_SCHEMA_VERSION, Run, ScenarioResult, Settings, Storage, SuiteRef,
    Summary, Thermal,
};
use benchpress_core::validate::{Flags, validate_result};
use benchpress_core::{Registry, Tier};
use sqlx::SqlitePool;

use crate::{api, db};

struct Machine {
    brand: &'static str,
    vendor: &'static str,
    family: &'static str,
    model_id: &'static str,
    form: &'static str,
    os: (&'static str, &'static str),
    arch: &'static str,
    logical: u32,
    pe: Option<(u32, u32)>,
    speed: f64,
    rams: &'static [u32],
}

const MACHINES: &[Machine] = &[
    Machine {
        brand: "Apple M1",
        vendor: "Apple",
        family: "MacBook Air",
        model_id: "MacBookAir10,1",
        form: "laptop",
        os: ("macOS", "15.1"),
        arch: "aarch64",
        logical: 8,
        pe: Some((4, 4)),
        speed: 0.78,
        rams: &[8, 16],
    },
    Machine {
        brand: "Apple M2 Pro",
        vendor: "Apple",
        family: "Mac mini",
        model_id: "Mac14,12",
        form: "desktop",
        os: ("macOS", "15.1"),
        arch: "aarch64",
        logical: 12,
        pe: Some((8, 4)),
        speed: 0.92,
        rams: &[16, 32],
    },
    Machine {
        brand: "Apple M3 Pro",
        vendor: "Apple",
        family: "MacBook Pro",
        model_id: "Mac15,6",
        form: "laptop",
        os: ("macOS", "15.1"),
        arch: "aarch64",
        logical: 12,
        pe: Some((6, 6)),
        speed: 1.0,
        rams: &[18, 36],
    },
    Machine {
        brand: "Apple M3 Max",
        vendor: "Apple",
        family: "MacBook Pro",
        model_id: "Mac15,9",
        form: "laptop",
        os: ("macOS", "15.1"),
        arch: "aarch64",
        logical: 16,
        pe: Some((12, 4)),
        speed: 1.05,
        rams: &[36, 48],
    },
    Machine {
        brand: "Apple M4",
        vendor: "Apple",
        family: "MacBook Air",
        model_id: "Mac16,12",
        form: "laptop",
        os: ("macOS", "15.2"),
        arch: "aarch64",
        logical: 10,
        pe: Some((4, 6)),
        speed: 1.12,
        rams: &[16, 24, 32],
    },
    Machine {
        brand: "Apple M4 Pro",
        vendor: "Apple",
        family: "MacBook Pro",
        model_id: "Mac16,8",
        form: "laptop",
        os: ("macOS", "15.2"),
        arch: "aarch64",
        logical: 14,
        pe: Some((10, 4)),
        speed: 1.15,
        rams: &[24, 48],
    },
    Machine {
        brand: "Apple M4 Max",
        vendor: "Apple",
        family: "Mac Studio",
        model_id: "Mac16,9",
        form: "desktop",
        os: ("macOS", "15.2"),
        arch: "aarch64",
        logical: 16,
        pe: Some((12, 4)),
        speed: 1.18,
        rams: &[64],
    },
    Machine {
        brand: "AMD Ryzen 7 7840U w/ Radeon 780M Graphics",
        vendor: "AMD",
        family: "ThinkPad T14s Gen 4",
        model_id: "21F8",
        form: "laptop",
        os: ("Linux", "Fedora Linux 41"),
        arch: "x86_64",
        logical: 16,
        pe: None,
        speed: 0.82,
        rams: &[16, 32],
    },
    Machine {
        brand: "AMD Ryzen 9 7950X 16-Core Processor",
        vendor: "AMD",
        family: "B650 AORUS ELITE",
        model_id: "B650 AORUS ELITE",
        form: "desktop",
        os: ("Linux", "Ubuntu 24.04.1 LTS"),
        arch: "x86_64",
        logical: 32,
        pe: None,
        speed: 1.08,
        rams: &[64],
    },
    Machine {
        brand: "AMD Ryzen 7 9800X3D 8-Core Processor",
        vendor: "AMD",
        family: "X870E Taichi",
        model_id: "X870E Taichi",
        form: "desktop",
        os: ("Linux", "Arch Linux"),
        arch: "x86_64",
        logical: 16,
        pe: None,
        speed: 1.2,
        rams: &[32, 64],
    },
    Machine {
        brand: "AMD Ryzen AI 9 HX 370 w/ Radeon 890M",
        vendor: "AMD",
        family: "Zenbook S 16",
        model_id: "UM5606WA",
        form: "laptop",
        os: ("Linux", "Ubuntu 24.10"),
        arch: "x86_64",
        logical: 24,
        pe: None,
        speed: 0.95,
        rams: &[32],
    },
    Machine {
        brand: "13th Gen Intel(R) Core(TM) i7-1360P",
        vendor: "Intel",
        family: "XPS 13 9315",
        model_id: "XPS 13 9315",
        form: "laptop",
        os: ("Linux", "Ubuntu 24.04.1 LTS"),
        arch: "x86_64",
        logical: 16,
        pe: Some((4, 8)),
        speed: 0.7,
        rams: &[16, 32],
    },
    Machine {
        brand: "Intel(R) Core(TM) Ultra 7 155H",
        vendor: "Intel",
        family: "ThinkPad X1 Carbon Gen 12",
        model_id: "21KC",
        form: "laptop",
        os: ("Linux", "Fedora Linux 41"),
        arch: "x86_64",
        logical: 22,
        pe: Some((6, 10)),
        speed: 0.8,
        rams: &[32],
    },
    Machine {
        brand: "13th Gen Intel(R) Core(TM) i9-13900K",
        vendor: "Intel",
        family: "PRO Z790-A WIFI",
        model_id: "MS-7E07",
        form: "desktop",
        os: ("Linux", "Debian GNU/Linux 12 (bookworm)"),
        arch: "x86_64",
        logical: 32,
        pe: Some((8, 16)),
        speed: 1.02,
        rams: &[32, 64],
    },
];

struct Lcg(u64);

impl Lcg {
    fn next(&mut self) -> f64 {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        ((self.0 >> 33) as f64) / f64::from(1u32 << 31)
    }
}

fn base_release_ms(tier: Tier) -> f64 {
    match tier {
        Tier::Quick => 14_000.0,
        Tier::Standard => 150_000.0,
        Tier::Heavy => 420_000.0,
    }
}

/// Insert `count` fake results.
pub async fn run(pool: &SqlitePool, count: usize) -> Result<()> {
    tracing::warn!("inserting seed data: never run this against production");
    let mut inserted = 0;
    for (mut result, flags) in generate(count) {
        result.client.version = format!("{}-seed", benchpress_core::VERSION);
        let fp = benchpress_core::hash::result_fingerprint(&result)?;
        if matches!(
            db::insert(
                pool,
                &api::public_id(),
                &fp,
                "seed",
                &result.started_at,
                &result,
                &flags
            )
            .await?,
            db::Inserted::Ok
        ) {
            inserted += 1;
        }
    }
    tracing::info!(inserted, "seed data inserted");
    Ok(())
}

/// `count` deterministic, valid results with their flags.
pub fn generate(count: usize) -> Vec<(BenchResult, Flags)> {
    let registry = Registry::builtin();
    let mut rng = Lcg(42);
    let mut out = Vec::with_capacity(count);
    let mut i = 0usize;
    while out.len() < count {
        let m = &MACHINES[i % MACHINES.len()];
        let ram = m.rams[(i / MACHINES.len()) % m.rams.len()];
        let tier = match i % 6 {
            0 => Tier::Quick,
            5 => Tier::Heavy,
            _ => Tier::Standard,
        };
        i += 1;
        let suite = registry.by_tier(tier).expect("builtin suite");
        let manifest = &suite.manifest;
        let parallel = (16.0 / f64::from(m.logical)).powf(0.45);
        let mem_penalty = if ram <= 8 {
            1.45
        } else if ram <= 16 && tier == Tier::Heavy {
            1.2
        } else {
            1.0
        };
        let release =
            base_release_ms(tier) * parallel * mem_penalty / m.speed * (0.94 + rng.next() * 0.12);
        let total_bytes = u64::from(ram) * (1 << 30) - 300 * (1 << 20);
        let swap_peak = if mem_penalty > 1.0 {
            u64::from(ram) * (1 << 28)
        } else {
            0
        };
        let battery = rng.next() < 0.08 && m.form == "laptop";
        let scenarios = manifest
            .scenarios
            .iter()
            .map(|sc| {
                let factor = match sc.id.as_str() {
                    "clean-debug" => 0.62,
                    "clean-release" => 1.0,
                    "check" => 0.36,
                    "incremental-leaf" => 0.018,
                    _ => 0.07,
                };
                let mean = (release * factor).max(sc.min_wall_ms as f64 * 1.5);
                let util = if sc.id.starts_with("incremental") {
                    0.18
                } else {
                    0.62 + rng.next() * 0.3
                };
                let runs: Vec<Run> =
                    (0..manifest.runs.count)
                        .map(|_| {
                            let wall = (mean * (0.985 + rng.next() * 0.03)) as u64;
                            let cpu = wall as f64 * f64::from(m.logical) * util;
                            Run {
                                wall_ms: wall,
                                user_ms: (cpu * 0.93) as u64,
                                sys_ms: (cpu * 0.07) as u64,
                                baseline_mem_used_bytes: total_bytes / 4,
                                peak_mem_used_bytes: (total_bytes / 4
                                    + (total_bytes as f64 * 0.45 * factor.min(1.0)) as u64)
                                    .min(total_bytes),
                                baseline_swap_used_bytes: 0,
                                peak_swap_used_bytes: if sc.kind
                                    == benchpress_core::suite::ScenarioKind::Clean
                                {
                                    swap_peak
                                } else {
                                    0
                                },
                                mem_pressure: MemPressure {
                                    some_avg10_max: (m.os.0 == "Linux")
                                        .then_some(if swap_peak > 0 { 14.0 } else { 0.4 }),
                                },
                                cpu_util_avg: (util * 1000.0).round() / 1000.0,
                                thermal_start: (m.os.0 == "macOS").then_some(Thermal::Nominal),
                                thermal_end: (m.os.0 == "macOS").then_some(if m.form == "laptop" {
                                    Thermal::Fair
                                } else {
                                    Thermal::Nominal
                                }),
                                exit_ok: true,
                            }
                        })
                        .collect();
                ScenarioResult {
                    id: sc.id.clone(),
                    summary: Summary::from_runs(&runs).expect("runs"),
                    runs,
                }
            })
            .collect();
        let mac = m.os.0 == "macOS";
        let result = BenchResult {
            schema_version: RESULT_SCHEMA_VERSION,
            client: Client {
                version: benchpress_core::VERSION.into(),
                target: if mac {
                    "aarch64-apple-darwin".into()
                } else {
                    format!("{}-unknown-linux-musl", m.arch)
                },
            },
            suite: SuiteRef {
                id: manifest.id.clone(),
                version: manifest.version,
                hash: suite.hash.clone(),
            },
            started_at: format!(
                "2026-11-{:02}T{:02}:{:02}:00Z",
                1 + i % 25,
                8 + i % 12,
                (i * 7) % 60
            ),
            settings: Settings {
                runs: manifest.runs.count,
                cooldown_secs: 10,
            },
            hardware: Hardware {
                cpu: Cpu {
                    brand: m.brand.into(),
                    vendor: Some(m.vendor.into()),
                    arch: m.arch.into(),
                    physical_cores: Some(m.pe.map_or(m.logical / 2, |(p, e)| p + e)),
                    logical_cores: m.logical,
                    performance_cores: m.pe.map(|p| p.0),
                    efficiency_cores: m.pe.map(|p| p.1),
                    base_mhz: None,
                    max_mhz: None,
                },
                memory: Memory {
                    total_bytes,
                    swap_total_bytes: Some(8 << 30),
                    unified: mac,
                },
                storage: Storage {
                    kind: Some("nvme".into()),
                    model: None,
                    filesystem: Some(if mac { "apfs" } else { "ext4" }.into()),
                },
                machine: machine(m),
            },
            environment: Environment {
                os: Os {
                    name: m.os.0.into(),
                    version: Some(m.os.1.into()),
                    kernel: Some(if mac { "24.1.0" } else { "6.11.0" }.into()),
                },
                power: Power {
                    source: Some(if battery {
                        PowerSource::Battery
                    } else {
                        PowerSource::Ac
                    }),
                    low_power_mode: Some(false),
                    profile: None,
                },
                thermal_at_start: mac.then_some(Thermal::Nominal),
                virtualization: None,
                mac: mac.then(|| MacEnv {
                    spotlight_excluded: true,
                    developer_tools_terminal: "unknown".into(),
                }),
                linux: (!mac).then(|| LinuxEnv {
                    governor: Some("powersave".into()),
                    psi_available: true,
                }),
                overrides_cleared: Vec::new(),
            },
            scenarios,
            samples_ref: None,
        };
        match validate_result(&result, &registry) {
            Ok(flags) => out.push((result, flags)),
            Err(e) => tracing::warn!(%e, "skipping invalid seed row"),
        }
    }
    out
}

fn machine(m: &Machine) -> benchpress_core::result::Machine {
    benchpress_core::result::Machine {
        vendor: Some(
            match m.vendor {
                "Apple" => "Apple",
                _ if m.family.starts_with("ThinkPad") => "Lenovo",
                _ if m.family.starts_with("XPS") => "Dell",
                _ if m.family.starts_with("Zenbook") => "ASUS",
                _ => "Custom build",
            }
            .into(),
        ),
        model_id: Some(m.model_id.into()),
        family: Some(m.family.into()),
        form_factor: Some(m.form.into()),
    }
}
