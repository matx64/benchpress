pub fn mix(x: u64) -> u64 {
    x.wrapping_mul(0x9E37_79B9_7F4A_7C15).rotate_left(17)
}
