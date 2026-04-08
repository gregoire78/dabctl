/// End-to-end integration test: generates synthetic ETI frames, writes them to a file,
/// reads them back with EtiReader, and parses them with parse_eti_frame.
/// This validates the full ETI pipeline without requiring RTL-SDR hardware.
use dabctl::eti2pcm::crc::crc16_ccitt;
use dabctl::eti2pcm::eti_frame::{parse_eti_frame, ETI_FRAME_SIZE};
use dabctl::eti2pcm::eti_reader::EtiReader;

use std::io::Cursor;

/// Build a valid ETI frame with alternating FSYNC, FIC data, and subchannel streams.
fn build_eti_frame(
    frame_index: usize,
    nst: u8,
    mid: u8,
    streams: &[(u8, u16)],
) -> [u8; ETI_FRAME_SIZE] {
    let mut frame = [0u8; ETI_FRAME_SIZE];
    frame[0] = 0xFF; // ERR = valid

    // Alternate FSYNC between frames (required by eti2pcm)
    if frame_index % 2 == 0 {
        frame[1] = 0x07;
        frame[2] = 0x3A;
        frame[3] = 0xB6;
    } else {
        frame[1] = 0xF8;
        frame[2] = 0xC5;
        frame[3] = 0x49;
    }

    // FCT (Frame Counter)
    frame[4] = (frame_index & 0xFF) as u8;

    // FICF=1 | NST
    frame[5] = 0x80 | nst;

    // FIC length in 32-bit words
    let ficl: u16 = if mid == 3 { 32 } else { 24 };
    let total_stl: u16 = streams.iter().map(|(_, stl)| stl).sum();
    // FL = NST + 1 + FICL + sum(STL)*2, in 32-bit words
    let fl = nst as u16 + 1 + ficl + total_stl * 2;
    frame[6] = (mid << 3) | ((fl >> 8) as u8 & 0x07);
    frame[7] = fl as u8;

    // STC (Stream Table Config) — 4 bytes per stream
    for (i, (scid, stl)) in streams.iter().enumerate() {
        let base = 8 + i * 4;
        frame[base] = scid << 2;
        frame[base + 1] = 0;
        frame[base + 2] = ((stl >> 8) & 0x03) as u8;
        frame[base + 3] = *stl as u8;
    }

    // MNSC (2 bytes)
    let mnsc_offset = 8 + nst as usize * 4;
    frame[mnsc_offset] = 0x00;
    frame[mnsc_offset + 1] = 0x00;

    // Header CRC (over FC..MNSC)
    let header_crc_data_len = 4 + nst as usize * 4 + 2;
    let crc = crc16_ccitt();
    let header_crc = crc.calc(&frame[4..4 + header_crc_data_len]);
    let crc_offset = 4 + header_crc_data_len;
    frame[crc_offset] = (header_crc >> 8) as u8;
    frame[crc_offset + 1] = header_crc as u8;

    // MST CRC (over FIC + stream data)
    let data_start = 8 + nst as usize * 4 + 4;
    let mst_data_len = (fl as usize - nst as usize - 1) * 4;
    let mst_crc = crc.calc(&frame[data_start..data_start + mst_data_len]);
    let mst_crc_offset = data_start + mst_data_len;
    frame[mst_crc_offset] = (mst_crc >> 8) as u8;
    frame[mst_crc_offset + 1] = mst_crc as u8;

    frame
}

