//! Ctrl+C handling: kill the running cargo process group, then unwind to cleanup.

use std::sync::atomic::{AtomicBool, AtomicI32, Ordering};

static INTERRUPTED: AtomicBool = AtomicBool::new(false);
static CHILD_PGID: AtomicI32 = AtomicI32::new(0);

/// Error returned when the user pressed Ctrl+C.
#[derive(Debug, thiserror::Error)]
#[error("interrupted")]
pub struct Interrupted;

/// Install the handler once at startup.
pub fn install() {
    let _ = ctrlc::set_handler(|| {
        INTERRUPTED.store(true, Ordering::SeqCst);
        kill_child();
    });
}

/// Whether Ctrl+C was pressed.
pub fn is_set() -> bool {
    INTERRUPTED.load(Ordering::SeqCst)
}

/// Fail fast after Ctrl+C.
pub fn check() -> Result<(), Interrupted> {
    if is_set() { Err(Interrupted) } else { Ok(()) }
}

/// Register the process group of a spawned child (0 to clear).
pub fn set_child(pgid: i32) {
    CHILD_PGID.store(pgid, Ordering::SeqCst);
    if pgid != 0 && is_set() {
        kill_child();
    }
}

fn kill_child() {
    let pgid = CHILD_PGID.load(Ordering::SeqCst);
    if pgid > 0 {
        let _ = nix::sys::signal::killpg(
            nix::unistd::Pid::from_raw(pgid),
            nix::sys::signal::Signal::SIGKILL,
        );
    }
}

/// Sleep in short slices so Ctrl+C stays responsive.
pub fn sleep(d: std::time::Duration) -> Result<(), Interrupted> {
    let end = std::time::Instant::now() + d;
    while std::time::Instant::now() < end {
        check()?;
        std::thread::sleep(
            (end - std::time::Instant::now()).min(std::time::Duration::from_millis(100)),
        );
    }
    check()
}
