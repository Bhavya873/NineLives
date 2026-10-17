//! FNV-1a, a tiny 64-bit hash. The golden trace fingerprints world states with it and
//! the observation layout (`obs`) fingerprints its schema, so both use one copy that is
//! checked against the published test vector below.

/// FNV-1a 64-bit starting value (the standard constant).
pub const FNV_OFFSET_BASIS: u64 = 0xcbf2_9ce4_8422_2325;
/// FNV-1a 64-bit multiplier (the standard constant).
const FNV_PRIME: u64 = 0x0000_0100_0000_01b3;

/// One round of the FNV-1a hash: for each byte, xor it in, then multiply.
pub fn fnv1a_add_bytes(mut hash: u64, bytes: &[u8]) -> u64 {
    for byte in bytes {
        hash ^= *byte as u64;
        hash = hash.wrapping_mul(FNV_PRIME);
    }
    hash
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fnv1a_of_the_letter_a_matches_the_published_value() {
        // Known FNV-1a 64-bit test vector: "a" -> af63dc4c8601ec8c.
        assert_eq!(
            fnv1a_add_bytes(FNV_OFFSET_BASIS, b"a"),
            0xaf63_dc4c_8601_ec8c
        );
    }
}
