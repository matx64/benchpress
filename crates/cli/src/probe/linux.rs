//! Linux probes: `/proc`, `/sys` and a few optional tools.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use benchpress_core::hardware::cpu_vendor;
use benchpress_core::result::{
    Cpu, Environment, Hardware, LinuxEnv, Machine, Memory, Os, Power, PowerSource, Storage, Thermal,
};

use super::{Probe, command_output, read_trimmed};

/// Probe backed by procfs and sysfs.
pub struct LinuxProbe;

impl Probe for LinuxProbe {
    fn hardware(&self, workdir: &Path) -> Hardware {
        Hardware {
            cpu: cpu(),
            memory: memory(),
            storage: storage(workdir),
            machine: machine(),
        }
    }

    fn environment(&self) -> Environment {
        Environment {
            os: Os {
                name: "Linux".into(),
                version: std::fs::read_to_string("/etc/os-release")
                    .ok()
                    .and_then(|s| parse_os_release(&s)),
                kernel: nix::sys::utsname::uname()
                    .ok()
                    .map(|u| u.release().to_string_lossy().into_owned()),
            },
            power: self.power(),
            thermal_at_start: None,
            virtualization: virtualization(),
            mac: None,
            linux: Some(LinuxEnv {
                governor: read_trimmed("/sys/devices/system/cpu/cpu0/cpufreq/scaling_governor"),
                psi_available: Path::new("/proc/pressure/memory").exists(),
            }),
            overrides_cleared: super::cargo_overrides(),
        }
    }

