// Integration tests for eti-rtlsdr-rust
// Tests for the new architecture modules

use eti_rtlsdr_rust::dab_constants::*;
use eti_rtlsdr_rust::support::dab_params::DabParams;
use eti_rtlsdr_rust::support::band_handler;
use eti_rtlsdr_rust::support::ringbuffer::RingBuffer;
use num_complex::Complex32;

// ==========================================================================
// DabParams tests
// ==========================================================================

#[test]
fn test_dab_params_mode1_defaults() {
    let p = DabParams::new(1);
    assert_eq!(p.dab_mode, 1);
    assert_eq!(p.l, 76);
    assert_eq!(p.k, 1536);
    assert_eq!(p.t_u, 2048);
    assert_eq!(p.t_s, 2552);
    assert_eq!(p.t_g, 504);
    assert_eq!(p.t_null, 2656);
    assert_eq!(p.t_f, 196608);
    assert_eq!(p.carrier_diff, 1000);
}

#[test]
fn test_dab_params_mode2() {
    let p = DabParams::new(2);
    assert_eq!(p.dab_mode, 2);
    assert_eq!(p.l, 76);
    assert_eq!(p.k, 384);
    assert_eq!(p.t_u, 512);
}

#[test]
fn test_dab_params_mode3() {
    let p = DabParams::new(3);
    assert_eq!(p.dab_mode, 3);
    assert_eq!(p.l, 153);
    assert_eq!(p.k, 192);
    assert_eq!(p.t_u, 256);
}

#[test]
fn test_dab_params_mode4() {
    let p = DabParams::new(4);
    assert_eq!(p.dab_mode, 4);
    assert_eq!(p.l, 76);
    assert_eq!(p.k, 768);
    assert_eq!(p.t_u, 1024);
}

#[test]
fn test_dab_params_invalid_defaults_to_mode1() {
    let p = DabParams::new(99);
    assert_eq!(p.dab_mode, 1);
    assert_eq!(p.k, 1536);
}

#[test]
fn test_dab_params_get_carriers() {
    let p = DabParams::new(1);
    assert_eq!(p.get_carriers(), 1536);
    let p2 = DabParams::new(2);
    assert_eq!(p2.get_carriers(), 384);
}

#[test]
fn test_dab_params_get_l() {
    let p = DabParams::new(1);
    assert_eq!(p.get_l(), 76);
    let p3 = DabParams::new(3);
    assert_eq!(p3.get_l(), 153);
}

#[test]
fn test_dab_params_symbol_length() {
    // t_s = t_u + t_g for all modes
    for mode in [1, 2, 3, 4] {
        let p = DabParams::new(mode);
        assert_eq!(p.t_s, p.t_u + p.t_g, "t_s != t_u + t_g for mode {}", mode);
    }
}

// ==========================================================================
// Band handler tests
// ==========================================================================

#[test]
fn test_band_handler_known_channel() {
    let freq = band_handler::frequency(BAND_III, "11C");
    // 11C = 220352 kHz
    assert_eq!(freq, 220352 * 1000);
}

#[test]
fn test_band_handler_5a() {
    let freq = band_handler::frequency(BAND_III, "5A");
    assert_eq!(freq, 174928 * 1000);
}

#[test]
fn test_band_handler_case_insensitive() {
    let f1 = band_handler::frequency(BAND_III, "11c");
    let f2 = band_handler::frequency(BAND_III, "11C");
    assert_eq!(f1, f2);
}

#[test]
fn test_band_handler_lband() {
    let freq = band_handler::frequency(L_BAND, "LA");
    assert_eq!(freq, 1452960 * 1000);
}

#[test]
fn test_band_handler_unknown_returns_first() {
    // Unknown channel returns the first frequency in the table
    let freq = band_handler::frequency(BAND_III, "UNKNOWN");
    assert_eq!(freq, 174928 * 1000); // = 5A
}

#[test]
fn test_band_handler_all_band_iii_increasing() {
    let channels = [
        "5A", "5B", "5C", "5D", "6A", "6B", "6C", "6D",
        "7A", "7B", "7C", "7D", "8A", "8B", "8C", "8D",
        "9A", "9B", "9C", "9D", "10A", "10B", "10C", "10D",
        "11A", "11B", "11C", "11D", "12A", "12B", "12C", "12D",
        "13A", "13B", "13C", "13D", "13E", "13F",
    ];
    let mut prev = 0i32;
    for ch in &channels {
        let f = band_handler::frequency(BAND_III, ch);
        assert!(f > prev, "Frequency for {} should be > prev ({})", ch, prev);
        prev = f;
    }
}

