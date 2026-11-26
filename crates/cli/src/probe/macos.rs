//! macOS (Apple Silicon) probes: `sysctl`, `pmset`, `system_profiler`, `diskutil` and `NSProcessInfo`.

use std::path::Path;

use benchpress_core::hardware::cpu_vendor;
use benchpress_core::result::{
    Cpu, Environment, Hardware, MacEnv, Machine, Memory, Os, Power, PowerSource, Storage, Thermal,
};

use super::{Probe, command_output};

/// Probe backed by system tools and Foundation.
pub struct MacProbe;

fn sysctl(key: &str) -> Option<String> {
    command_output("/usr/sbin/sysctl", &["-n", key]).filter(|s| !s.is_empty())
}

fn sysctl_u64(key: &str) -> Option<u64> {
    sysctl(key)?.parse().ok()
}

impl Probe for MacProbe {
    fn hardware(&self, workdir: &Path) -> Hardware {
        let brand = sysctl("machdep.cpu.brand_string").unwrap_or_else(|| "Unknown CPU".into());
        let logical = sysctl_u64("hw.logicalcpu").map_or_else(
            || std::thread::available_parallelism().map_or(1, |n| n.get() as u32),
            |n| n as u32,
        );
        let cpu = Cpu {
            vendor: cpu_vendor(&brand).map(str::to_owned),
            brand,
            arch: super::arch(),
            physical_cores: sysctl_u64("hw.physicalcpu").map(|n| n as u32),
            logical_cores: logical,
            performance_cores: sysctl_u64("hw.perflevel0.physicalcpu").map(|n| n as u32),
            efficiency_cores: sysctl_u64("hw.perflevel1.physicalcpu").map(|n| n as u32),
            base_mhz: None,
            max_mhz: None,
        };
        let memory = Memory {
            total_bytes: sysctl_u64("hw.memsize").unwrap_or(0),
            swap_total_bytes: sysctl("vm.swapusage").and_then(|s| parse_swap_total(&s)),
            unified: true,
        };
        let profile = command_output(
            "/usr/sbin/system_profiler",
            &["SPHardwareDataType", "-json"],
        )
        .and_then(|s| parse_hardware_profile(&s));
        let model_id = sysctl("hw.model");
        let family = profile.as_ref().map(|p| p.0.clone());
        let machine = Machine {
            vendor: Some("Apple".into()),
            form_factor: family.as_deref().map(mac_form_factor).map(str::to_owned),
            model_id,
            family,
        };
        Hardware {
            cpu,
            memory,
            storage: storage(workdir),
            machine,
        }
    }

    fn environment(&self) -> Environment {
        Environment {
            os: Os {
                name: "macOS".into(),
                version: command_output("/usr/bin/sw_vers", &["-productVersion"]),
                kernel: nix::sys::utsname::uname()
                    .ok()
                    .map(|u| u.release().to_string_lossy().into_owned()),
            },
            power: self.power(),
            thermal_at_start: self.thermal(),
            virtualization: (sysctl("kern.hv_vmm_present").as_deref() == Some("1"))
                .then(|| "vm".to_owned()),
            mac: Some(MacEnv {
                spotlight_excluded: false,
                developer_tools_terminal: "unknown".into(),
            }),
            linux: None,
            overrides_cleared: super::cargo_overrides(),
        }
    }

    fn power(&self) -> Power {
        Power {
            source: command_output("/usr/bin/pmset", &["-g", "batt"])
                .and_then(|s| parse_pmset_batt(&s)),
            low_power_mode: Some(foundation::low_power_mode()),
            profile: None,
        }
    }

    fn thermal(&self) -> Option<Thermal> {
        Some(foundation::thermal_state())
    }
}

mod foundation {
    use benchpress_core::result::Thermal;
    use objc2_foundation::{NSProcessInfo, NSProcessInfoThermalState};

    pub fn low_power_mode() -> bool {
        NSProcessInfo::processInfo().isLowPowerModeEnabled()
    }

    pub fn thermal_state() -> Thermal {
        match NSProcessInfo::processInfo().thermalState() {
            NSProcessInfoThermalState::Fair => Thermal::Fair,
            NSProcessInfoThermalState::Serious => Thermal::Serious,
            NSProcessInfoThermalState::Critical => Thermal::Critical,
            _ => Thermal::Nominal,
        }
    }
}

fn mac_form_factor(family: &str) -> &'static str {
    if family.contains("MacBook") {
        "laptop"
    } else {
        "desktop"
    }
}

fn storage(workdir: &Path) -> Storage {
    let path = workdir
        .ancestors()
        .find(|p| p.exists())
        .unwrap_or(Path::new("/"));
    let filesystem = nix::sys::statfs::statfs(path)
        .ok()
        .map(|s| s.filesystem_type_name().to_owned());
    let mount = command_output("/bin/df", &["-P", &path.to_string_lossy()])
        .and_then(|s| parse_df_mount(&s));
    let info = mount
        .and_then(|m| command_output("/usr/sbin/diskutil", &["info", "-plist", &m]))
        .map(|s| parse_diskutil_plist(&s))
        .unwrap_or_default();
    Storage { filesystem, ..info }
}