    fn power(&self) -> Power {
        let supplies = std::fs::read_dir("/sys/class/power_supply")
            .map(|rd| {
                rd.filter_map(Result::ok)
                    .map(|e| {
                        let p = e.path();
                        (
                            read_trimmed(p.join("type")).unwrap_or_default(),
                            read_trimmed(p.join("online")),
                            read_trimmed(p.join("status")),
                        )
                    })
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        let profile = read_trimmed("/sys/firmware/acpi/platform_profile")
            .or_else(|| command_output("powerprofilesctl", &["get"]));
        Power {
            source: power_source(&supplies),
            low_power_mode: profile
                .as_deref()
                .map(|p| matches!(p, "low-power" | "power-saver" | "quiet" | "cool")),
            profile,
        }
    }

    fn thermal(&self) -> Option<Thermal> {
        None
    }

    fn memory_pressure(&self) -> Option<f64> {
        parse_psi_some_avg10(&std::fs::read_to_string("/proc/pressure/memory").ok()?)
    }
}

/// `(type, online, status)` for each power supply.
pub(crate) fn power_source(
    supplies: &[(String, Option<String>, Option<String>)],
) -> Option<PowerSource> {
    let mains_online = supplies
        .iter()
        .any(|(t, online, _)| (t == "Mains" || t == "USB") && online.as_deref() == Some("1"));
    let batteries: Vec<_> = supplies.iter().filter(|(t, _, _)| t == "Battery").collect();
    if mains_online {
        return Some(PowerSource::Ac);
    }
    if batteries.is_empty() {
        let has_mains = supplies.iter().any(|(t, _, _)| t == "Mains");
        return (!has_mains || supplies.is_empty()).then_some(PowerSource::Ac);
    }
    if batteries
        .iter()
        .any(|(_, _, status)| status.as_deref() == Some("Discharging"))
    {
        Some(PowerSource::Battery)
    } else {
        Some(PowerSource::Ac)
    }
}

fn cpu() -> Cpu {
    let info = std::fs::read_to_string("/proc/cpuinfo").unwrap_or_default();
    let parsed = parse_cpuinfo(&info);
    let logical = std::thread::available_parallelism()
        .map_or(parsed.logical, |n| n.get() as u32)
        .max(parsed.logical);
    let brand = parsed
        .brand
        .or_else(|| lscpu_model_name(&command_output("lscpu", &[]).unwrap_or_default()))
        .unwrap_or_else(|| "Unknown CPU".into());
    let (p, e) = hybrid_cores();
    let max_khz = (0..logical)
        .filter_map(|i| {
            read_trimmed(format!(
                "/sys/devices/system/cpu/cpu{i}/cpufreq/cpuinfo_max_freq"
            ))
        })
        .filter_map(|s| s.parse::<u64>().ok())
        .max();
    Cpu {
        vendor: cpu_vendor(&brand).map(str::to_owned).or(parsed.vendor),
        brand,
        arch: super::arch(),
        physical_cores: physical_cores(),
        logical_cores: logical,
        performance_cores: p,
        efficiency_cores: e,
        base_mhz: read_trimmed("/sys/devices/system/cpu/cpu0/cpufreq/base_frequency")
            .and_then(|s| s.parse::<u64>().ok())
            .map(|k| (k / 1000) as u32),
        max_mhz: max_khz.map(|k| (k / 1000) as u32),
    }
}

#[derive(Debug, Default, PartialEq)]
pub(crate) struct CpuInfo {
    pub brand: Option<String>,
    pub vendor: Option<String>,
    pub logical: u32,
    pub hypervisor: bool,
}

pub(crate) fn parse_cpuinfo(text: &str) -> CpuInfo {
    let mut info = CpuInfo::default();
    for line in text.lines() {
        let Some((k, v)) = line.split_once(':') else {
            continue;
        };
        let (k, v) = (k.trim(), v.trim());
        match k {
            "processor" => info.logical += 1,
            "model name" | "Model Name" if info.brand.is_none() && !v.is_empty() => {
                info.brand = Some(v.to_owned());
            }
            "vendor_id" if info.vendor.is_none() => {
                info.vendor = Some(match v {
                    "GenuineIntel" => "Intel".to_owned(),
                    "AuthenticAMD" => "AMD".to_owned(),
                    other => other.to_owned(),
                });
            }
            "flags" if v.split_whitespace().any(|f| f == "hypervisor") => info.hypervisor = true,
            _ => {}
        }
    }
    info
}

pub(crate) fn lscpu_model_name(text: &str) -> Option<String> {
    let mut vendor = None;
    let mut model = None;
    for line in text.lines() {
        let Some((k, v)) = line.split_once(':') else {
            continue;
        };
        match k.trim() {
            "Vendor ID" => vendor = Some(v.trim().to_owned()),
            "Model name" if model.is_none() => model = Some(v.trim().to_owned()),
            _ => {}
        }
    }
    let model = model.filter(|m| !m.is_empty() && m != "-")?;
    Some(match vendor {
        Some(v) if !model.starts_with(&v) && v != "ARM" => format!("{v} {model}"),
        _ => model,
    })
}

fn cpu_topology(cpus: impl Iterator<Item = u32>) -> usize {
    cpus.filter_map(|i| {
        let base = format!("/sys/devices/system/cpu/cpu{i}/topology");
        Some((
            read_trimmed(format!("{base}/physical_package_id"))?,
            read_trimmed(format!("{base}/core_id"))?,
        ))
    })
    .collect::<BTreeSet<_>>()
    .len()
}

fn physical_cores() -> Option<u32> {
    let online = read_trimmed("/sys/devices/system/cpu/online")?;
    let n = cpu_topology(parse_cpu_list(&online).into_iter());
    (n > 0).then_some(n as u32)
}

fn hybrid_cores() -> (Option<u32>, Option<u32>) {
    let read =
        |kind: &str| read_trimmed(format!("/sys/devices/{kind}/cpus")).map(|l| parse_cpu_list(&l));
    match (read("cpu_core"), read("cpu_atom")) {
        (Some(p), Some(e)) if !p.is_empty() && !e.is_empty() => (
            Some(cpu_topology(p.into_iter()) as u32),
            Some(cpu_topology(e.into_iter()) as u32),
        ),
        _ => (None, None),
    }
}

/// Parse a kernel CPU list such as `0-3,8,10-11`.
pub(crate) fn parse_cpu_list(s: &str) -> Vec<u32> {
    s.trim()
        .split(',')
        .filter(|p| !p.is_empty())
        .flat_map(|part| match part.split_once('-') {
            Some((a, b)) => match (a.parse::<u32>(), b.parse::<u32>()) {
                (Ok(a), Ok(b)) if a <= b => (a..=b).collect(),
                _ => Vec::new(),
            },
            None => part.parse().into_iter().collect(),
        })
        .collect()
}

fn memory() -> Memory {
    let Ok(mut sys) = sysinfo::System::new() else {
        return Memory::default();
    };
    sys.refresh_memory();
    Memory {
        total_bytes: sys.total_memory(),
        swap_total_bytes: Some(sys.total_swap()),
        unified: false,
    }
}

fn machine() -> Machine {
    let dmi =
        |f: &str| read_trimmed(format!("/sys/class/dmi/id/{f}")).filter(|v| !is_placeholder(v));
    let vendor = dmi("sys_vendor");
    let name = dmi("product_name");
    let version = dmi("product_version");
    let family = match (vendor.as_deref(), &name, &version) {
        (Some(v), _, Some(ver)) if v.eq_ignore_ascii_case("LENOVO") && !ver.is_empty() => {
            Some(ver.clone())
        }
        _ => name.clone(),
    };
    let form_factor = if virtualization().is_some() {
        Some("vm".to_owned())
    } else {
        dmi("chassis_type")
            .and_then(|t| t.parse().ok())
            .and_then(form_factor)
            .map(str::to_owned)
    };
    Machine {
        vendor: vendor.map(|v| tidy_vendor(&v)),
        model_id: name,
        family,
        form_factor,
    }
}

fn tidy_vendor(v: &str) -> String {
    match v {
        "LENOVO" => "Lenovo".into(),
        "Dell Inc." => "Dell".into(),
        "ASUSTeK COMPUTER INC." => "ASUS".into(),
        "Micro-Star International Co., Ltd." => "MSI".into(),
        "HP" | "Hewlett-Packard" => "HP".into(),
        other => other.to_owned(),
    }
}

/// SMBIOS chassis type to a form factor.
pub(crate) fn form_factor(chassis: u32) -> Option<&'static str> {
    match chassis {
        8 | 9 | 10 | 11 | 14 | 30 | 31 | 32 => Some("laptop"),
        3..=7 | 13 | 15 | 16 | 24 | 34..=36 => Some("desktop"),
        17 | 23 | 25 | 28 | 29 => Some("server"),
        _ => None,
    }
}

fn is_placeholder(v: &str) -> bool {
    let l = v.to_ascii_lowercase();
    [
        "to be filled",
        "default string",
        "system product name",
        "system manufacturer",
        "not specified",
        "not applicable",
        "o.e.m.",
        "none",
        "type1productconfigid",
    ]
    .iter()
    .any(|p| l.contains(p))
        || l.chars().all(|c| c == 'x' || c == '0' || c == ' ')
}

#[derive(Debug, PartialEq)]
pub(crate) struct Mount {
    pub major_minor: String,
    pub mount_point: PathBuf,
    pub fstype: String,
}

/// Mount containing `path`, from `/proc/self/mountinfo` text.
pub(crate) fn mount_for(mountinfo: &str, path: &Path) -> Option<Mount> {
    mountinfo
        .lines()
        .filter_map(|line| {
            let (left, right) = line.split_once(" - ")?;
            let f: Vec<&str> = left.split_whitespace().collect();
            let fstype = right.split_whitespace().next()?;
            let mp = PathBuf::from(f.get(4)?.replace("\\040", " "));
            Some(Mount {
                major_minor: (*f.get(2)?).to_owned(),
                mount_point: mp,
                fstype: fstype.to_owned(),
            })
        })
        .filter(|m| path.starts_with(&m.mount_point))
        .max_by_key(|m| m.mount_point.as_os_str().len())
}

fn storage(workdir: &Path) -> Storage {
    let path = workdir
        .ancestors()
        .find(|p| p.exists())
        .unwrap_or(Path::new("/"));
    let path = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    let Some(mount) = std::fs::read_to_string("/proc/self/mountinfo")
        .ok()
        .and_then(|s| mount_for(&s, &path))
    else {
        return Storage::default();
    };
    let filesystem = Some(mount.fstype.clone());
    if mount.fstype == "tmpfs" {
        return Storage {
            kind: Some("ram".into()),
            model: None,
            filesystem,
        };
    }
    let Some(dev) = block_device(&mount.major_minor) else {
        return Storage {
            filesystem,
            ..Storage::default()
        };
    };
    let base = PathBuf::from("/sys/block").join(&dev);
    let rotational = read_trimmed(base.join("queue/rotational"));
    let kind = if dev.starts_with("nvme") {
        Some("nvme".into())
    } else if dev.starts_with("mmcblk") {
        Some("emmc".into())
    } else if dev.starts_with("vd") || dev.starts_with("xvd") {
        Some("virtual".into())
    } else {
        rotational.map(|r| if r == "1" { "hdd".into() } else { "ssd".into() })
    };
    Storage {
        kind,
        model: read_trimmed(base.join("device/model")),
        filesystem,
    }
}

/// Physical disk name (e.g. `nvme0n1`) behind a `major:minor`, following partitions and device-mapper.
fn block_device(major_minor: &str) -> Option<String> {
    let mut path = std::fs::canonicalize(format!("/sys/dev/block/{major_minor}")).ok()?;
    for _ in 0..8 {
        if path.join("partition").exists() {
            path = path.parent()?.to_path_buf();
            continue;
        }
        let slaves: Vec<PathBuf> = std::fs::read_dir(path.join("slaves"))
            .map(|rd| rd.filter_map(Result::ok).map(|e| e.path()).collect())
            .unwrap_or_default();
        match slaves.first() {
            Some(s) => path = std::fs::canonicalize(s).ok()?,
            None => break,
        }
    }
    Some(path.file_name()?.to_string_lossy().into_owned())
}

pub(crate) fn parse_os_release(text: &str) -> Option<String> {
    text.lines()
        .find_map(|l| l.strip_prefix("PRETTY_NAME="))
        .map(|v| v.trim_matches('"').to_owned())
}

pub(crate) fn parse_psi_some_avg10(text: &str) -> Option<f64> {
    let line = text.lines().find(|l| l.starts_with("some "))?;
    line.split_whitespace()
        .find_map(|kv| kv.strip_prefix("avg10="))
        .and_then(|v| v.parse().ok())
}

fn virtualization() -> Option<String> {
    if let Some(v) = command_output("systemd-detect-virt", &[]).filter(|v| v != "none") {
        return Some(v);
    }
    let version = std::fs::read_to_string("/proc/version")
        .unwrap_or_default()
        .to_ascii_lowercase();
    if version.contains("microsoft") {
        return Some("wsl".into());
    }
    if Path::new("/.dockerenv").exists() {
        return Some("docker".into());
    }
    if Path::new("/run/.containerenv").exists() {
        return Some("podman".into());
    }
    let cpuinfo = std::fs::read_to_string("/proc/cpuinfo").unwrap_or_default();
    parse_cpuinfo(&cpuinfo).hypervisor.then(|| "vm".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_probe_fixtures() {
        let intel = parse_cpuinfo(include_str!("../../tests/fixtures/probe/cpuinfo-intel.txt"));
        assert_eq!(
            intel.brand.as_deref(),
            Some("13th Gen Intel(R) Core(TM) i7-1360P")
        );
        assert_eq!(
            (intel.vendor.as_deref(), intel.logical, intel.hypervisor),
            (Some("Intel"), 2, false)
        );

        let amd = parse_cpuinfo(include_str!(
            "../../tests/fixtures/probe/cpuinfo-amd-vm.txt"
        ));
        assert_eq!(
            amd.brand.as_deref(),
            Some("AMD EPYC 7763 64-Core Processor")
        );
        assert!(amd.hypervisor);

        let arm = parse_cpuinfo(include_str!("../../tests/fixtures/probe/cpuinfo-arm.txt"));
        assert_eq!((arm.brand, arm.logical), (None, 2));
        let lscpu = lscpu_model_name(include_str!("../../tests/fixtures/probe/lscpu-arm.txt"));
        assert_eq!(lscpu.as_deref(), Some("Neoverse-N1"));

        assert_eq!(
            parse_os_release(include_str!(
                "../../tests/fixtures/probe/os-release-ubuntu.txt"
            ))
            .as_deref(),
            Some("Ubuntu 24.04.1 LTS")
        );
        assert_eq!(
            parse_psi_some_avg10(include_str!("../../tests/fixtures/probe/psi-memory.txt")),
            Some(1.25)
        );
        assert_eq!(parse_cpu_list("0-3,8,10-11\n"), vec![0, 1, 2, 3, 8, 10, 11]);

        let mounts = include_str!("../../tests/fixtures/probe/mountinfo.txt");
        let m = mount_for(mounts, Path::new("/var/tmp/benchpress-1000/x")).expect("root mount");
        assert_eq!(
            (m.major_minor.as_str(), m.fstype.as_str()),
            ("259:2", "ext4")
        );
        let m = mount_for(mounts, Path::new("/tmp/x")).expect("tmp mount");
        assert_eq!(m.fstype, "tmpfs");
        let m = mount_for(mounts, Path::new("/home/me/My Files")).expect("home mount");
        assert_eq!(
            (m.mount_point.as_path(), m.fstype.as_str()),
            (Path::new("/home"), "btrfs")
        );

        let s = |t: &str, o: Option<&str>, st: Option<&str>| {
            (t.to_owned(), o.map(str::to_owned), st.map(str::to_owned))
        };
        let cases = [
            (vec![], Some(PowerSource::Ac)),
            (
                vec![
                    s("Mains", Some("1"), None),
                    s("Battery", None, Some("Charging")),
                ],
                Some(PowerSource::Ac),
            ),
            (
                vec![
                    s("Mains", Some("0"), None),
                    s("Battery", None, Some("Discharging")),
                ],
                Some(PowerSource::Battery),
            ),
            (
                vec![s("Battery", None, Some("Discharging"))],
                Some(PowerSource::Battery),
            ),
            (vec![s("Mains", Some("0"), None)], None),
        ];
        for (supplies, expected) in cases {
            assert_eq!(power_source(&supplies), expected, "{supplies:?}");
        }
        assert_eq!(
            (
                form_factor(10),
                form_factor(3),
                form_factor(17),
                form_factor(1)
            ),
            (Some("laptop"), Some("desktop"), Some("server"), None)
        );
    }
}
