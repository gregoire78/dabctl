use super::*;

#[test]
fn fic_demapper_builds_bitstream_from_normalized_frame() {
    let demapper = FicDemapper::new();
    let normalized = DabNormalizedFrame {
        start_sample: 321,
        phase_reference: DabMappedSymbol {
            start_sample: 321,
            carriers: vec![Complex32::new(1.0, 0.0), Complex32::new(1.0, 0.0)],
        },
        fic_symbols: vec![
            DabMappedSymbol {
                start_sample: 322,
                carriers: vec![Complex32::new(1.0, 1.0), Complex32::new(-1.0, -1.0)],
            },
            DabMappedSymbol {
                start_sample: 323,
                carriers: vec![Complex32::new(-2.0, 0.0), Complex32::new(-2.0, 0.0)],
            },
            DabMappedSymbol {
                start_sample: 324,
                carriers: vec![Complex32::new(-2.0, -2.0), Complex32::new(-2.0, 2.0)],
            },
        ],
        msc_symbols: vec![
            DabMappedSymbol {
                start_sample: 325,
                carriers: vec![Complex32::new(0.0, 0.0); DAB_MODE_I_ACTIVE_CARRIERS],
            };
            DAB_MSC_SYMBOL_COUNT
        ],
        channel_gains: vec![],
    };

    let candidates = demapper.demapp_candidates(&[normalized]);
    assert_eq!(candidates.len(), 1);
    assert_eq!(candidates[0].frame_start_sample, 321);
    assert_eq!(candidates[0].bit_count, 12);
        // Im-first (ik,0) then Re (ik,1) per ETSI EN 300 401 §14.4.1
        assert_eq!(candidates[0].bits, vec![0, 0, 1, 1, 0, 1, 1, 0, 0, 0, 1, 0]);
    assert_eq!(candidates[0].soft_bits.len(), 12);
}

#[test]
fn fic_predecoder_transposes_symbol_major_bits_to_carrier_major_bits() {
    let predecoder = FicPreDecoder::new(3, 256);
    let candidate = FicBitstreamCandidate {
        frame_start_sample: 900,
        bit_count: 12,
        bits: vec![0, 0, 1, 1, 1, 0, 0, 1, 1, 1, 0, 0],
        soft_bits: vec![127, 127, -127, -127, -127, 127, 127, -127, -127, -127, 127, 127],
    };

    let deinterleaved = predecoder.deinterleave_candidates(&[candidate]);
    assert_eq!(deinterleaved.len(), 1);
    assert_eq!(
        deinterleaved[0].bits,
        vec![0, 0, 1, 0, 1, 1, 1, 1, 0, 1, 0, 0]
    );
    assert_eq!(deinterleaved[0].soft_bits.len(), 12);
}

#[test]
fn fic_predecoder_segments_bitstream_into_candidate_windows() {
    let predecoder = FicPreDecoder::new(3, 4);
    let candidate = FicDeinterleavedCandidate {
        frame_start_sample: 901,
        bits: vec![0, 1, 2, 3, 4, 5, 6, 7, 8, 9],
        soft_bits: Vec::new(),
    };

    let segments = predecoder.segment_candidates(&[candidate]);
    assert_eq!(segments.len(), 3);
    assert_eq!(segments[0].segment_index, 0);
    assert_eq!(segments[0].bit_count, 4);
    assert_eq!(segments[0].frame_start_sample, 901);
    assert_eq!(segments[0].bits, vec![0, 1, 2, 3]);
    assert_eq!(segments[2].bit_count, 2);
    assert_eq!(segments[2].bits, vec![8, 9]);
}

#[test]
fn crc_helpers_round_trip_known_payload() {
    let payload = vec![1, 0, 1, 1, 0, 0, 1, 0, 1, 1, 1, 0, 0, 1, 0, 1];
    let crc = crc16_ccitt_false(&payload);
    let crc_bits: Vec<u8> = (0..16)
        .rev()
        .map(|shift| ((crc >> shift) & 1) as u8)
        .collect();

    let mut block = payload.clone();
    block.extend_from_slice(&crc_bits);

    assert!(crc16_matches(&block));
    assert_eq!(bits_to_u16(&crc_bits), crc);
}

#[test]
fn fic_predecoder_builds_crc_checked_blocks() {
    let predecoder = FicPreDecoder::new(3, 32);
    let payload = vec![0, 1, 1, 0, 1, 0, 0, 1, 1, 1, 0, 0, 1, 0, 1, 1];
    let crc = crc16_ccitt_false(&payload);
    let crc_bits: Vec<u8> = (0..16)
        .rev()
        .map(|shift| ((crc >> shift) & 1) as u8)
        .collect();
    let mut good_bits = payload.clone();
    good_bits.extend_from_slice(&crc_bits);

    let segments = vec![
        FicSegmentCandidate {
            frame_start_sample: 100,
            segment_index: 0,
            bit_count: 32,
            bits: good_bits,
        },
        FicSegmentCandidate {
            frame_start_sample: 100,
            segment_index: 1,
            bit_count: 16,
            bits: vec![0; 16],
        },
    ];

    let blocks = predecoder.build_blocks(&segments);
    assert_eq!(blocks.len(), 1);
    assert_eq!(blocks[0].frame_start_sample, 100);
    assert_eq!(blocks[0].block_index, 0);
    assert!(blocks[0].crc_ok);
    assert_eq!(blocks[0].bit_count, 32);
    assert_eq!(blocks[0].bits.len(), 32);
}