// ==========================================================================
// DAB constants tests
// ==========================================================================

#[test]
fn test_input_rate() {
    assert_eq!(INPUT_RATE, 2_048_000);
}

#[test]
fn test_jan_abs() {
    let z = Complex32::new(3.0, 4.0);
    assert!((jan_abs(z) - 7.0).abs() < 1e-6);
    let z2 = Complex32::new(-1.0, -2.0);
    assert!((jan_abs(z2) - 3.0).abs() < 1e-6);
}

#[test]
fn test_get_bits_basic() {
    let data: Vec<u8> = vec![1, 0, 1, 1, 0, 0, 1, 0];
    assert_eq!(get_bits(&data, 0, 4), 0b1011); // 1,0,1,1 -> 11
    assert_eq!(get_bits(&data, 4, 4), 0b0010); // 0,0,1,0 -> 2
    assert_eq!(get_bits(&data, 0, 8), 0b10110010); // 178
}

#[test]
fn test_get_bits_1() {
    assert_eq!(get_bits_1(&[0], 0), 0);
    assert_eq!(get_bits_1(&[1], 0), 1);
    assert_eq!(get_bits_1(&[0xFF], 0), 1);
}

#[test]
fn test_check_crc_bits_valid() {
    // Build a bit array with data and CRC
    // The CRC-CCITT of 0x00 (8 zero-bits) should be 0x1D0F
    let mut bits = vec![0u8; 24]; // 8 data bits + 16 CRC bits
    // data = all zeros → CRC of all-zero byte via CCITT is specific value
    // Let's compute manually: init 0xFFFF, process 8 zero bits
    let mut crc: u16 = 0xFFFF;
    for _i in 0..8 {
        let bit = 0u8;
        if ((crc >> 15) ^ bit as u16) & 1 != 0 {
            crc = (crc << 1) ^ 0x1021;
        } else {
            crc <<= 1;
        }
    }
    crc = !crc & 0xFFFF;
    // Place CRC bits
    for i in 0..16 {
        bits[8 + i] = ((crc >> (15 - i)) & 1) as u8;
    }
    assert!(check_crc_bits(&bits, 24));
}

#[test]
fn test_check_crc_bits_invalid() {
    let bits = vec![0u8; 24]; // all zeros, CRC won't match
    assert!(!check_crc_bits(&bits, 24));
}

#[test]
fn test_channel_data_defaults() {
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
fn test_calc_crc() {
    // CRC of empty data should be 0xFFFF
    let data = [0u8; 0];
    let crc = calc_crc(&data, 0, 0);
    assert_eq!(crc, 0xFFFF);
}

#[test]
fn test_calc_crc_known() {
    // CRC-CCITT of "123456789" (ASCII bytes) = 0x29B1
    let data = b"123456789";
    let crc = calc_crc(data, 0, 9);
    assert_eq!(crc, 0x29B1);
}

// ==========================================================================
// RingBuffer tests
// ==========================================================================

#[test]
fn test_ringbuffer_basic() {
    let rb: RingBuffer<u8> = RingBuffer::new(100);
    assert_eq!(rb.available_read(), 0);
    assert_eq!(rb.available_write(), 100);
}

#[test]
fn test_ringbuffer_put_get() {
    let rb: RingBuffer<u8> = RingBuffer::new(100);
    let data = vec![1u8, 2, 3, 4, 5];
    let written = rb.put_data(&data);
    assert_eq!(written, 5);
    assert_eq!(rb.available_read(), 5);

    let mut out = vec![0u8; 5];
    let read = rb.get_data(&mut out);
    assert_eq!(read, 5);
    assert_eq!(out, vec![1, 2, 3, 4, 5]);
    assert_eq!(rb.available_read(), 0);
}

#[test]
fn test_ringbuffer_overflow() {
    let rb: RingBuffer<u8> = RingBuffer::new(3);
    let data = vec![1u8, 2, 3, 4, 5];
    let written = rb.put_data(&data);
    assert_eq!(written, 3); // only 3 fit
    assert_eq!(rb.available_read(), 3);
}

#[test]
fn test_ringbuffer_flush() {
    let rb: RingBuffer<u8> = RingBuffer::new(100);
    rb.put_data(&[1, 2, 3]);
    assert_eq!(rb.available_read(), 3);
    rb.flush();
    assert_eq!(rb.available_read(), 0);
}
