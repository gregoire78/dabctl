use super::*;

#[test]
fn frame_aligner_extracts_two_symbols_when_aligned() {
    let fft_len = 2048;
    let cp_len = 504;
    let mut aligner = FrameAligner::new(fft_len, cp_len);

    let prefix_samples = 200;
    let mut stream = vec![128u8; prefix_samples * IQ_BYTES_PER_SAMPLE];
    let sym = make_symbol_like_iq(fft_len, cp_len);
    stream.extend_from_slice(&sym);
    stream.extend_from_slice(&sym);

    let symbols = aligner.push_chunk(
        &stream,
        Some(SyncCandidate {
            sample_offset: prefix_samples,
            metric: 0.95,
            cfo_phase_per_sample: 0.0,
        }),
    );

    assert_eq!(symbols.len(), 2);
    assert_eq!(symbols[0].start_sample, prefix_samples);
    assert_eq!(symbols[0].iq_fft_only.len(), fft_len * IQ_BYTES_PER_SAMPLE);
}

#[test]
fn pipeline_reports_aligned_symbols() {
    let fft_len = 2048;
    let cp_len = 504;

    let mut stream = vec![128u8; 1024 * IQ_BYTES_PER_SAMPLE];
    let sym = make_symbol_like_iq(fft_len, cp_len);
    stream.extend_from_slice(&sym);
    stream.extend_from_slice(&sym);

    let mut p = DabPipeline::new(PipelineMode::RawIq);
    let mut out = Vec::new();
    p.process_chunk(&stream, &mut out).expect("process");

    assert_eq!(out.len(), stream.len());
    assert!(p.last_report().aligned_symbols <= 2);
}

#[test]
fn frame_builder_groups_full_frame() {
    let fft_len = 2048;
    let cp_len = 504;
    let symbol_len = fft_len + cp_len;
    let mut builder = FrameBuilder::new(symbol_len);

    let mut symbols = Vec::new();
    for index in 0..DAB_MODE_I_SYMBOLS_PER_FRAME {
        symbols.push(DabOfdmSymbol {
            start_sample: 10_000 + index * symbol_len,
            cfo_phase_per_sample: 0.0,
            iq_fft_only: vec![128; fft_len * IQ_BYTES_PER_SAMPLE],
        });
    }

    let frames = builder.push_symbols(symbols);
    assert_eq!(frames.len(), 1);
    assert_eq!(frames[0].symbol_count, DAB_MODE_I_SYMBOLS_PER_FRAME);
    assert_eq!(frames[0].start_sample, 10_000);
    assert_eq!(frames[0].symbols.len(), DAB_MODE_I_SYMBOLS_PER_FRAME);
}

#[test]
fn frame_builder_rejects_gap_until_realigned() {
    let fft_len = 2048;
    let cp_len = 504;
    let symbol_len = fft_len + cp_len;
    let mut builder = FrameBuilder::new(symbol_len);

    let mut symbols = Vec::new();
    for index in 0..10 {
        symbols.push(DabOfdmSymbol {
            start_sample: 1_000 + index * symbol_len,
            cfo_phase_per_sample: 0.0,
            iq_fft_only: vec![128; fft_len * IQ_BYTES_PER_SAMPLE],
        });
    }
    symbols.push(DabOfdmSymbol {
        start_sample: 99_999,
        cfo_phase_per_sample: 0.0,
        iq_fft_only: vec![128; fft_len * IQ_BYTES_PER_SAMPLE],
    });
    for index in 0..DAB_MODE_I_SYMBOLS_PER_FRAME {
        symbols.push(DabOfdmSymbol {
            start_sample: 200_000 + index * symbol_len,
            cfo_phase_per_sample: 0.0,
            iq_fft_only: vec![128; fft_len * IQ_BYTES_PER_SAMPLE],
        });
    }

    let frames = builder.push_symbols(symbols);
    assert_eq!(frames.len(), 1);
    assert_eq!(frames[0].start_sample, 200_000);
}

#[test]
fn detects_symbol_with_cyclic_prefix() {
    let fft_len = 2048;
    let cp_len = 504;
    let mut detector = OfdmSyncDetector::new(fft_len, cp_len, 0.6);

    let mut stream = vec![128u8; 1024 * IQ_BYTES_PER_SAMPLE];
    let symbol = make_symbol_like_iq(fft_len, cp_len);
    stream.extend_from_slice(&symbol);

    let report = detector.inspect(&stream);
    assert!(report.sync_candidate.is_some());

    let cand = report.sync_candidate.expect("candidate");
    assert!(cand.metric > 0.6);
}

#[test]
fn detector_estimates_near_zero_cfo_on_clean_symbol() {
    let fft_len = 2048;
    let cp_len = 504;
    let mut detector = OfdmSyncDetector::new(fft_len, cp_len, 0.6);

    let mut stream = vec![128u8; 1024 * IQ_BYTES_PER_SAMPLE];
    let symbol = make_symbol_like_iq(fft_len, cp_len);
    stream.extend_from_slice(&symbol);

    let report = detector.inspect(&stream);
    let cand = report.sync_candidate.expect("candidate");
    assert!(cand.cfo_phase_per_sample.abs() < 0.02);
}

