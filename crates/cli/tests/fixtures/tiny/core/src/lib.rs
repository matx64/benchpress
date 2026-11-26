pub fn checksum(n: u64) -> u64 {
    (0..n).map(tiny_util::mix).fold(0, u64::wrapping_add)
}
