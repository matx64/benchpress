//! Human formatting shared by the CLI and the website.

/// Duration as `4m 12s`, `38.2s` or `412ms`.
pub fn duration_ms(ms: u64) -> String {
    if ms < 1_000 {
        format!("{ms}ms")
    } else if ms < 60_000 {
        format!("{:.1}s", ms as f64 / 1000.0)
    } else if ms < 3_600_000 {
        let s = (ms + 500) / 1000;
        format!("{}m {:02}s", s / 60, s % 60)
    } else {
        let m = (ms + 30_000) / 60_000;
        format!("{}h {:02}m", m / 60, m % 60)
    }
}

/// Bytes as `14.1 GB` (decimal units, as marketed).
pub fn bytes(b: u64) -> String {
    let b = b as f64;
    if b >= 1e9 {
        format!("{:.1} GB", b / 1e9)
    } else if b >= 1e6 {
        format!("{:.0} MB", b / 1e6)
    } else if b >= 1e3 {
        format!("{:.0} kB", b / 1e3)
    } else {
        format!("{b:.0} B")
    }
}

/// Fraction as a whole percentage, e.g. `0.873` → `87%`.
pub fn percent(f: f64) -> String {
    format!("{:.0}%", f * 100.0)
}
