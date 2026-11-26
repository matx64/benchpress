//! Background sampling of system memory, swap and pressure during a run.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use serde::Serialize;

use crate::probe::Probe;

/// Sampling period.
pub const INTERVAL: Duration = Duration::from_millis(250);

/// One sample.
#[derive(Debug, Clone, Copy, Serialize)]
pub struct Sample {
    /// Milliseconds since the run started.
    pub t_ms: u64,
    /// System memory in use.
    pub mem_used: u64,
    /// Swap in use.
    pub swap_used: u64,
    /// Linux PSI `some avg10`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub psi_some_avg10: Option<f64>,
}

/// Current memory and swap in use.
pub fn snapshot(sys: &mut Option<sysinfo::System>) -> (u64, u64) {
    match sys {
        Some(sys) => {
            sys.refresh_memory();
            (sys.used_memory(), sys.used_swap())
        }
        None => (0, 0),
    }
}

/// A memory reader, `None` when the OS refuses.
pub fn system() -> Option<sysinfo::System> {
    sysinfo::System::new().ok()
}

/// A running sampler.
pub struct Sampler {
    stop: Arc<AtomicBool>,
    handle: JoinHandle<Vec<Sample>>,
}

impl Sampler {
    /// Start sampling on a background thread.
    pub fn start(probe: Arc<dyn Probe + Send + Sync>) -> Sampler {
        let stop = Arc::new(AtomicBool::new(false));
        let flag = stop.clone();
        let handle = std::thread::spawn(move || {
            let mut sys = system();
            let start = Instant::now();
            let mut samples = Vec::new();
            loop {
                let (mem_used, swap_used) = snapshot(&mut sys);
                samples.push(Sample {
                    t_ms: start.elapsed().as_millis() as u64,
                    mem_used,
                    swap_used,
                    psi_some_avg10: probe.memory_pressure(),
                });
                let period = INTERVAL.as_millis() as u64;
                let next =
                    Duration::from_millis(period - (start.elapsed().as_millis() as u64 % period));
                let deadline = Instant::now() + next;
                while Instant::now() < deadline {
                    if flag.load(Ordering::Relaxed) {
                        return samples;
                    }
                    std::thread::sleep(Duration::from_millis(10).min(deadline - Instant::now()));
                }
            }
        });
        Sampler { stop, handle }
    }

    /// Stop and return all samples.
    pub fn finish(self) -> Vec<Sample> {
        self.stop.store(true, Ordering::Relaxed);
        self.handle.join().unwrap_or_default()
    }
}
