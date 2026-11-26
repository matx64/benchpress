//! Publishing results to the website.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use benchpress_core::BenchResult;
use benchpress_core::validate::{Rejection, validate_payload_size, validate_result};
use serde::{Deserialize, Serialize};

use crate::output::{self, Role, paint};
use crate::{fetch, paths};

/// Base URL of the website. TODO(decision): final domain.
pub fn server_url() -> String {
    std::env::var("BENCHPRESS_SERVER")
        .unwrap_or_else(|_| "https://benchpress.matx.dev".into())
        .trim_end_matches('/')
        .to_owned()
}

/// Saved next to a local result after upload.
#[derive(Debug, Serialize, Deserialize)]
pub struct Submission {
    /// Public id.
    pub id: String,
    /// Share URL.
    pub url: String,
    /// Secret that lets this machine unpublish the result.
    pub delete_token: String,
}

/// Resolve a local result id or a path to a result file.
pub fn resolve(id_or_path: &str) -> Result<PathBuf> {
    let p = Path::new(id_or_path);
    if p.is_file() {
        return Ok(p.to_path_buf());
    }
    let dir = paths::results_dir()?;
    let exact = dir.join(format!("{id_or_path}.json"));
    if exact.is_file() {
        return Ok(exact);
    }
    let matches: Vec<PathBuf> = list_local()?
        .into_iter()
        .filter(|(id, _)| id.starts_with(id_or_path))
        .map(|(_, p)| p)
        .collect();
    match matches.as_slice() {
        [one] => Ok(one.clone()),
        [] => bail!("no local result `{id_or_path}`. Run `benchpress results` to list them."),
        _ => bail!("`{id_or_path}` matches several results; use the full id."),
    }
}

/// Local results, newest first: `(id, path)`.
pub fn list_local() -> Result<Vec<(String, PathBuf)>> {
    let dir = paths::results_dir()?;
    let Ok(rd) = std::fs::read_dir(&dir) else {
        return Ok(Vec::new());
    };
    let mut v: Vec<(String, PathBuf)> = rd
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| {
            let name = p
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            name.ends_with(".json") && !name.ends_with(".submission.json")
        })
        .filter_map(|p| Some((p.file_stem()?.to_string_lossy().into_owned(), p)))
        .collect();
    v.sort_by(|a, b| b.0.cmp(&a.0));
    Ok(v)
}

fn submission_path(result_path: &Path) -> PathBuf {
    result_path.with_extension("submission.json")
}

/// Delete a local result and the files saved next to it (samples, submission).
pub fn delete_local(result_path: &Path) -> Result<()> {
    let (Some(dir), Some(stem)) = (result_path.parent(), result_path.file_stem()) else {
        return Ok(());
    };
    let prefix = format!("{}.", stem.to_string_lossy());
    for entry in std::fs::read_dir(dir)?.filter_map(Result::ok) {
        if entry.file_name().to_string_lossy().starts_with(&prefix) {
            paths::remove(&entry.path())?;
        }
    }
    Ok(())
}

/// Load a saved submission for a local result.
pub fn load_submission(result_path: &Path) -> Option<Submission> {
    serde_json::from_str(&std::fs::read_to_string(submission_path(result_path)).ok()?).ok()
}

/// Validate, show the payload, confirm, upload. Returns the share URL.
pub fn submit(result_path: &Path, yes: bool) -> Result<Option<String>> {
    let text = std::fs::read_to_string(result_path)
        .with_context(|| format!("reading {}", result_path.display()))?;
    let result: BenchResult =
        serde_json::from_str(&text).context("the result file is not a valid benchpress result")?;
    if let Some(s) = load_submission(result_path) {
        output::line(format!("Already published: {}", paint(Role::You, &s.url)));
        return Ok(Some(s.url));
    }
    let payload = serde_json::to_string(&result)?;
    let registry = benchpress_core::Registry::builtin();
    validate_payload_size(payload.len()).map_err(local_rejection)?;
    validate_result(&result, &registry).map_err(local_rejection)?;

    let server = server_url();
    if !yes {
        output::blank();
        output::line(format!(
            "This exact JSON will be sent to {server}. It contains hardware model details and timings, and no names, paths, serial numbers or addresses."
        ));
        output::line(format!(
            "Published results are public. You can unpublish it later {}.",
            output::later("unpublish <id>", "Your results")
        ));
        output::blank();
        output::page(&serde_json::to_string_pretty(&result)?);
        output::blank();
        if !output::confirm("Publish this result?", false) {
            output::line("Not published. The result stays on this machine.");
            return Ok(None);
        }
    }
    let mut resp = fetch::agent()
        .post(&format!("{server}/api/v1/results"))
        .header("content-type", "application/json")
        .send(payload.as_bytes())
        .with_context(|| format!("connecting to {server}"))?;
    let status = resp.status().as_u16();
    let body = resp.body_mut().read_to_string().unwrap_or_default();
    match status {
        201 => {
            let sub: Submission =
                serde_json::from_str(&body).context("unexpected server response")?;
            std::fs::write(
                submission_path(result_path),
                serde_json::to_string_pretty(&sub)?,
            )?;
            output::line(format!("Published: {}", paint(Role::You, &sub.url)));
            Ok(Some(sub.url))
        }
        409 => bail!("the server already has this exact result."),
        422 => {
            let r: Rejection = serde_json::from_str(&body).unwrap_or(Rejection {
                code: "rejected".into(),
                message: body,
            });
            bail!("the server rejected the result: {r}")
        }
        429 => bail!("too many submissions from this network. Try again in an hour."),
        other => {
            bail!("the server returned HTTP {other}. Try again later; the result is saved locally.")
        }
    }
}

#[allow(clippy::needless_pass_by_value)]
fn local_rejection(r: Rejection) -> anyhow::Error {
    anyhow::anyhow!("this result can't be published: {r}")
}

/// Remove a published result using the saved delete token.
pub fn unpublish(result_path: &Path) -> Result<()> {
    let sub =
        load_submission(result_path).context("this result was not published from this machine")?;
    let server = server_url();
    let resp = fetch::agent()
        .delete(&format!("{server}/api/v1/results/{}", sub.id))
        .header("authorization", &format!("Bearer {}", sub.delete_token))
        .call()
        .with_context(|| format!("connecting to {server}"))?;
    match resp.status().as_u16() {
        204 | 404 => {
            std::fs::remove_file(submission_path(result_path))?;
            output::line(format!("Unpublished {}.", sub.url));
            Ok(())
        }
        other => bail!("the server returned HTTP {other}"),
    }
}