#[test]
fn e2e_generate_and_parse_eti_frames() {
    // Generate 20 alternating frames with 2 subchannels
    let num_frames = 20;
    let streams = vec![(3u8, 12u16), (7u8, 8u16)];
    let mut raw = Vec::with_capacity(ETI_FRAME_SIZE * num_frames);

    for i in 0..num_frames {
        let frame = build_eti_frame(i, 2, 1, &streams);
        raw.extend_from_slice(&frame);
    }

    assert_eq!(raw.len(), ETI_FRAME_SIZE * num_frames);

    // Read back with EtiReader
    let cursor = Cursor::new(&raw);
    let mut reader = EtiReader::new(cursor);

    let mut parsed_count = 0;
    let mut prev_fsync: u32 = 0;

    while let Ok(Some(frame_data)) = reader.next_frame() {
        let frame = parse_eti_frame(&frame_data).expect("Frame should parse successfully");

        // Validate header fields
        assert_eq!(frame.header.err, 0xFF);
        assert!(frame.header.fsync == 0x073AB6 || frame.header.fsync == 0xF8C549);
        assert!(frame.header.ficf);
        assert_eq!(frame.header.nst, 2);
        assert_eq!(frame.header.mid, 1);
        assert_eq!(frame.header.streams.len(), 2);
        assert_eq!(frame.header.streams[0].scid, 3);
        assert_eq!(frame.header.streams[0].stl, 12);
        assert_eq!(frame.header.streams[1].scid, 7);
        assert_eq!(frame.header.streams[1].stl, 8);

        // Verify FSYNC alternates
        if parsed_count > 0 {
            assert_ne!(
                frame.header.fsync, prev_fsync,
                "FSYNC should alternate between frames"
            );
        }
        prev_fsync = frame.header.fsync;

        // Verify FIC data is present
        assert!(
            !frame.fic_data.is_empty(),
            "FIC data should be present (FICF=1)"
        );
        // FIC length for mode 1 = 24 * 4 = 96 bytes
        assert_eq!(frame.fic_data.len(), 96);

        // Verify subchannel data extraction
        let sc3 = frame.subchannel_data(3);
        assert!(sc3.is_some(), "Subchannel 3 should be present");
        assert_eq!(sc3.unwrap().len(), 12 * 8); // STL * 8 bytes

        let sc7 = frame.subchannel_data(7);
        assert!(sc7.is_some(), "Subchannel 7 should be present");
        assert_eq!(sc7.unwrap().len(), 8 * 8);

        // Non-existent subchannel
        assert!(frame.subchannel_data(99).is_none());

        parsed_count += 1;
    }

    assert_eq!(parsed_count, num_frames, "All frames should be parsed");
}

#[test]
fn e2e_eti_reader_handles_garbage_and_resync() {
    let streams = vec![(5u8, 10u16)];
    let valid_frame = build_eti_frame(0, 1, 1, &streams);

    // Prepend garbage
    let mut data = vec![0xDE, 0xAD, 0xBE, 0xEF, 0x00, 0x11, 0x22, 0x33];
    data.extend_from_slice(&valid_frame);

    // Append a second frame
    let valid_frame2 = build_eti_frame(1, 1, 1, &streams);
    data.extend_from_slice(&valid_frame2);

    let cursor = Cursor::new(data);
    let mut reader = EtiReader::new(cursor);

    // First frame should be found despite garbage prefix
    let f1 = reader.next_frame().unwrap().unwrap();
    let parsed1 = parse_eti_frame(&f1).expect("First frame should parse");
    assert_eq!(parsed1.header.fsync, 0x073AB6);
    assert_eq!(parsed1.header.nst, 1);

    // Second frame with alternating FSYNC
    let f2 = reader.next_frame().unwrap().unwrap();
    let parsed2 = parse_eti_frame(&f2).expect("Second frame should parse");
    assert_eq!(parsed2.header.fsync, 0xF8C549);

    // EOF
    assert!(reader.next_frame().unwrap().is_none());
}