#[test]
fn fic_demapper_uses_differential_phase_reference() {
    let demapper = FicDemapper::new();
    let normalized = DabNormalizedFrame {
        start_sample: 100,
        phase_reference: DabMappedSymbol {
            start_sample: 100,
            carriers: vec![Complex32::new(1.0, 0.0); 2],
        },
        fic_symbols: vec![
            DabMappedSymbol {
                start_sample: 101,
                carriers: vec![Complex32::new(1.0, 1.0), Complex32::new(-1.0, 1.0)],
            },
            DabMappedSymbol {
                start_sample: 102,
                carriers: vec![Complex32::new(-2.0, 0.0), Complex32::new(0.0, -2.0)],
            },
            DabMappedSymbol {
                start_sample: 103,
                carriers: vec![Complex32::new(-2.0, -2.0), Complex32::new(-2.0, -2.0)],
            },
        ],
        msc_symbols: vec![
            DabMappedSymbol {
                start_sample: 104,
                carriers: vec![Complex32::new(0.0, 0.0); DAB_MODE_I_ACTIVE_CARRIERS],
            };
            DAB_MSC_SYMBOL_COUNT
        ],
        channel_gains: vec![],
    };

    let bitstream = demapper.demapp_frame(&normalized);
    // Im-first (ik,0) then Re (ik,1) per ETSI EN 300 401 §14.4.1
    assert_eq!(bitstream.bits, vec![0, 0, 0, 1, 0, 1, 0, 1, 0, 0, 1, 0]);
}

#[test]
fn fib_extractor_converts_crc_valid_block_to_fib() {
    let extractor = FibExtractor::new();
    let mut bytes = vec![0u8; DAB_FIB_BYTES - 2];
    bytes[0] = 0x21;
    bytes[1] = 0x34;
    bytes[2] = 0xAA;
    bytes[3] = 0xFF;

    let payload_bits: Vec<u8> = bytes
        .iter()
        .flat_map(|byte| (0..8).rev().map(move |shift| (byte >> shift) & 1))
        .collect();
    let crc = crc16_ccitt_false(&payload_bits);
    let crc_bits: Vec<u8> = (0..16)
        .rev()
        .map(|shift| ((crc >> shift) & 1) as u8)
        .collect();

    let mut block_bits = payload_bits;
    block_bits.extend_from_slice(&crc_bits);

    let blocks = vec![FicBlockCandidate {
        frame_start_sample: 123,
        block_index: 0,
        bit_count: DAB_FIB_BITS,
        crc_ok: true,
        bits: block_bits,
    }];

    let fibs = extractor.extract_fibs(&blocks);
    assert_eq!(fibs.len(), 1);
    assert_eq!(fibs[0].frame_start_sample, 123);
    assert!(fibs[0].crc_ok);
    assert_eq!(fibs[0].bytes.len(), DAB_FIB_BYTES);
}

#[test]
fn fib_extractor_parses_minimal_fig_headers() {
    let extractor = FibExtractor::new();
    let fib = FibCandidate {
        frame_start_sample: 456,
        block_index: 2,
        crc_ok: true,
        bytes: {
            let mut bytes = vec![0u8; DAB_FIB_BYTES];
            bytes[0] = 0b0000_0010;
            bytes[1] = 0x45;
            bytes[2] = 0x99;
            bytes[3] = 0b0010_0001;
            bytes[4] = 0x05;
            bytes[5] = 0xFF;
            bytes
        },
    };

    let figs = extractor.extract_figs(&[fib]);
    assert_eq!(figs.len(), 2);
    assert_eq!(figs[0].frame_start_sample, 456);
    assert_eq!(figs[0].block_index, 2);
    assert_eq!(figs[0].fig_type, 0);
    assert_eq!(figs[0].extension, Some(0x45));
    assert_eq!(figs[0].payload_len, 2);
    assert_eq!(figs[0].payload, vec![0x45, 0x99]);
    match &figs[0].details {
        FigDetails::Type0(details) => {
            assert!(!details.cn);
            assert!(details.oe);
            assert!(!details.pd);
            assert_eq!(details.extension, 0x05);
            assert_eq!(details.body, vec![0x99]);
        }
        FigDetails::Raw => panic!("expected FIG type 0 details"),
    }
    assert_eq!(figs[1].fig_type, 1);
    assert_eq!(figs[1].payload_len, 1);
    assert_eq!(figs[1].payload, vec![0x05]);
    assert!(matches!(figs[1].details, FigDetails::Raw));
}
