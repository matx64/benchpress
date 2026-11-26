//! What Benchpress keeps on disk, and removing it.

use std::path::PathBuf;

use anyhow::Result;
use benchpress_core::format::bytes;

use crate::output::{self, Role, paint};
use crate::paths;

/// Bytes used in each place Benchpress writes to.
pub struct Usage {
    pub cache_dir: PathBuf,
    pub cache: u64,
    pub builds_dir: PathBuf,
    pub builds: u64,
    pub results_dir: PathBuf,
    pub results: u64,
}

impl Usage {
    /// Measure now.
    pub fn measure() -> Result<Usage> {
        let builds_dir = paths::work_root();
        let results_dir = paths::results_dir()?;
        Ok(Usage {
            cache_dir: paths::cache_dir()?,
            cache: paths::cache_entries()?
                .iter()
                .map(|p| paths::size_of(p))
                .sum(),
            builds: paths::size_of(&builds_dir),
            builds_dir,
            results: paths::size_of(&results_dir),
            results_dir,
        })
    }

    /// Where the removable downloads live.
    pub fn downloads_at(&self) -> String {
        if self.builds > 0 {
            format!(
                "{} and {}",
                paths::display(&self.cache_dir),
                paths::display(&self.builds_dir)
            )
        } else {
            paths::display(&self.cache_dir)
        }
    }

    /// Everything that can be removed without losing results.
    pub fn downloads(&self) -> u64 {
        self.cache + self.builds
    }

    /// Print one line per location.
    pub fn print(&self) {
        let row = |label: &str, size: u64, dir: &PathBuf| {
            output::field(
                label,
                format!(
                    "{:>9}  {}",
                    bytes(size),
                    paint(Role::Muted, &paths::display(dir))
                ),
            );
        };
        row("Downloads", self.cache, &self.cache_dir);
        row("Builds", self.builds, &self.builds_dir);
        row("Results", self.results, &self.results_dir);
    }
}

/// Remove toolchains, sources, crates and build directories. Results stay.
/// Returns `false` when a run elsewhere holds the lock and nothing was removed.
pub fn remove_downloads() -> Result<bool> {
    let Some(_lock) = paths::lock_runs(true)? else {
        output::line(
            "A benchmark is running in another terminal, so the downloads were left in place.",
        );
        return Ok(false);
    };
    let mut targets = paths::cache_entries()?;
    targets.push(paths::work_root());
    for p in targets {
        if paths::remove(&p)? {
            output::line(paint(
                Role::Muted,
                &format!("Removed {}", paths::display(&p)),
            ));
        }
    }
    let root = paths::cache_dir()?;
    let _ = std::fs::remove_file(root.join("run.lock"));
    let _ = std::fs::remove_dir(root);
    Ok(true)
}

/// Remove local results, including the tokens that unpublish them. Only files Benchpress
/// names are deleted, so a results directory shared with other files survives.
pub fn remove_results() -> Result<()> {
    let dir = paths::results_dir()?;
    let Ok(rd) = std::fs::read_dir(&dir) else {
        return Ok(());
    };
    let mut removed = 0;
    for entry in rd.filter_map(Result::ok) {
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.ends_with(".json") || name.ends_with(".samples.jsonl") {
            paths::remove(&entry.path())?;
            removed += 1;
        }
    }
    let _ = std::fs::remove_dir(&dir);
    if removed > 0 {
        output::line(paint(
            Role::Muted,
            &format!("Removed {removed} files from {}", paths::display(&dir)),
        ));
    }
    Ok(())
}

/// After a run: say how much the downloads take and how to remove them.
pub fn note_downloads() {
    let Ok(u) = Usage::measure() else { return };
    if u.downloads() > 0 {
        output::line(paint(
            Role::Muted,
            &format!(
                "Toolchains, sources and crates use {} in {}. Remove them {}.",
                bytes(u.downloads()),
                u.downloads_at(),
                output::later("clean", "Manage local data")
            ),
        ));
    }
}

/// Before leaving: offer to remove the downloads, and say where results live.
pub fn offer_cleanup() -> Result<()> {
    let u = Usage::measure()?;
    if u.downloads() > 0 {
        output::blank();
        output::line(format!(
            "Toolchains, sources and crates use {} in {}. Keeping them makes the next run faster.",
            bytes(u.downloads()),
            u.downloads_at()
        ));
        if output::confirm("Remove them?", paths::ephemeral()) {
            remove_downloads()?;
        }
    }
    if u.results > 0 {
        output::line(paint(
            Role::Muted,
            &format!(
                "Your results are in {}. They hold the tokens that unpublish them; delete the folder when you no longer need them.",
                paths::display(&u.results_dir)
            ),
        ));
    }
    Ok(())
}
