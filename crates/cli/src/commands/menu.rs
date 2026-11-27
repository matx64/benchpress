//! `benchpress` with no arguments: a menu that every action returns to.

use std::path::Path;

use anyhow::{Result, bail};

use super::data::{self, Usage};
use crate::output::{self, Role, paint};
use crate::{interrupt, submit};

/// Show the menu until the user quits.
pub fn run() -> Result<()> {
    if !output::is_interactive() {
        bail!("no command given. Try `benchpress run quick` or `benchpress --help`.");
    }
    output::set_menu_mode(true);
    output::line(paint(Role::Strong, "Know what your machine can lift."));
    if crate::paths::ephemeral() {
        output::line(paint(
            Role::Muted,
            "Running without installing. This copy of benchpress is deleted when you quit.",
        ));
    }
    output::line(paint(
        Role::Muted,
        "Esc goes back. Ctrl+C cancels a running benchmark.",
    ));
    let items = [
        "Run a benchmark",
        "Your results",
        "Check this machine",
        "Manage local data",
        "Quit",
    ];
    loop {
        interrupt::reset();
        output::blank();
        let Some(pick) = output::choose("What next?", &items, 0)? else {
            break;
        };
        if interrupt::terminating() {
            return Ok(());
        }
        let res = match pick {
            0 => super::run::pick_and_run(),
            1 => results(),
            2 => super::doctor(),
            3 => manage_data(),
            _ => break,
        };
        if let Err(e) = res {
            super::report_error(&e);
        }
        if interrupt::terminating() {
            return Ok(());
        }
    }
    if interrupt::terminating() {
        return Ok(());
    }
    interrupt::reset();
    data::offer_cleanup()
}

fn results() -> Result<()> {
    loop {
        let all = submit::list_local()?;
        if all.is_empty() {
            output::line("No results yet. Pick Run a benchmark to make one.");
            return Ok(());
        }
        let lines: Vec<String> = all
            .iter()
            .map(|(id, p)| super::result_line(id, p).unwrap_or_else(|| id.clone()))
            .collect();
        let Some(i) = output::choose("Pick a result", &lines, 0)? else {
            return Ok(());
        };
        if interrupt::terminating() {
            return Ok(());
        }
        let (id, path) = &all[i];
        if let Err(e) = result_actions(id, path) {
            super::report_error(&e);
        }
    }
}

fn result_actions(id: &str, path: &Path) -> Result<()> {
    super::results_show(id, false)?;
    let published = submit::load_submission(path).is_some();
    let actions = [
        if published {
            "Unpublish from the website"
        } else {
            "Publish"
        },
        "Delete from this machine",
        "Back",
    ];
    output::blank();
    match output::choose("Do what with it?", &actions, 0)? {
        Some(0) if published => submit::unpublish(path),
        Some(0) => submit::submit(path, true).map(|_| ()),
        Some(1) => {
            if published {
                output::line(
                    "It stays on the website, and this machine can no longer unpublish it.",
                );
            }
            if output::confirm("Delete it from this machine?", false) {
                submit::delete_local(path)?;
                output::line(format!("Deleted {id}."));
            }
            Ok(())
        }
        _ => Ok(()),
    }
}

fn manage_data() -> Result<()> {
    let u = Usage::measure()?;
    u.print();
    output::blank();
    let actions = [
        format!(
            "Remove toolchains, sources, crates and builds ({})",
            benchpress_core::format::bytes(u.downloads())
        ),
        "Remove everything, including results".to_owned(),
        "Back".to_owned(),
    ];
    match output::choose("Free up space?", &actions, 0)? {
        Some(0) => data::remove_downloads().map(|_| ()),
        Some(1) => {
            output::line(
                "Published results stay online, and this machine can no longer unpublish them.",
            );
            if output::confirm("Remove all local data?", false) && data::remove_downloads()? {
                data::remove_results()?;
            }
            Ok(())
        }
        _ => Ok(()),
    }
}
