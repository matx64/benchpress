//! Calm progress lines: one per phase, one per measured run.

use std::time::{Duration, Instant};

use indicatif::{ProgressBar, ProgressDrawTarget, ProgressStyle};

use crate::output::{self, Role, paint};

const TICKS: &str = "⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏ ";

fn spinner(prefix: String) -> ProgressBar {
    let pb = ProgressBar::new_spinner();
    if output::is_fancy() {
        pb.set_draw_target(ProgressDrawTarget::stderr());
        pb.set_style(
            ProgressStyle::with_template("  {prefix} {spinner} {msg}")
                .expect("template")
                .tick_chars(TICKS),
        );
        pb.enable_steady_tick(Duration::from_millis(100));
    } else {
        pb.set_draw_target(ProgressDrawTarget::hidden());
    }
    pb.set_prefix(prefix);
    pb
}

fn label(name: &str) -> String {
    paint(Role::Muted, &format!("{name:<18}"))
}

/// Run an untimed phase. `f` gets a callback to update the status text.
pub fn phase<T>(
    name: &str,
    detail: &str,
    f: impl FnOnce(&mut dyn FnMut(&str)) -> anyhow::Result<T>,
) -> anyhow::Result<T> {
    let pb = spinner(label(name));
    pb.set_message(detail.to_owned());
    let start = Instant::now();
    let mut set = |s: &str| pb.set_message(format!("{detail} · {s}"));
    let res = f(&mut set);
    pb.finish_and_clear();
    match &res {
        Ok(_) => output::line(format!(
            "  {} {} {}",
            label(name),
            detail,
            paint(
                Role::Muted,
                &benchpress_core::format::duration_ms(start.elapsed().as_millis() as u64)
            )
        )),
        Err(_) => output::line(format!(
            "  {} {} {}",
            label(name),
            detail,
            paint(Role::Error, "failed")
        )),
    }
    res
}

/// Run one measured iteration, showing elapsed time, then the final time.
pub fn timed<T>(
    name: &str,
    i: u32,
    n: u32,
    f: impl FnOnce() -> anyhow::Result<(benchpress_core::result::Run, T)>,
) -> anyhow::Result<(benchpress_core::result::Run, T)> {
    let prefix = format!(
        "{} {}",
        label(name),
        paint(Role::Muted, &format!("run {i}/{n}"))
    );
    let pb = spinner(prefix.clone());
    let start = Instant::now();
    let ticker = pb.clone();
    let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let stop2 = stop.clone();
    let t = std::thread::spawn(move || {
        while !stop2.load(std::sync::atomic::Ordering::Relaxed) {
            ticker.set_message(benchpress_core::format::duration_ms(
                start.elapsed().as_millis() as u64,
            ));
            std::thread::sleep(Duration::from_millis(100));
        }
    });
    let res = f();
    stop.store(true, std::sync::atomic::Ordering::Relaxed);
    let _ = t.join();
    pb.finish_and_clear();
    match &res {
        Ok((run, _)) => output::line(format!(
            "  {prefix}  {}  {}",
            paint(
                Role::Strong,
                &format!("{:>8}", benchpress_core::format::duration_ms(run.wall_ms))
            ),
            paint(
                Role::Muted,
                &format!("cpu {}", benchpress_core::format::percent(run.cpu_util_avg))
            )
        )),
        Err(_) => output::line(format!("  {prefix}  {}", paint(Role::Error, "failed"))),
    }
    res
}

/// Show a cooldown countdown.
pub fn waiting<T, E>(secs: u32, f: impl FnOnce() -> Result<T, E>) -> Result<T, E> {
    let pb = spinner(label("Cooling down"));
    pb.set_message(format!("{secs}s"));
    let res = f();
    pb.finish_and_clear();
    res
}
