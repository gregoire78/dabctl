// DAB constants - converted from dab-constants.h (eti-cmdline)

use num_complex::Complex32;

pub const INPUT_RATE: u32 = 2_048_000;
pub const BANDWIDTH: u32 = 1_536_000;
pub const DIFF_LENGTH: i16 = 50;

pub const BAND_III: u8 = 0o100;
pub const L_BAND: u8 = 0o101;

pub const CU_SIZE: usize = 4 * 16;

/// Manhattan-distance approximation of complex magnitude (matches jan_abs in C++)
#[inline]
pub fn jan_abs(z: Complex32) -> f32 {
    z.re.abs() + z.im.abs()
}

#[inline]
pub fn get_db(x: f32) -> f32 {
    20.0 * ((x + 1.0) / 256.0).log10()
}

/// Data structure for subchannel information (matches channel_data in C++)
#[derive(Clone, Debug, Default)]
pub struct ChannelData {
    pub in_use: bool,
    pub id: i16,
    pub start_cu: i16,
    pub uep_flag: bool,
    pub protlev: i16,
    pub size: i16,
    pub bitrate: i16,
}

/// Extract `size` bits from a bit-array `d` starting at `offset`
#[inline]
pub fn get_bits(d: &[u8], offset: usize, size: usize) -> u16 {
    let mut res: u16 = 0;
    for i in 0..size {
        res <<= 1;
        res |= d[offset + i] as u16 & 1;
    }
    res
}

#[inline]
pub fn get_bits_1(d: &[u8], offset: usize) -> u16 {
    d[offset] as u16 & 0x01
}

#[inline]
pub fn get_bits_2(d: &[u8], offset: usize) -> u16 {
    ((d[offset] as u16) << 1) | (d[offset + 1] as u16 & 1)
}

#[inline]
pub fn get_bits_3(d: &[u8], offset: usize) -> u16 {
    ((d[offset] as u16) << 2) | ((d[offset + 1] as u16) << 1) | (d[offset + 2] as u16 & 1)
}

#[inline]
pub fn get_bits_4(d: &[u8], offset: usize) -> u16 {
    get_bits(d, offset, 4)
}

#[inline]
pub fn get_bits_5(d: &[u8], offset: usize) -> u16 {
    get_bits(d, offset, 5)
}

#[inline]
pub fn get_bits_6(d: &[u8], offset: usize) -> u16 {
    get_bits(d, offset, 6)
}

#[inline]
pub fn get_bits_8(d: &[u8], offset: usize) -> u16 {
    get_bits(d, offset, 8)
}

/// Get bits as u32 (for larger fields)
#[inline]
pub fn get_lbits(d: &[u8], offset: usize, size: usize) -> u32 {
    let mut res: u32 = 0;
    for i in 0..size {
        res <<= 1;
        res |= d[offset + i] as u32 & 1;
    }
    res
}

/// CRC-16 with CCITT polynomial 0x1021, used throughout DAB
pub fn check_crc_bits(data: &[u8], length: usize) -> bool {
    let data_len = length - 16;
    let mut crc: u16 = 0xFFFF;
    for i in 0..data_len {
        let bit = data[i] & 1;
        if ((crc >> 15) ^ bit as u16) & 1 != 0 {
            crc = (crc << 1) ^ 0x1021;
        } else {
            crc <<= 1;
        }
    }
    crc = !crc & 0xFFFF;
    let mut transmitted: u16 = 0;
    for i in 0..16 {
        transmitted <<= 1;
        transmitted |= data[data_len + i] as u16 & 1;
    }
    crc == transmitted
}

