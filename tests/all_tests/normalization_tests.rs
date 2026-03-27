use super::*;

#[test]
fn frame_normalizer_splits_reference_fic_and_msc() {
    let normalizer = FrameNormalizer::new();
    let frame = DabMappedFrameCandidate {
        start_sample: 42,
        symbol_count: DAB_MODE_I_SYMBOLS_PER_FRAME,
        symbols: (0..DAB_MODE_I_SYMBOLS_PER_FRAME)
            .map(|index| DabMappedSymbol {
                start_sample: 42 + index,
                carriers: vec![Complex32::new(index as f32, 0.0); DAB_MODE_I_ACTIVE_CARRIERS],
            })
            .collect(),
    };

    let normalized = normalizer.normalize_frames(&[frame]);
    assert_eq!(normalized.len(), 1);
    assert_eq!(normalized[0].phase_reference.start_sample, 42);
    assert_eq!(normalized[0].fic_symbols.len(), DAB_FIC_SYMBOL_COUNT);
    assert_eq!(normalized[0].msc_symbols.len(), DAB_MSC_SYMBOL_COUNT);
}

#[test]
fn frame_normalizer_equalizes_fic_carrier_amplitudes_from_phase_reference() {
    let normalizer = FrameNormalizer::new();
    let theoretical_reference = dab_mode_i_phase_reference_mapped();
    let channel = [Complex32::new(2.0, 1.0), Complex32::new(-1.5, 0.5)];
    let expected_fic = vec![Complex32::new(1.0, 1.0), Complex32::new(-1.0, 1.0)];
    let phase_reference_carriers: Vec<Complex32> = theoretical_reference
        .iter()
        .take(2)
        .zip(channel.iter())
        .map(|(reference, gain)| *reference * *gain)
        .collect();
    let fic_carriers: Vec<Complex32> = expected_fic
        .iter()
        .zip(channel.iter())
        .map(|(carrier, gain)| *carrier * *gain)
        .collect();

    let frame = DabMappedFrameCandidate {
        start_sample: 200,
        symbol_count: DAB_MODE_I_SYMBOLS_PER_FRAME,
        symbols: (0..DAB_MODE_I_SYMBOLS_PER_FRAME)
            .map(|index| {
                let carriers = if index == 0 {
                    phase_reference_carriers.clone()
                } else if (1..=DAB_FIC_SYMBOL_COUNT).contains(&index) {
                    fic_carriers.clone()
                } else {
                    vec![Complex32::new(0.0, 0.0), Complex32::new(0.0, 0.0)]
                };

                DabMappedSymbol {
                    start_sample: 200 + index,
                    carriers,
                }
            })
            .collect(),
    };

    let normalized = normalizer.normalize_frame(&frame).expect("normalized frame");
    for (carrier, expected) in normalized.fic_symbols[0].carriers.iter().zip(expected_fic.iter()) {
        assert!((carrier.re - expected.re).abs() < 1e-5);
        assert!((carrier.im - expected.im).abs() < 1e-5);
    }
    for (carrier, expected) in normalized.phase_reference.carriers.iter().take(2).zip(theoretical_reference.iter().take(2)) {
        assert!((carrier.re - expected.re).abs() < 1e-5);
        assert!((carrier.im - expected.im).abs() < 1e-5);
    }
}

#[test]
fn frame_normalizer_extracts_fic_candidates() {
    let normalizer = FrameNormalizer::new();
    let normalized = DabNormalizedFrame {
        start_sample: 777,
        phase_reference: DabMappedSymbol {
            start_sample: 777,
            carriers: vec![Complex32::new(0.0, 0.0); DAB_MODE_I_ACTIVE_CARRIERS],
        },
        fic_symbols: vec![
            DabMappedSymbol {
                start_sample: 778,
                carriers: vec![Complex32::new(0.0, 0.0); DAB_MODE_I_ACTIVE_CARRIERS],
            },
            DabMappedSymbol {
                start_sample: 779,
                carriers: vec![Complex32::new(0.0, 0.0); DAB_MODE_I_ACTIVE_CARRIERS],
            },
            DabMappedSymbol {
                start_sample: 780,
                carriers: vec![Complex32::new(0.0, 0.0); DAB_MODE_I_ACTIVE_CARRIERS],
            },
        ],
        msc_symbols: vec![
            DabMappedSymbol {
                start_sample: 781,
                carriers: vec![Complex32::new(0.0, 0.0); DAB_MODE_I_ACTIVE_CARRIERS],
            };
            DAB_MSC_SYMBOL_COUNT
        ],
        channel_gains: vec![],
    };

    let fic = normalizer.extract_fic_candidates(&[normalized]);
    assert_eq!(fic.len(), 1);
    assert_eq!(fic[0].frame_start_sample, 777);
    assert_eq!(fic[0].symbol_count, DAB_FIC_SYMBOL_COUNT);
    assert_eq!(fic[0].carriers_per_symbol, DAB_MODE_I_ACTIVE_CARRIERS);
}

#[test]
fn frame_normalizer_reports_low_prs_error_after_complex_equalization() {
    let normalizer = FrameNormalizer::new();
    let theoretical_reference = dab_mode_i_phase_reference_mapped();
    let channel = [Complex32::new(2.0, 1.0), Complex32::new(-1.5, 0.5)];
    let phase_reference_carriers: Vec<Complex32> = theoretical_reference
        .iter()
        .take(2)
        .zip(channel.iter())
        .map(|(reference, gain)| *reference * *gain)
        .collect();

    let frame = DabMappedFrameCandidate {
        start_sample: 300,
        symbol_count: DAB_MODE_I_SYMBOLS_PER_FRAME,
        symbols: (0..DAB_MODE_I_SYMBOLS_PER_FRAME)
            .map(|index| {
                let carriers = if index == 0 {
                    phase_reference_carriers.clone()
                } else {
                    vec![Complex32::new(0.0, 0.0), Complex32::new(0.0, 0.0)]
                };

                DabMappedSymbol {
                    start_sample: 300 + index,
                    carriers,
                }
            })
            .collect(),
    };

    let quality = normalizer
        .analyze_phase_reference(&frame)
        .expect("phase reference quality");

    let gain_0 = channel[0].norm();
    let gain_1 = channel[1].norm();
    let expected_spread_db = 20.0 * (gain_0.max(gain_1) / gain_0.min(gain_1)).log10();

    assert!(quality.eq_mse < 1e-5);
    assert!(quality.eq_phase_rms_rad < 1e-5);
    assert!((quality.channel_gain_avg - ((gain_0 + gain_1) * 0.5)).abs() < 1e-5);
    assert!((quality.channel_gain_spread_db - expected_spread_db).abs() < 1e-5);
}

#[test]
fn qpsk_hard_demapper_maps_quadrants_to_bits() {
    assert_eq!(qpsk_hard_demapp(Complex32::new(1.0, 1.0)), (0, 0));
    assert_eq!(qpsk_hard_demapp(Complex32::new(-1.0, 1.0)), (1, 0));
    assert_eq!(qpsk_hard_demapp(Complex32::new(-1.0, -1.0)), (1, 1));
    assert_eq!(qpsk_hard_demapp(Complex32::new(1.0, -1.0)), (0, 1));
}
