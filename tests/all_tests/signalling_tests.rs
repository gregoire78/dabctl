use super::*;

#[test]
fn signalling_decoder_aggregates_type0_extensions() {
    let decoder = SignallingDecoder::new();
    let figs = vec![
        FigCandidate {
            frame_start_sample: 10,
            block_index: 0,
            fig_type: 0,
            extension: Some(0x45),
            payload_len: 2,
            payload: vec![0x45, 0xAA],
            details: FigDetails::Type0(FigType0Details {
                cn: false,
                oe: true,
                pd: false,
                extension: 0x05,
                body: vec![0xAA],
            }),
        },
        FigCandidate {
            frame_start_sample: 11,
            block_index: 0,
            fig_type: 0,
            extension: Some(0x25),
            payload_len: 2,
            payload: vec![0x25, 0xBB],
            details: FigDetails::Type0(FigType0Details {
                cn: false,
                oe: false,
                pd: true,
                extension: 0x05,
                body: vec![0xBB],
            }),
        },
        FigCandidate {
            frame_start_sample: 12,
            block_index: 1,
            fig_type: 1,
            extension: Some(0x33),
            payload_len: 1,
            payload: vec![0x33],
            details: FigDetails::Raw,
        },
    ];

    let snapshot = decoder.decode(&figs).expect("snapshot");
    assert_eq!(snapshot.fig_count, 3);
    assert_eq!(snapshot.type1_count, 1);
    assert_eq!(snapshot.type0_extensions.len(), 1);
    assert_eq!(snapshot.type0_extensions[0].extension, 0x05);
    assert_eq!(snapshot.type0_extensions[0].count, 2);
    assert!(!snapshot.type0_extensions[0].cn);
    assert!(snapshot.type0_extensions[0].oe);
    assert!(snapshot.type0_extensions[0].pd);
    assert_eq!(snapshot.type0_extensions[0].last_body, vec![0xBB]);
}

#[test]
fn multiplex_state_accumulates_signalling_snapshots() {
    let mut state = MultiplexState::new();
    let first = SignallingSnapshot {
        fig_count: 2,
        type1_count: 0,
        type0_extensions: vec![Type0ExtensionSummary {
            extension: 0x05,
            count: 1,
            cn: false,
            oe: true,
            pd: false,
            last_body: vec![0x11],
        }],
    };
    let second = SignallingSnapshot {
        fig_count: 3,
        type1_count: 1,
        type0_extensions: vec![
            Type0ExtensionSummary {
                extension: 0x05,
                count: 2,
                cn: true,
                oe: false,
                pd: true,
                last_body: vec![0x22],
            },
            Type0ExtensionSummary {
                extension: 0x07,
                count: 1,
                cn: false,
                oe: false,
                pd: false,
                last_body: vec![0x33],
            },
        ],
    };

    state.update(&first, Some(1000));
    state.update(&second, Some(2000));

    assert_eq!(state.updates, 2);
    assert_eq!(state.total_fig_count, 5);
    assert_eq!(state.total_type1_count, 1);
    assert_eq!(state.last_frame_start_sample, Some(2000));
    assert_eq!(state.type0_extensions.len(), 2);
    assert_eq!(state.type0_extensions[0].extension, 0x05);
    assert_eq!(state.type0_extensions[0].count, 3);
    assert!(state.type0_extensions[0].cn);
    assert!(state.type0_extensions[0].oe);
    assert!(state.type0_extensions[0].pd);
    assert_eq!(state.type0_extensions[0].last_body, vec![0x22]);
    assert_eq!(state.type0_extensions[1].extension, 0x07);
}

#[test]
fn decode_fig0_ext0_parses_ensemble_info() {
    let body = vec![0x12u8, 0x34, 0xA5, 200];
    match decode_fig0(0, false, &body) {
        Fig0Decoded::EnsembleInfo(info) => {
            assert_eq!(info.eid, 0x1234);
            assert_eq!(info.change_flags, 0b10);
            assert!(info.al);
            assert_eq!(info.cif_count_high, 5);
            assert_eq!(info.cif_count_low, 200);
        }
        _ => panic!("expected EnsembleInfo"),
    }
}

#[test]
fn decode_fig0_ext0_rejects_short_body() {
    assert_eq!(decode_fig0(0, false, &[0x12, 0x34, 0xA5]), Fig0Decoded::Unknown);
}

#[test]
fn decode_fig0_ext1_parses_short_form_sub_channels() {
    let body = vec![0x15u8, 0x2C, 0x0C];
    match decode_fig0(1, false, &body) {
        Fig0Decoded::SubChannels(channels) => {
            assert_eq!(channels.len(), 1);
            assert_eq!(channels[0].id, 5);
            assert_eq!(channels[0].start_address, 300);
            match channels[0].protection {
                SubChannelProtection::Short { table_switch, table_index } => {
                    assert!(!table_switch);
                    assert_eq!(table_index, 12);
                }
                _ => panic!("expected Short protection"),
            }
        }
        _ => panic!("expected SubChannels"),
    }
}

