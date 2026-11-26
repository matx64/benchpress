//! Suite manifests: what to build, with which toolchain, and how.

use serde::{Deserialize, Serialize};

use crate::Error;

/// Schema version of suite manifests understood by this release.
pub const SUITE_SCHEMA_VERSION: u32 = 1;

/// A benchmark suite, parsed from one TOML file in `suites/`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SuiteManifest {
    /// Manifest schema version.
    pub schema_version: u32,
    /// Stable kebab-case identifier.
    pub id: String,
    /// Bumped whenever any field changes.
    pub version: u32,
    /// Size tier.
    pub tier: Tier,
    /// Human name shown in listings.
    pub display_name: String,
    /// One-line description of the project being built.
    pub description: String,
    /// Pinned Rust toolchain.
    pub toolchain: Toolchain,
    /// Pinned project source.
    pub source: Source,
    /// Cargo build options shared by all scenarios.
    #[serde(default)]
    pub build: Build,
    /// Scenarios, run in order.
    #[serde(rename = "scenario")]
    pub scenarios: Vec<Scenario>,
    /// Iteration settings.
    pub runs: Runs,
    /// Machine requirements checked before a run.
    pub requirements: Requirements,
}

/// Suite size tier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Tier {
    /// Small project, about a minute.
    Quick,
    /// Mid-size app, several minutes.
    Standard,
    /// Large workspace, tens of minutes.
    Heavy,
}

impl Tier {
    /// All tiers, light to heavy.
    pub const ALL: [Tier; 3] = [Tier::Quick, Tier::Standard, Tier::Heavy];

    /// Lowercase identifier.
    pub fn as_str(self) -> &'static str {
        match self {
            Tier::Quick => "quick",
            Tier::Standard => "standard",
            Tier::Heavy => "heavy",
        }
    }

    /// Sentence-case label.
    pub fn label(self) -> &'static str {
        match self {
            Tier::Quick => "Quick",
            Tier::Standard => "Standard",
            Tier::Heavy => "Heavy",
        }
    }
}

impl std::str::FromStr for Tier {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "quick" => Ok(Tier::Quick),
            "standard" => Ok(Tier::Standard),
            "heavy" => Ok(Tier::Heavy),
            other => Err(Error::Invalid(format!("unknown tier `{other}`"))),
        }
    }
}

/// Pinned toolchain installed through rustup.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Toolchain {
    /// Exact stable version such as `1.99.1`.
    pub channel: String,
    /// Rustup profile.
    pub profile: String,
}

/// Pinned project source.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Source {
    /// GitHub repository URL.
    pub repo: String,
    /// Full 40-character commit SHA.
    pub commit: String,
    /// SHA-256 of the GitHub tarball for `commit`.
    pub archive_sha256: String,
    /// Optional subdirectory holding the cargo workspace.
    #[serde(default)]
    pub subdir: String,
}

impl Source {
    /// Tarball URL for the pinned commit.
    pub fn archive_url(&self) -> String {
        format!(
            "{}/archive/{}.tar.gz",
            self.repo.trim_end_matches('/'),
            self.commit
        )
    }
}

/// Cargo options shared by every scenario.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Build {
    /// Optional `-p` target.
    #[serde(default)]
    pub package: String,
    /// Features to enable.
    #[serde(default)]
    pub features: Vec<String>,
    /// Pass `--no-default-features`.
    #[serde(default)]
    pub no_default_features: bool,
}

impl Build {
    /// Extra cargo arguments derived from these options.
    pub fn cargo_args(&self) -> Vec<String> {
        let mut args = Vec::new();
        if !self.package.is_empty() {
            args.push("-p".to_owned());
            args.push(self.package.clone());
        }
        if !self.features.is_empty() {
            args.push("--features".to_owned());
            args.push(self.features.join(","));
        }
        if self.no_default_features {
            args.push("--no-default-features".to_owned());
        }
        args
    }
}

/// How a scenario prepares the target directory.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ScenarioKind {
    /// Target directory removed before every run.
    Clean,
    /// A prepared build exists; one file is changed before every run.
    Incremental,
}

/// One measured cargo invocation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Scenario {
    /// Stable identifier within the suite.
    pub id: String,
    /// Preparation semantics.
    pub kind: ScenarioKind,
    /// Cargo arguments.
    pub args: Vec<String>,
    /// File modified before each incremental run, relative to the workspace.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub touch: Option<String>,
    /// Plausibility floor for a single run's wall time.
    #[serde(default = "default_min_wall_ms")]
    pub min_wall_ms: u64,
}

fn default_min_wall_ms() -> u64 {
    100
}

impl Scenario {
    /// Sentence-case label derived from the id, e.g. `clean-release` → `Clean release`.
    pub fn label(&self) -> String {
        scenario_label(&self.id)
    }
}

/// Sentence-case label for a scenario id.
pub fn scenario_label(id: &str) -> String {
    let spaced = id.replace('-', " ");
    let mut chars = spaced.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => String::new(),
    }
}

/// Iteration settings.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Runs {
    /// Measured iterations per scenario.
    pub count: u32,
    /// Run each clean scenario once untimed before measuring.
    #[serde(default)]
    pub warmup: bool,
}