/// CRC-16 byte-level calculator (used in ETI frame construction)
pub const CRC_TAB_1021: [u16; 256] = [
    0x0000, 0x1021, 0x2042, 0x3063, 0x4084, 0x50a5, 0x60c6, 0x70e7,
    0x8108, 0x9129, 0xa14a, 0xb16b, 0xc18c, 0xd1ad, 0xe1ce, 0xf1ef,
    0x1231, 0x0210, 0x3273, 0x2252, 0x52b5, 0x4294, 0x72f7, 0x62d6,
    0x9339, 0x8318, 0xb37b, 0xa35a, 0xd3bd, 0xc39c, 0xf3ff, 0xe3de,
    0x2462, 0x3443, 0x0420, 0x1401, 0x64e6, 0x74c7, 0x44a4, 0x5485,
    0xa56a, 0xb54b, 0x8528, 0x9509, 0xe5ee, 0xf5cf, 0xc5ac, 0xd58d,
    0x3653, 0x2672, 0x1611, 0x0630, 0x76d7, 0x66f6, 0x5695, 0x46b4,
    0xb75b, 0xa77a, 0x9719, 0x8738, 0xf7df, 0xe7fe, 0xd79d, 0xc7bc,
    0x48c4, 0x58e5, 0x6886, 0x78a7, 0x0840, 0x1861, 0x2802, 0x3823,
    0xc9cc, 0xd9ed, 0xe98e, 0xf9af, 0x8948, 0x9969, 0xa90a, 0xb92b,
    0x5af5, 0x4ad4, 0x7ab7, 0x6a96, 0x1a71, 0x0a50, 0x3a33, 0x2a12,
    0xdbfd, 0xcbdc, 0xfbbf, 0xeb9e, 0x9b79, 0x8b58, 0xbb3b, 0xab1a,
    0x6ca6, 0x7c87, 0x4ce4, 0x5cc5, 0x2c22, 0x3c03, 0x0c60, 0x1c41,
    0xedae, 0xfd8f, 0xcdec, 0xddcd, 0xad2a, 0xbd0b, 0x8d68, 0x9d49,
    0x7e97, 0x6eb6, 0x5ed5, 0x4ef4, 0x3e13, 0x2e32, 0x1e51, 0x0e70,
    0xff9f, 0xefbe, 0xdfdd, 0xcffc, 0xbf1b, 0xaf3a, 0x9f59, 0x8f78,
    0x9188, 0x81a9, 0xb1ca, 0xa1eb, 0xd10c, 0xc12d, 0xf14e, 0xe16f,
    0x1080, 0x00a1, 0x30c2, 0x20e3, 0x5004, 0x4025, 0x7046, 0x6067,
    0x83b9, 0x9398, 0xa3fb, 0xb3da, 0xc33d, 0xd31c, 0xe37f, 0xf35e,
    0x02b1, 0x1290, 0x22f3, 0x32d2, 0x4235, 0x5214, 0x6277, 0x7256,
    0xb5ea, 0xa5cb, 0x95a8, 0x8589, 0xf56e, 0xe54f, 0xd52c, 0xc50d,
    0x34e2, 0x24c3, 0x14a0, 0x0481, 0x7466, 0x6447, 0x5424, 0x4405,
    0xa7db, 0xb7fa, 0x8799, 0x97b8, 0xe75f, 0xf77e, 0xc71d, 0xd73c,
    0x26d3, 0x36f2, 0x0691, 0x16b0, 0x6657, 0x7676, 0x4615, 0x5634,
    0xd94c, 0xc96d, 0xf90e, 0xe92f, 0x99c8, 0x89e9, 0xb98a, 0xa9ab,
    0x5844, 0x4865, 0x7806, 0x6827, 0x18c0, 0x08e1, 0x3882, 0x28a3,
    0xcb7d, 0xdb5c, 0xeb3f, 0xfb1e, 0x8bf9, 0x9bd8, 0xabbb, 0xbb9a,
    0x4a75, 0x5a54, 0x6a37, 0x7a16, 0x0af1, 0x1ad0, 0x2ab3, 0x3a92,
    0xfd2e, 0xed0f, 0xdd6c, 0xcd4d, 0xbdaa, 0xad8b, 0x9de8, 0x8dc9,
    0x7c26, 0x6c07, 0x5c64, 0x4c45, 0x3ca2, 0x2c83, 0x1ce0, 0x0cc1,
    0xef1f, 0xff3e, 0xcf5d, 0xdf7c, 0xaf9b, 0xbfba, 0x8fd9, 0x9ff8,
    0x6e17, 0x7e36, 0x4e55, 0x5e74, 0x2e93, 0x3eb2, 0x0ed1, 0x1ef0,
];

pub fn calc_crc(data: &[u8], offset: usize, length: usize) -> u16 {
    let mut crc: u16 = 0xFFFF;
    for i in 0..length {
        let temp = ((data[offset + i] as u16) ^ (crc >> 8)) & 0xff;
        crc = CRC_TAB_1021[temp as usize] ^ (crc << 8);
    }
    crc & 0xffff
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn input_rate() {
        assert_eq!(INPUT_RATE, 2_048_000);
    }

    #[test]
    fn jan_abs_manhattan() {
        let z = Complex32::new(3.0, 4.0);
        assert!((jan_abs(z) - 7.0).abs() < 1e-6);
        let z2 = Complex32::new(-1.0, -2.0);
        assert!((jan_abs(z2) - 3.0).abs() < 1e-6);
    }

    #[test]
    fn get_bits_basic() {
        let data: Vec<u8> = vec![1, 0, 1, 1, 0, 0, 1, 0];
        assert_eq!(get_bits(&data, 0, 4), 0b1011);
        assert_eq!(get_bits(&data, 4, 4), 0b0010);
        assert_eq!(get_bits(&data, 0, 8), 0b10110010);
    }

    #[test]
    fn get_bits_1_values() {
        assert_eq!(get_bits_1(&[0], 0), 0);
        assert_eq!(get_bits_1(&[1], 0), 1);
        assert_eq!(get_bits_1(&[0xFF], 0), 1);
    }

    #[test]
    fn check_crc_bits_valid() {
        let mut bits = vec![0u8; 24];
        let mut crc: u16 = 0xFFFF;
        for _i in 0..8 {
            if ((crc >> 15) ^ 0u16) & 1 != 0 {
                crc = (crc << 1) ^ 0x1021;
            } else {
                crc <<= 1;
            }
        }
        crc = !crc & 0xFFFF;
        for i in 0..16 {
            bits[8 + i] = ((crc >> (15 - i)) & 1) as u8;
        }
        assert!(check_crc_bits(&bits, 24));
    }

    #[test]
    fn check_crc_bits_invalid() {
        let bits = vec![0u8; 24];
        assert!(!check_crc_bits(&bits, 24));
    }

    #[test]
    fn channel_data_defaults() {
        let cd = ChannelData::default();
        assert!(!cd.in_use);
        assert_eq!(cd.id, 0);
        assert_eq!(cd.start_cu, 0);
        assert!(!cd.uep_flag);
        assert_eq!(cd.protlev, 0);
        assert_eq!(cd.size, 0);
        assert_eq!(cd.bitrate, 0);
    }

    #[test]
    fn calc_crc_empty() {
        assert_eq!(calc_crc(&[0u8; 0], 0, 0), 0xFFFF);
    }

    #[test]
    fn calc_crc_known_value() {
        let data = b"123456789";
        assert_eq!(calc_crc(data, 0, 9), 0x29B1);
    }
}