#[test]
fn e2e_eti_write_to_file_and_read_back() {
    use std::io::Write;

    let dir = std::env::temp_dir().join("eti-e2e-test");
    std::fs::create_dir_all(&dir).unwrap();
    let eti_path = dir.join("test-output.eti");

    let streams = vec![(1u8, 16u16)];
    let num_frames = 10;

    // Write ETI frames to file
    {
        let mut file = std::fs::File::create(&eti_path).unwrap();
        for i in 0..num_frames {
            let frame = build_eti_frame(i, 1, 1, &streams);
            file.write_all(&frame).unwrap();
        }
        file.flush().unwrap();
    }

    // Verify file size
    let metadata = std::fs::metadata(&eti_path).unwrap();
    assert_eq!(metadata.len(), (ETI_FRAME_SIZE * num_frames) as u64);

    // Read back and parse
    {
        let file = std::fs::File::open(&eti_path).unwrap();
        let mut reader = EtiReader::new(std::io::BufReader::new(file));
        let mut count = 0;
        while let Ok(Some(frame_data)) = reader.next_frame() {
            let frame = parse_eti_frame(&frame_data).expect("Frame from file should parse");
            assert_eq!(frame.header.streams[0].scid, 1);
            assert_eq!(frame.header.streams[0].stl, 16);
            count += 1;
        }
        assert_eq!(count, num_frames);
    }

    // Cleanup
    std::fs::remove_file(&eti_path).unwrap();
    let _ = std::fs::remove_dir(&dir);
}

#[test]
fn e2e_eti_frame_crc_integrity() {
    let crc = crc16_ccitt();
    let streams = vec![(2u8, 6u16)];
    let frame = build_eti_frame(0, 1, 1, &streams);

    // Manually verify header CRC
    let nst = 1usize;
    let header_crc_data_len = 4 + nst * 4 + 2;
    let crc_offset = 4 + header_crc_data_len;
    let stored_hcrc = (frame[crc_offset] as u16) << 8 | frame[crc_offset + 1] as u16;
    let calc_hcrc = crc.calc(&frame[4..4 + header_crc_data_len]);
    assert_eq!(stored_hcrc, calc_hcrc, "Header CRC should match");

    // Verify MST CRC
    let fl = ((frame[6] & 0x07) as u16) << 8 | frame[7] as u16;
    let data_start = 8 + nst * 4 + 4;
    let mst_data_len = (fl as usize - nst - 1) * 4;
    let mst_crc_offset = data_start + mst_data_len;
    let stored_mcrc = (frame[mst_crc_offset] as u16) << 8 | frame[mst_crc_offset + 1] as u16;
    let calc_mcrc = crc.calc(&frame[data_start..data_start + mst_data_len]);
    assert_eq!(stored_mcrc, calc_mcrc, "MST CRC should match");

    // Tamper with a byte and verify parse fails
    let mut bad_frame = frame;
    bad_frame[10] ^= 0xFF; // corrupt STC area
    assert!(
        parse_eti_frame(&bad_frame).is_none(),
        "Corrupted frame should not parse"
    );
}

#[test]
fn e2e_multiple_dab_modes() {
    // Test Mode 1 (mid=1): FIC = 24 * 4 bytes = 96
    let frame_m1 = build_eti_frame(0, 1, 1, &[(1, 8)]);
    let parsed1 = parse_eti_frame(&frame_m1).unwrap();
    assert_eq!(parsed1.header.mid, 1);
    assert_eq!(parsed1.fic_data.len(), 96);

    // Test Mode 2 (mid=2): FIC = 24 * 4 bytes = 96
    let frame_m2 = build_eti_frame(0, 1, 2, &[(1, 8)]);
    let parsed2 = parse_eti_frame(&frame_m2).unwrap();
    assert_eq!(parsed2.header.mid, 2);
    assert_eq!(parsed2.fic_data.len(), 96);

    // Test Mode 3 (mid=3): FIC = 32 * 4 bytes = 128
    let frame_m3 = build_eti_frame(0, 1, 3, &[(1, 8)]);
    let parsed3 = parse_eti_frame(&frame_m3).unwrap();
    assert_eq!(parsed3.header.mid, 3);
    assert_eq!(parsed3.fic_data.len(), 128);
}