/// Machine requirements.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Requirements {
    /// Free disk needed on the workdir's filesystem.
    pub disk_gb: u32,
    /// Rough duration range on a modern laptop.
    pub estimated_minutes: MinutesRange,
}

/// Inclusive minute range.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MinutesRange {
    /// Lower bound.
    pub min: u32,
    /// Upper bound.
    pub max: u32,
}

impl SuiteManifest {
    /// Parse and validate a manifest from TOML.
    pub fn from_toml(text: &str) -> Result<Self, Error> {
        let manifest: SuiteManifest =
            toml::from_str(text).map_err(|e| Error::Parse(e.to_string()))?;
        manifest.validate()?;
        Ok(manifest)
    }

    /// `id@version`.
    pub fn key(&self) -> String {
        format!("{}@{}", self.id, self.version)
    }

    /// Scenario by id.
    pub fn scenario(&self, id: &str) -> Option<&Scenario> {
        self.scenarios.iter().find(|s| s.id == id)
    }

    /// Project name derived from the repository URL.
    pub fn project(&self) -> &str {
        self.source
            .repo
            .trim_end_matches('/')
            .rsplit('/')
            .next()
            .unwrap_or(&self.source.repo)
    }

    /// Check structural rules that serde cannot express.
    pub fn validate(&self) -> Result<(), Error> {
        let invalid = |m: String| Err(Error::Invalid(format!("suite `{}`: {m}", self.id)));
        if self.schema_version != SUITE_SCHEMA_VERSION {
            return invalid(format!(
                "unsupported schema_version {}",
                self.schema_version
            ));
        }
        if self.id.is_empty()
            || !self
                .id
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
        {
            return invalid("id must be kebab-case".into());
        }
        if self.version == 0 {
            return invalid("version must be at least 1".into());
        }
        if !is_exact_version(&self.toolchain.channel) {
            return invalid(format!(
                "toolchain channel `{}` must be an exact version like 1.99.1",
                self.toolchain.channel
            ));
        }
        if !is_hex(&self.source.commit, 40) {
            return invalid("source.commit must be a full 40-character hex sha".into());
        }
        if !is_hex(&self.source.archive_sha256, 64) {
            return invalid("source.archive_sha256 must be 64 hex characters".into());
        }
        if !self.source.repo.starts_with("https://github.com/") {
            return invalid("source.repo must be a https://github.com/ URL".into());
        }
        if self.source.subdir.starts_with('/') || self.source.subdir.contains("..") {
            return invalid("source.subdir must be a relative path".into());
        }
        if self.scenarios.is_empty() {
            return invalid("at least one scenario is required".into());
        }
        if self.runs.count == 0 {
            return invalid("runs.count must be at least 1".into());
        }
        let mut seen = std::collections::HashSet::new();
        for s in &self.scenarios {
            if !seen.insert(s.id.as_str()) {
                return invalid(format!("duplicate scenario id `{}`", s.id));
            }
            if s.args.is_empty() {
                return invalid(format!("scenario `{}` has no args", s.id));
            }
            match (s.kind, &s.touch) {
                (ScenarioKind::Incremental, None) => {
                    return invalid(format!("incremental scenario `{}` needs `touch`", s.id));
                }
                (ScenarioKind::Clean, Some(_)) => {
                    return invalid(format!("clean scenario `{}` must not set `touch`", s.id));
                }
                (_, Some(t)) if t.starts_with('/') || t.contains("..") => {
                    return invalid(format!("scenario `{}` touch must be relative", s.id));
                }
                _ => {}
            }
        }
        Ok(())
    }
}

fn is_exact_version(s: &str) -> bool {
    let parts: Vec<&str> = s.split('.').collect();
    parts.len() == 3
        && parts
            .iter()
            .all(|p| !p.is_empty() && p.chars().all(|c| c.is_ascii_digit()))
}

fn is_hex(s: &str, len: usize) -> bool {
    s.len() == len
        && s.chars()
            .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase())
}

#[cfg(test)]
mod tests {
    use super::*;

    const BASE: &str = include_str!("../../../suites/rust-quick@1.toml");

    #[test]
    fn manifest_rules() {
        let cases: &[(&str, &str, &str)] = &[
            (
                "channel stable",
                "channel = \"1.99.1\"",
                "channel = \"stable\"",
            ),
            (
                "short commit",
                "commit = \"669cb4c755db20b26c3af3d923c042ae224c7496\"",
                "commit = \"669cb4c\"",
            ),
            ("missing touch", "touch = \"crates/core/main.rs\"\n", ""),
            (
                "duplicate id",
                "id = \"incremental-core\"",
                "id = \"incremental-leaf\"",
            ),
            (
                "non-github",
                "https://github.com/BurntSushi",
                "https://example.com/BurntSushi",
            ),
        ];
        assert!(SuiteManifest::from_toml(BASE).is_ok());
        for (name, from, to) in cases {
            assert!(BASE.contains(from), "{name}: fixture text missing");
            let text = BASE.replacen(from, to, 1);
            assert!(
                SuiteManifest::from_toml(&text).is_err(),
                "{name} should be rejected"
            );
        }
    }
}