#[test]
fn iq_bytes_convert_to_complex_samples() {
    let iq = [128u8, 128, 255, 0, 0, 255];
    let complex = iq_bytes_to_complex(&iq);
    assert_eq!(complex.len(), 3);
    assert_eq!(complex[0], Complex32::new(0.0, 0.0));
    assert_eq!(complex[1], Complex32::new(127.0, -128.0));
    assert_eq!(complex[2], Complex32::new(-128.0, 127.0));
}

#[test]
fn apply_cfo_correction_removes_linear_phase_rotation() {
    let phase_per_sample = 0.03125f32;
    let mut samples = vec![Complex32::new(40.0, 0.0); 32];
    apply_test_phase_rotation(&mut samples, phase_per_sample);

    apply_cfo_correction(&mut samples, phase_per_sample);

    for sample in samples {
        assert!((sample.re - 40.0).abs() < 0.05);
        assert!(sample.im.abs() < 0.05);
    }
}

#[test]
fn frequency_transformer_generates_fft_bins_for_frame() {
    let fft_len = 2048;
    let transformer = FrequencyFrameTransformer::new(fft_len);

    let frame = DabFrameCandidate {
        start_sample: 1234,
        symbol_count: 1,
        symbols: vec![DabOfdmSymbol {
            start_sample: 1234,
            cfo_phase_per_sample: 0.0,
            iq_fft_only: vec![128; fft_len * IQ_BYTES_PER_SAMPLE],
        }],
    };

    let transformed = transformer.transform_frames(&[frame]);
    assert_eq!(transformed.len(), 1);
    assert_eq!(transformed[0].symbol_count, 1);
    assert_eq!(transformed[0].symbols.len(), 1);
    assert_eq!(transformed[0].symbols[0].start_sample, 1234);
    assert_eq!(transformed[0].symbols[0].carriers.len(), fft_len);
}

#[test]
fn frequency_transformer_applies_cfo_correction_before_fft() {
    let fft_len = 64;
    let transformer = FrequencyFrameTransformer::new(fft_len);
    let phase_per_sample = 2.0 * std::f32::consts::PI * 5.0 / fft_len as f32;

    let mut time_samples = vec![Complex32::new(60.0, 0.0); fft_len];
    apply_test_phase_rotation(&mut time_samples, phase_per_sample);
    let iq_fft_only = complex_to_iq_bytes(&time_samples);

    let symbol = DabOfdmSymbol {
        start_sample: 0,
        cfo_phase_per_sample: phase_per_sample,
        iq_fft_only,
    };

    let carriers = transformer.transform_symbol(&symbol);
    let peak_index = carriers
        .iter()
        .enumerate()
        .max_by(|left, right| left.1.norm_sqr().partial_cmp(&right.1.norm_sqr()).expect("finite"))
        .map(|(index, _)| index)
        .expect("peak");

    assert_eq!(peak_index, 0);
}

#[test]
fn carrier_mapper_extracts_1536_active_carriers() {
    let fft_len = 2048;
    let mapper = CarrierMapper::new(fft_len, DAB_MODE_I_ACTIVE_CARRIERS);

    let carriers: Vec<Complex32> = (0..fft_len)
        .map(|index| Complex32::new(index as f32, 0.0))
        .collect();

    let symbol = DabFrequencySymbol {
        start_sample: 77,
        carriers,
    };

    let mapped = mapper.map_symbol(&symbol);
    assert_eq!(mapped.len(), DAB_MODE_I_ACTIVE_CARRIERS);
    assert_eq!(mapped[0], Complex32::new((fft_len - 768) as f32, 0.0));
    assert_eq!(mapped[767], Complex32::new((fft_len - 1) as f32, 0.0));
    assert_eq!(mapped[768], Complex32::new(1.0, 0.0));
    assert_eq!(mapped[1535], Complex32::new(768.0, 0.0));
}

#[test]
fn carrier_mapper_maps_frame_symbols() {
    let mapper = CarrierMapper::new(2048, DAB_MODE_I_ACTIVE_CARRIERS);
    let frame = DabFrequencyFrameCandidate {
        start_sample: 500,
        symbol_count: 1,
        symbols: vec![DabFrequencySymbol {
            start_sample: 500,
            carriers: vec![Complex32::new(0.0, 0.0); 2048],
        }],
    };

    let mapped_frames = mapper.map_frames(&[frame]);
    assert_eq!(mapped_frames.len(), 1);
    assert_eq!(mapped_frames[0].symbol_count, 1);
    assert_eq!(mapped_frames[0].symbols.len(), 1);
    assert_eq!(mapped_frames[0].symbols[0].start_sample, 500);
    assert_eq!(mapped_frames[0].symbols[0].carriers.len(), DAB_MODE_I_ACTIVE_CARRIERS);
}

#[test]
fn ignores_short_chunks() {
    let mut detector = OfdmSyncDetector::new(2048, 504, 0.6);
    let report = detector.inspect(&[128, 129]);
    assert!(report.sync_candidate.is_none());
}