#[test]
fn decode_fig0_ext1_two_short_channels() {
    let body = vec![0x0Eu8, 0x00, 0x47, 0x28, 0x64, 0x03];
    match decode_fig0(1, false, &body) {
        Fig0Decoded::SubChannels(channels) => {
            assert_eq!(channels.len(), 2);
            assert_eq!(channels[0].id, 3);
            assert_eq!(channels[0].start_address, 512);
            match channels[0].protection {
                SubChannelProtection::Short { table_switch, table_index } => {
                    assert!(table_switch);
                    assert_eq!(table_index, 7);
                }
                _ => panic!("expected Short"),
            }
            assert_eq!(channels[1].id, 10);
            assert_eq!(channels[1].start_address, 100);
        }
        _ => panic!("expected SubChannels"),
    }
}

#[test]
fn multiplex_state_decodes_ensemble_info_from_fig0_snapshot() {
    let mut state = MultiplexState::new();
    let snapshot = SignallingSnapshot {
        fig_count: 1,
        type1_count: 0,
        type0_extensions: vec![Type0ExtensionSummary {
            extension: 0,
            count: 1,
            cn: false,
            oe: false,
            pd: false,
            last_body: vec![0xAB, 0xCD, 0x00, 0x01],
        }],
    };
    state.update(&snapshot, Some(5000));
    let info = state.ensemble_info.as_ref().expect("ensemble_info");
    assert_eq!(info.eid, 0xABCD);
    assert_eq!(info.change_flags, 0);
    assert!(!info.al);
    assert_eq!(info.cif_count_low, 1);
}

#[test]
fn multiplex_state_accumulates_sub_channels_across_frames() {
    let mut state = MultiplexState::new();
    let snap1 = SignallingSnapshot {
        fig_count: 1,
        type1_count: 0,
        type0_extensions: vec![Type0ExtensionSummary {
            extension: 1,
            count: 1,
            cn: false,
            oe: false,
            pd: false,
            last_body: vec![0x15, 0x2C, 0x0C],
        }],
    };
    let snap2 = SignallingSnapshot {
        fig_count: 1,
        type1_count: 0,
        type0_extensions: vec![Type0ExtensionSummary {
            extension: 1,
            count: 1,
            cn: false,
            oe: false,
            pd: false,
            last_body: vec![0x28, 0x64, 0x03],
        }],
    };
    state.update(&snap1, Some(1000));
    state.update(&snap2, Some(2000));
    assert_eq!(state.sub_channels.len(), 2);
    assert!(state.sub_channels.iter().any(|c| c.id == 5));
    assert!(state.sub_channels.iter().any(|c| c.id == 10));
}

#[test]
fn decode_fig0_ext2_parses_audio_service() {
    let body = vec![0x12u8, 0x34, 0x01, 0x00, 0x16];
    match decode_fig0(2, false, &body) {
        Fig0Decoded::Services(services) => {
            assert_eq!(services.len(), 1);
            assert_eq!(services[0].sid, 0x1234);
            assert_eq!(services[0].ca_id, 0);
            assert_eq!(services[0].components.len(), 1);
            assert_eq!(services[0].components[0].tm_id, 0);
            assert_eq!(services[0].components[0].type_id, 0);
            assert_eq!(services[0].components[0].sub_ch_id, 5);
            assert!(services[0].components[0].primary);
            assert!(!services[0].components[0].ca);
        }
        _ => panic!("expected Services"),
    }
}

#[test]
fn decode_fig0_ext2_parses_data_service_pd1() {
    let body = vec![0x12u8, 0x34, 0x56, 0x78, 0x41, 0x43, 0x1D];
    match decode_fig0(2, true, &body) {
        Fig0Decoded::Services(services) => {
            assert_eq!(services.len(), 1);
            assert_eq!(services[0].sid, 0x12345678);
            assert_eq!(services[0].ca_id, 2);
            assert_eq!(services[0].components.len(), 1);
            assert_eq!(services[0].components[0].tm_id, 1);
            assert_eq!(services[0].components[0].sub_ch_id, 7);
            assert!(!services[0].components[0].primary);
            assert!(services[0].components[0].ca);
        }
        _ => panic!("expected Services"),
    }
}

#[test]
fn multiplex_state_accumulates_services_from_fig02() {
    let mut state = MultiplexState::new();
    let snap = SignallingSnapshot {
        fig_count: 1,
        type1_count: 0,
        type0_extensions: vec![Type0ExtensionSummary {
            extension: 2,
            count: 1,
            cn: false,
            oe: false,
            pd: false,
            last_body: vec![0x12, 0x34, 0x01, 0x00, 0x16],
        }],
    };
    state.update(&snap, Some(3000));
    assert_eq!(state.services.len(), 1);
    assert_eq!(state.services[0].sid, 0x1234);
    assert_eq!(state.services[0].components.len(), 1);
    assert_eq!(state.services[0].components[0].sub_ch_id, 5);
}
