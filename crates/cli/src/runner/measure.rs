//! Spawning cargo in its own process group and measuring it.

use std::collections::VecDeque;
use std::io::{BufRead, BufReader};
use std::os::unix::process::CommandExt;
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::time::Instant;

use anyhow::{Context, Result};
use benchpress_core::result::{MemPressure, Run};
use nix::sys::resource::{UsageWho, getrusage};

use crate::interrupt::{self, Interrupted};
use crate::probe::Probe;
use crate::sampler::{self, Sample, Sampler};

/// Lines of stderr kept for failure reports.
pub const TAIL_LINES: usize = 50;

/// A command that exited non-zero.
#[derive(Debug, thiserror::Error)]
#[error("`{command}` failed ({status})")]
pub struct CommandFailed {
    /// What was run.
    pub command: String,
    /// Exit status description.
    pub status: String,
    /// Last lines of stderr.
    pub stderr_tail: Vec<String>,
}

/// Outcome of one spawned command.
pub struct Spawned {
    /// Wall time.
    pub wall_ms: u64,
    /// Whether it exited successfully.
    pub ok: bool,
    /// Exit status description.
    pub status: String,
    /// Last lines of stderr.
    pub stderr_tail: Vec<String>,
}

fn cpu_ms() -> (u64, u64) {
    match getrusage(UsageWho::RUSAGE_CHILDREN) {
        Ok(u) => {
            let ms = |t: nix::sys::time::TimeVal| {
                (t.tv_sec() as u64) * 1000 + (t.tv_usec() as u64) / 1000
            };
            (ms(u.user_time()), ms(u.system_time()))
        }
        Err(_) => (0, 0),
    }
}

/// Spawn `cmd` in a new process group, wait, and keep the stderr tail.
pub fn spawn(mut cmd: Command) -> Result<Spawned> {
    interrupt::check()?;
    let description = describe(&cmd);
    cmd.stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .process_group(0);
    let start = Instant::now();
    let mut child = cmd
        .spawn()
        .with_context(|| format!("starting {description}"))?;
    interrupt::set_child(child.id() as i32);
    let stderr = child.stderr.take().context("stderr pipe")?;
    let reader = std::thread::spawn(move || {
        let mut tail = VecDeque::with_capacity(TAIL_LINES);
        for line in BufReader::new(stderr).lines().map_while(Result::ok) {
            if tail.len() == TAIL_LINES {
                tail.pop_front();
            }
            tail.push_back(line);
        }
        Vec::from(tail)
    });
    let status = child.wait().context("waiting for cargo");
    let wall_ms = start.elapsed().as_millis() as u64;
    interrupt::set_child(0);
    let stderr_tail = reader.join().unwrap_or_default();
    let status = status?;
    if interrupt::is_set() {
        return Err(Interrupted.into());
    }
    Ok(Spawned {
        wall_ms,
        ok: status.success(),
        status: status.to_string(),
        stderr_tail,
    })
}

/// Run an untimed preparation command; fail with its stderr tail on error.
pub fn run_untimed(cmd: Command) -> Result<()> {
    let command = describe(&cmd);
    let s = spawn(cmd)?;
    if !s.ok {
        return Err(CommandFailed {
            command,
            status: s.status,
            stderr_tail: s.stderr_tail,
        }
        .into());
    }
    Ok(())
}

/// Measure one run of `cmd`.
pub fn measure(
    cmd: Command,
    probe: &Arc<dyn Probe + Send + Sync>,
    logical_cores: u32,
) -> Result<(Run, Vec<Sample>)> {
    let command = describe(&cmd);
    let thermal_start = probe.thermal();
    let mut sys = sampler::system();
    let (baseline_mem, baseline_swap) = sampler::snapshot(&mut sys);
    let sampler = Sampler::start(probe.clone());
    let (u0, s0) = cpu_ms();
    let spawned = spawn(cmd);
    let (u1, s1) = cpu_ms();
    let samples = sampler.finish();
    let spawned = spawned?;
    if !spawned.ok {
        return Err(CommandFailed {
            command,
            status: spawned.status,
            stderr_tail: spawned.stderr_tail,
        }
        .into());
    }
    let (user_ms, sys_ms) = (u1.saturating_sub(u0), s1.saturating_sub(s0));
    let wall_ms = spawned.wall_ms.max(1);
    let peak_mem = samples
        .iter()
        .map(|s| s.mem_used)
        .max()
        .unwrap_or(baseline_mem)
        .max(baseline_mem);
    let peak_swap = samples
        .iter()
        .map(|s| s.swap_used)
        .max()
        .unwrap_or(baseline_swap)
        .max(baseline_swap);
    let psi = samples
        .iter()
        .filter_map(|s| s.psi_some_avg10)
        .fold(None, |m: Option<f64>, v| Some(m.map_or(v, |m| m.max(v))));
    let util = (user_ms + sys_ms) as f64 / (wall_ms as f64 * f64::from(logical_cores.max(1)));
    let run = Run {
        wall_ms,
        user_ms,
        sys_ms,
        baseline_mem_used_bytes: baseline_mem,
        peak_mem_used_bytes: peak_mem,
        baseline_swap_used_bytes: baseline_swap,
        peak_swap_used_bytes: peak_swap,
        mem_pressure: MemPressure {
            some_avg10_max: psi,
        },
        cpu_util_avg: (util * 1000.0).round() / 1000.0,
        thermal_start,
        thermal_end: probe.thermal(),
        exit_ok: true,
    };
    Ok((run, samples))
}

fn describe(cmd: &Command) -> String {
    let prog = std::path::Path::new(cmd.get_program())
        .file_name()
        .map_or_else(
            || cmd.get_program().to_string_lossy().into_owned(),
            |n| n.to_string_lossy().into_owned(),
        );
    let args: Vec<String> = cmd
        .get_args()
        .map(|a| a.to_string_lossy().into_owned())
        .collect();
    format!("{prog} {}", args.join(" "))
}
