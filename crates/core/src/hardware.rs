//! Normalization helpers used to group results.

const MARKETED_GB: &[u32] = &[
    2, 4, 6, 8, 12, 16, 18, 24, 32, 36, 48, 64, 96, 128, 192, 256, 384, 512, 768, 1024,
];

/// Marketed RAM size in GB for a reported total, e.g. 15.4 GiB → 16.
pub fn memory_bucket(total_bytes: u64) -> u32 {
    let gib = total_bytes as f64 / f64::from(1u32 << 30);
    MARKETED_GB
        .iter()
        .copied()
        .find(|&s| gib <= f64::from(s) * 1.01)
        .unwrap_or_else(|| ((gib / 256.0).ceil() as u32) * 256)
}

/// Normalized CPU family from a brand string, e.g.
/// `Intel(R) Core(TM) i7-1185G7 @ 3.00GHz` → `Intel Core i7-1185G7`.
pub fn cpu_family(brand: &str) -> String {
    let mut s = brand.to_owned();
    for noise in ["(R)", "(r)", "(TM)", "(tm)", "®", "™"] {
        s = s.replace(noise, " ");
    }
    if let Some(i) = s.find('@') {
        s.truncate(i);
    }
    let mut words: Vec<&str> = s.split_whitespace().collect();
    if let Some(i) = words.iter().position(|w| matches!(*w, "w/" | "with")) {
        words.truncate(i);
    }
    if words.len() >= 2 && words[1] == "Gen" && words[0].ends_with("th") {
        words.drain(..2);
    }
    while let Some(last) = words.last() {
        let l = last.to_ascii_lowercase();
        let is_core_count =
            l.ends_with("-core") && l[..l.len() - 5].chars().all(|c| c.is_ascii_digit());
        if l == "processor" || l == "cpu" || is_core_count {
            words.pop();
        } else {
            break;
        }
    }
    words.retain(|w| *w != "CPU");
    if words.is_empty() {
        brand.trim().to_owned()
    } else {
        words.join(" ")
    }
}

/// Vendor guessed from a brand string, when it names one.
pub fn cpu_vendor(brand: &str) -> Option<&'static str> {
    let b = brand.to_ascii_lowercase();
    if b.starts_with("apple") {
        Some("Apple")
    } else if b.contains("amd") {
        Some("AMD")
    } else if b.contains("intel") {
        Some("Intel")
    } else if b.contains("ampere") {
        Some("Ampere")
    } else if b.contains("qualcomm") || b.contains("snapdragon") {
        Some("Qualcomm")
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cpu_family_normalizes_real_brand_strings() {
        let cases = [
            ("Apple M3 Pro", "Apple M3 Pro"),
            ("Apple M4", "Apple M4"),
            (
                "AMD Ryzen 7 7840U w/ Radeon 780M Graphics",
                "AMD Ryzen 7 7840U",
            ),
            ("AMD Ryzen 9 7950X 16-Core Processor", "AMD Ryzen 9 7950X"),
            (
                "AMD Ryzen 7 PRO 7840U with Radeon Graphics",
                "AMD Ryzen 7 PRO 7840U",
            ),
            ("AMD EPYC 7763 64-Core Processor", "AMD EPYC 7763"),
            (
                "Intel(R) Core(TM) i7-1185G7 @ 3.00GHz",
                "Intel Core i7-1185G7",
            ),
            (
                "11th Gen Intel(R) Core(TM) i7-1165G7 @ 2.80GHz",
                "Intel Core i7-1165G7",
            ),
            (
                "13th Gen Intel(R) Core(TM) i9-13900K",
                "Intel Core i9-13900K",
            ),
            ("Intel(R) Core(TM) Ultra 7 155H", "Intel Core Ultra 7 155H"),
            (
                "Intel(R) Xeon(R) CPU E5-2680 v4 @ 2.40GHz",
                "Intel Xeon E5-2680 v4",
            ),
            ("  Neoverse-N1  ", "Neoverse-N1"),
        ];
        for (brand, family) in cases {
            assert_eq!(cpu_family(brand), family, "{brand}");
        }
    }

    #[test]
    fn memory_bucket_rounds_to_marketed_sizes() {
        let gib = |g: f64| (g * f64::from(1u32 << 30)) as u64;
        let cases = [
            (7.6, 8),
            (15.4, 16),
            (16.0, 16),
            (18.0, 18),
            (23.4, 24),
            (30.5, 32),
            (36.0, 36),
            (62.7, 64),
            (125.0, 128),
            (1500.0, 1536),
        ];
        for (g, bucket) in cases {
            assert_eq!(memory_bucket(gib(g)), bucket, "{g} GiB");
        }
    }
}
