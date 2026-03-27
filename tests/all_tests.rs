use eti_rtlsdr_rust::pipeline::*;
use eti_rtlsdr_rust::percentile::{percentile95, percentile95_from_histogram};
use rustfft::num_complex::Complex32;

const IQ_BYTES_PER_SAMPLE: usize = 2;
const ETI_LATE_SLOTS_HIST_MAX: usize = 64;

fn make_symbol_like_iq(fft_len: usize, cp_len: usize) -> Vec<u8> {
    let mut payload = Vec::with_capacity(fft_len * IQ_BYTES_PER_SAMPLE);
    let mut seed: u32 = 0x1234_5678;

    for _ in 0..fft_len {
        seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
        let i = ((seed >> 24) as u8).saturating_sub(1);
        seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
        let q = ((seed >> 24) as u8).saturating_sub(1);
        payload.push(i);
        payload.push(q);
    }

    let cp_start = (fft_len - cp_len) * IQ_BYTES_PER_SAMPLE;
    let mut out = Vec::with_capacity((fft_len + cp_len) * IQ_BYTES_PER_SAMPLE);
    out.extend_from_slice(&payload[cp_start..]);
    out.extend_from_slice(&payload);
    out
}

fn complex_to_iq_bytes(samples: &[Complex32]) -> Vec<u8> {
    let mut out = Vec::with_capacity(samples.len() * IQ_BYTES_PER_SAMPLE);
    for sample in samples {
        let i = (sample.re.round() as i16).clamp(-128, 127) + 128;
        let q = (sample.im.round() as i16).clamp(-128, 127) + 128;
        out.push(i as u8);
        out.push(q as u8);
    }
    out
}

fn apply_test_phase_rotation(samples: &mut [Complex32], phase_per_sample: f32) {
    for (index, sample) in samples.iter_mut().enumerate() {
        let phase = phase_per_sample * index as f32;
        let rotation = Complex32::new(phase.cos(), phase.sin());
        *sample *= rotation;
    }
}

#[path = "all_tests/percentile_tests.rs"]
mod percentile_tests;
#[path = "all_tests/sync_pipeline_tests.rs"]
mod sync_pipeline_tests;
#[path = "all_tests/normalization_tests.rs"]
mod normalization_tests;
#[path = "all_tests/fic_tests.rs"]
mod fic_tests;
#[path = "all_tests/signalling_tests.rs"]
mod signalling_tests;
#[path = "all_tests/eti_tests.rs"]
mod eti_tests;
#[path = "all_tests/viterbi_tests.rs"]
mod viterbi_tests;
#[path = "all_tests/rtlsdr_port_tests.rs"]
mod rtlsdr_port_tests;
#[path = "all_tests/eti_port_tests.rs"]
mod eti_port_tests;
