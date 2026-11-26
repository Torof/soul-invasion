//! The hash every draw of the planet comes from: a copy of the simulation's
//! own (`sim::gen::hash`, splitmix64 chained), pinned by the same values, so
//! this crate builds without the simulation and draws the same planets.

/// The odd constant of splitmix64 (2^64 divided by the golden ratio).
const GOLDEN: u64 = 0x9e37_79b9_7f4a_7c15;

/// A hash of a few numbers, well mixed (splitmix64 finaliser, chained).
/// The same numbers always give the same hash, on every machine; order
/// matters.
pub fn hash(values: &[u64]) -> u64 {
    let mut h: u64 = GOLDEN;
    for &v in values {
        h ^= v
            .wrapping_add(GOLDEN)
            .wrapping_add(h << 6)
            .wrapping_add(h >> 2);
        h = h.wrapping_add(GOLDEN);
        let mut z = h;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        h = z ^ (z >> 31);
    }
    h
}

#[cfg(test)]
mod tests {
    use super::hash;

    #[test]
    fn hash_is_pinned_to_the_simulations() {
        assert_eq!(hash(&[]), 0x9e37_79b9_7f4a_7c15);
        assert_eq!(hash(&[0]), 0x3a13_eaf2_0263_2586);
        assert_eq!(hash(&[1]), 0xf6fe_0721_c560_f476);
        assert_eq!(hash(&[1, 2, 3]), 0xf51e_fbb9_4395_9f15);
        assert_eq!(hash(&[u64::MAX, 0x9a7c, 7]), 0x8abb_c423_61c4_0023);
    }
}
