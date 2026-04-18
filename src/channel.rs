// Literal translation support for DABstar channel selection.
// ETSI EN 300 401, Band III and L-band ensemble center frequencies.

pub fn channel_to_frequency(channel: &str) -> Option<u32> {
    match channel.to_ascii_uppercase().as_str() {
        "5A" => Some(174_928_000),
        "5B" => Some(176_640_000),
        "5C" => Some(178_352_000),
        "5D" => Some(180_064_000),
        "6A" => Some(181_936_000),
        "6B" => Some(183_648_000),
        "6C" => Some(185_360_000),
        "6D" => Some(187_072_000),
        "7A" => Some(188_928_000),
        "7B" => Some(190_640_000),
        "7C" => Some(192_352_000),
        "7D" => Some(194_064_000),
        "8A" => Some(195_936_000),
        "8B" => Some(197_648_000),
        "8C" => Some(199_360_000),
        "8D" => Some(201_072_000),
        "9A" => Some(202_928_000),
        "9B" => Some(204_640_000),
        "9C" => Some(206_352_000),
        "9D" => Some(208_064_000),
        "10A" => Some(209_936_000),
        "10B" => Some(211_648_000),
        "10C" => Some(213_360_000),
        "10D" => Some(215_072_000),
        "11A" => Some(216_928_000),
        "11B" => Some(218_640_000),
        "11C" => Some(220_352_000),
        "11D" => Some(222_064_000),
        "12A" => Some(223_936_000),
        "12B" => Some(225_648_000),
        "12C" => Some(227_360_000),
        "12D" => Some(229_072_000),
        "13A" => Some(230_784_000),
        "13B" => Some(232_496_000),
        "13C" => Some(234_208_000),
        "13D" => Some(235_776_000),
        "13E" => Some(237_488_000),
        "13F" => Some(239_200_000),
        "LA" => Some(1_452_960_000),
        "LB" => Some(1_454_672_000),
        "LC" => Some(1_456_384_000),
        "LD" => Some(1_458_096_000),
        "LE" => Some(1_459_808_000),
        "LF" => Some(1_461_520_000),
        "LG" => Some(1_463_232_000),
        "LH" => Some(1_464_944_000),
        "LI" => Some(1_466_656_000),
        "LJ" => Some(1_468_368_000),
        "LK" => Some(1_470_080_000),
        "LL" => Some(1_471_792_000),
        "LM" => Some(1_473_504_000),
        "LN" => Some(1_475_216_000),
        "LO" => Some(1_476_928_000),
        "LP" => Some(1_478_640_000),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::channel_to_frequency;

    #[test]
    fn known_channel_maps_to_frequency() {
        assert_eq!(channel_to_frequency("6C"), Some(185_360_000));
        assert_eq!(channel_to_frequency("la"), Some(1_452_960_000));
        assert_eq!(channel_to_frequency("0Z"), None);
    }
}
