// Band handler - converted from band-handler.cpp (eti-cmdline)
// Copyright (C) 2013-2017 Jan van Katwijk - Lazy Chair Computing

use crate::dab_constants::BAND_III;

struct DabFrequency {
    key: &'static str,
    f_khz: i32,
}

static BAND_III_FREQUENCIES: &[DabFrequency] = &[
    DabFrequency { key: "5A",  f_khz: 174928 },
    DabFrequency { key: "5B",  f_khz: 176640 },
    DabFrequency { key: "5C",  f_khz: 178352 },
    DabFrequency { key: "5D",  f_khz: 180064 },
    DabFrequency { key: "6A",  f_khz: 181936 },
    DabFrequency { key: "6B",  f_khz: 183648 },
    DabFrequency { key: "6C",  f_khz: 185360 },
    DabFrequency { key: "6D",  f_khz: 187072 },
    DabFrequency { key: "7A",  f_khz: 188928 },
    DabFrequency { key: "7B",  f_khz: 190640 },
    DabFrequency { key: "7C",  f_khz: 192352 },
    DabFrequency { key: "7D",  f_khz: 194064 },
    DabFrequency { key: "8A",  f_khz: 195936 },
    DabFrequency { key: "8B",  f_khz: 197648 },
    DabFrequency { key: "8C",  f_khz: 199360 },
    DabFrequency { key: "8D",  f_khz: 201072 },
    DabFrequency { key: "9A",  f_khz: 202928 },
    DabFrequency { key: "9B",  f_khz: 204640 },
    DabFrequency { key: "9C",  f_khz: 206352 },
    DabFrequency { key: "9D",  f_khz: 208064 },
    DabFrequency { key: "10A", f_khz: 209936 },
    DabFrequency { key: "10B", f_khz: 211648 },
    DabFrequency { key: "10C", f_khz: 213360 },
    DabFrequency { key: "10D", f_khz: 215072 },
    DabFrequency { key: "11A", f_khz: 216928 },
    DabFrequency { key: "11B", f_khz: 218640 },
    DabFrequency { key: "11C", f_khz: 220352 },
    DabFrequency { key: "11D", f_khz: 222064 },
    DabFrequency { key: "12A", f_khz: 223936 },
    DabFrequency { key: "12B", f_khz: 225648 },
    DabFrequency { key: "12C", f_khz: 227360 },
    DabFrequency { key: "12D", f_khz: 229072 },
    DabFrequency { key: "13A", f_khz: 230748 },
    DabFrequency { key: "13B", f_khz: 232496 },
    DabFrequency { key: "13C", f_khz: 234208 },
    DabFrequency { key: "13D", f_khz: 235776 },
    DabFrequency { key: "13E", f_khz: 237488 },
    DabFrequency { key: "13F", f_khz: 239200 },
];

static LBAND_FREQUENCIES: &[DabFrequency] = &[
    DabFrequency { key: "LA", f_khz: 1452960 },
    DabFrequency { key: "LB", f_khz: 1454672 },
    DabFrequency { key: "LC", f_khz: 1456384 },
    DabFrequency { key: "LD", f_khz: 1458096 },
    DabFrequency { key: "LE", f_khz: 1459808 },
    DabFrequency { key: "LF", f_khz: 1461520 },
    DabFrequency { key: "LG", f_khz: 1463232 },
    DabFrequency { key: "LH", f_khz: 1464944 },
    DabFrequency { key: "LI", f_khz: 1466656 },
    DabFrequency { key: "LJ", f_khz: 1468368 },
    DabFrequency { key: "LK", f_khz: 1470080 },
    DabFrequency { key: "LL", f_khz: 1471792 },
    DabFrequency { key: "LM", f_khz: 1473504 },
    DabFrequency { key: "LN", f_khz: 1475216 },
    DabFrequency { key: "LO", f_khz: 1476928 },
    DabFrequency { key: "LP", f_khz: 1478640 },
];

pub fn frequency(dab_band: u8, channel: &str) -> i32 {
    let table = if dab_band == BAND_III {
        BAND_III_FREQUENCIES
    } else {
        LBAND_FREQUENCIES
    };

    let channel_upper = channel.to_uppercase();
    for f in table {
        if f.key == channel_upper {
            return f.f_khz * 1000;
        }
    }
    // Default: return first entry
    table[0].f_khz * 1000
}
