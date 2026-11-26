//! Ctrl+C, SIGTERM and SIGHUP: kill the running cargo process group, then unwind to cleanup.

use std::sync::atomic::{AtomicBool, AtomicI32, Ordering};

static INTERRUPTED: AtomicBool = AtomicBool::new(false);
static CHILD_PGID: AtomicI32 = AtomicI32::new(0);
static TERMINATE: AtomicBool = AtomicBool::new(false);
static RUN_ACTIVE: AtomicBool = AtomicBool::new(false);

/// Error returned when the user pressed Ctrl+C.
#[derive(Debug, thiserror::Error)]
#[error("interrupted")]
pub struct Interrupted;

/// Install the handlers once at startup.
pub fn install() {
    let _ = ctrlc::set_handler(|| {
        INTERRUPTED.store(true, Ordering::SeqCst);
        kill_child();
    });
    let saved = nix::sys::termios::tcgetattr(std::io::stdin()).ok();
    if let Ok(mut signals) = signal_hook::iterator::Signals::new([
        signal_hook::consts::SIGTERM,
        signal_hook::consts::SIGHUP,
    ]) {
        std::thread::spawn(move || {
            if signals.forever().next().is_some() {
                TERMINATE.store(true, Ordering::SeqCst);
                INTERRUPTED.store(true, Ordering::SeqCst);
                kill_child();
                if !RUN_ACTIVE.load(Ordering::SeqCst) {
                    if let Some(t) = &saved {
                        let _ = nix::sys::termios::tcsetattr(
                            std::io::stdin(),
                            nix::sys::termios::SetArg::TCSANOW,
                            t,
                        );
                    }
                    let _ = console::Term::stderr().show_cursor();
                    std::process::exit(143);
                }
            }
        });
    }
}

/// Marks a run in progress: termination then unwinds it so its cleanup runs.
pub struct RunActive;

impl RunActive {
    /// Start marking.
    pub fn new() -> RunActive {
        RUN_ACTIVE.store(true, Ordering::SeqCst);
        RunActive
    }
}

impl Drop for RunActive {
    fn drop(&mut self) {
        RUN_ACTIVE.store(false, Ordering::SeqCst);
    }
}

/// Whether SIGTERM or SIGHUP arrived: finish cleanup and exit, don't return to the menu.
pub fn terminating() -> bool {
    TERMINATE.load(Ordering::SeqCst)
}

/// Whether Ctrl+C was pressed.
pub fn is_set() -> bool {
    INTERRUPTED.load(Ordering::SeqCst)
}

/// Clear the flag so the menu can carry on after a canceled action.
pub fn reset() {
    if !terminating() {
        INTERRUPTED.store(false, Ordering::SeqCst);
    }
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
