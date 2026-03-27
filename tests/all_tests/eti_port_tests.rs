use super::*;
use eti_rtlsdr_rust::eti_handling::cif_interleaver::CifInterleaver;
use eti_rtlsdr_rust::eti_handling::eti_generator::extract_msc_payload_from_normalized_frame;

fn one_subchannel_state() -> MultiplexState {
    let mut s = MultiplexState::new();
    s.sub_channels.push(DabSubChannel {
        id: 0,
        start_address: 0,
        protection: SubChannelProtection::Short {
            table_switch: false,
            table_index: 0,
        },
    });
    s
}

#[test]
fn eti_builder_with_msc_injects_bytes_when_nst_non_zero() {
    let mut builder = EtiFrameBuilder::new();
    let state = one_subchannel_state();
    let msc = vec![0xA5u8; 32];

    let frame = builder.build_frame_with_msc(&state, None, Some(&msc));
    let nst = 1usize;
    let mst_start = 8 + nst * 4 + 4;
    let msc_start = mst_start + ETI_FIC_BYTES;

    assert_eq!(&frame[msc_start..msc_start + 32], &msc[..]);
}

#[test]
fn eti_builder_with_msc_does_not_inject_when_nst_zero() {
    let mut builder = EtiFrameBuilder::new();
    let state = MultiplexState::new();
    let msc = vec![0xA5u8; 32];

    let frame = builder.build_frame_with_msc(&state, None, Some(&msc));
    let mst_start = 12usize;
    let msc_start = mst_start + ETI_FIC_BYTES;

    assert_eq!(&frame[msc_start..msc_start + 32], &[0x55u8; 32]);
}

#[test]
fn eti_builder_build_frame_wrapper_matches_without_msc() {
    let mut a = EtiFrameBuilder::new();
    let mut b = EtiFrameBuilder::new();
    let state = one_subchannel_state();

    let fa = a.build_frame(&state, None);
    let fb = b.build_frame_with_msc(&state, None, None);
    assert_eq!(fa, fb);
}

#[test]
fn eti_builder_with_msc_is_truncated_to_mst_capacity() {
    let mut builder = EtiFrameBuilder::new();
    let state = one_subchannel_state();
    let huge = vec![0xCCu8; 9000];

    let frame = builder.build_frame_with_msc(&state, None, Some(&huge));
    assert_eq!(frame.len(), ETI_FRAME_BYTES);
    let eof_start = ETI_FRAME_BYTES - 8;
    // Just before EOF must not remain default padding everywhere.
    assert_ne!(frame[eof_start - 1], 0x55);
}

#[test]
fn cif_interleaver_requires_full_history_before_output() {
    let mut interleaver = CifInterleaver::new();
    for i in 0..15 {
        let payload = vec![i as u8; 64];
        assert!(interleaver.push_and_interleave(&payload).is_none());
    }
    let out = interleaver.push_and_interleave(&[0xEEu8; 64]);
    assert!(out.is_some());
}

#[test]
fn cif_interleaver_output_len_matches_input() {
    let mut interleaver = CifInterleaver::new();
    for i in 0..16 {
        let payload = vec![i as u8; 127];
        let out = interleaver.push_and_interleave(&payload);
        if i < 15 {
            assert!(out.is_none());
        } else {
            assert_eq!(out.expect("output").len(), 127);
        }
    }
}

#[test]
fn msc_extractor_produces_bytes_from_qpsk_symbols() {
    let symbol = DabMappedSymbol {
        start_sample: 0,
        carriers: vec![
            Complex32::new(1.0, 1.0),
            Complex32::new(-1.0, 1.0),
            Complex32::new(-1.0, -1.0),
            Complex32::new(1.0, -1.0),
        ],
    };
    let frame = DabNormalizedFrame {
        start_sample: 0,
        phase_reference: symbol.clone(),
        fic_symbols: vec![],
        msc_symbols: vec![symbol],
        channel_gains: vec![1.0; 4],
    };

    let out = extract_msc_payload_from_normalized_frame(&frame);
    assert!(!out.is_empty());
}

#[test]
fn eti_frame_padding_is_55_by_default() {
    let mut builder = EtiFrameBuilder::new();
    let frame = builder.build_frame(&MultiplexState::new(), None);
    assert!(frame.iter().any(|b| *b == 0x55));
}