/// Mount point from `df -P` output.
pub(crate) fn parse_df_mount(text: &str) -> Option<String> {
    let line = text.lines().nth(1)?;
    let idx = line.find(" /")?;
    let rest = &line[idx..];
    let mp = rest.split_once('%').map_or(rest, |(_, after)| after);
    Some(mp.trim().to_owned()).filter(|m| m.starts_with('/'))
}

/// Swap total from `vm.swapusage`, e.g. `total = 2048.00M  used = 1048.25M  free = 999.75M`.
pub(crate) fn parse_swap_total(text: &str) -> Option<u64> {
    let v = text.split("total = ").nth(1)?.split_whitespace().next()?;
    let (num, unit) = v.split_at(v.len() - 1);
    let num: f64 = num.parse().ok()?;
    let mult = match unit {
        "K" => 1024.0,
        "M" => 1024.0 * 1024.0,
        "G" => 1024.0 * 1024.0 * 1024.0,
        _ => return None,
    };
    Some((num * mult) as u64)
}

/// `(machine_name, machine_model)` from `system_profiler SPHardwareDataType -json`.
/// Serial numbers and UUIDs in the same output are ignored.
pub(crate) fn parse_hardware_profile(json: &str) -> Option<(String, Option<String>)> {
    let v: serde_json::Value = serde_json::from_str(json).ok()?;
    let item = v.get("SPHardwareDataType")?.get(0)?;
    let name = item.get("machine_name")?.as_str()?.to_owned();
    let model = item
        .get("machine_model")
        .and_then(|m| m.as_str())
        .map(str::to_owned);
    Some((name, model))
}

/// Power source from `pmset -g batt`.
pub(crate) fn parse_pmset_batt(text: &str) -> Option<PowerSource> {
    if text.contains("'AC Power'") {
        Some(PowerSource::Ac)
    } else if text.contains("'Battery Power'") {
        Some(PowerSource::Battery)
    } else {
        None
    }
}

/// Media name, solid-state flag and bus protocol from `diskutil info -plist`.
pub(crate) fn parse_diskutil_plist(xml: &str) -> Storage {
    let value_after = |key: &str| -> Option<String> {
        let start = xml.find(&format!("<key>{key}</key>"))? + key.len() + 11;
        let rest = xml[start..].trim_start();
        if rest.starts_with("<true/>") {
            return Some("true".into());
        }
        if rest.starts_with("<false/>") {
            return Some("false".into());
        }
        let open = rest.find('>')? + 1;
        let close = rest.find("</")?;
        Some(rest[open..close].trim().to_owned())
    };
    let protocol = value_after("BusProtocol");
    let solid = value_after("SolidState");
    let kind = match (protocol.as_deref(), solid.as_deref()) {
        (Some("Apple Fabric" | "PCI-Express" | "PCI"), _) => Some("nvme".to_owned()),
        (Some("Disk Image"), _) => Some("virtual".to_owned()),
        (_, Some("true")) => Some("ssd".to_owned()),
        (_, Some("false")) => Some("hdd".to_owned()),
        _ => None,
    };
    Storage {
        kind,
        model: value_after("MediaName").filter(|m| !m.is_empty()),
        filesystem: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_probe_fixtures() {
        let profile = parse_hardware_profile(include_str!(
            "../../tests/fixtures/probe/system-profiler.json"
        ));
        assert_eq!(
            profile,
            Some(("MacBook Pro".into(), Some("Mac15,6".into())))
        );
        assert_eq!(
            parse_pmset_batt(include_str!("../../tests/fixtures/probe/pmset-ac.txt")),
            Some(PowerSource::Ac)
        );
        assert_eq!(
            parse_pmset_batt(include_str!("../../tests/fixtures/probe/pmset-battery.txt")),
            Some(PowerSource::Battery)
        );
        let disk = parse_diskutil_plist(include_str!("../../tests/fixtures/probe/diskutil.plist"));
        assert_eq!(
            (disk.kind.as_deref(), disk.model.as_deref()),
            (Some("nvme"), Some("APPLE SSD AP0512Z"))
        );
        assert_eq!(
            parse_swap_total("total = 2048.00M  used = 1048.25M  free = 999.75M  (encrypted)"),
            Some(2_147_483_648)
        );
        let df = "Filesystem     512-blocks      Used Available Capacity  Mounted on\n/dev/disk3s5    1942700360 912345678 1030354682    47%    /System/Volumes/Data\n";
        assert_eq!(parse_df_mount(df).as_deref(), Some("/System/Volumes/Data"));
    }
}
