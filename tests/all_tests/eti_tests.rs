use super::*;

#[test]
fn eti_frame_builder_produces_valid_structure() {
    let mut builder = EtiFrameBuilder::new();
    let state = MultiplexState::new();
    let frame = builder.build_frame(&state, None);
    assert_eq!(frame.len(), ETI_FRAME_BYTES);
    assert_eq!(&frame[0..4], &ETI_SYNC_EVEN);
    assert_eq!(frame[4], 0);
    assert_eq!(frame[5], 0x80);
    assert_eq!(frame[6], 0x0D);
    assert_eq!(frame[7], 0xFE);
    assert_eq!(&frame[ETI_FRAME_BYTES - 4..], &[0xFF, 0xFF, 0xFF, 0x00]);
}

#[test]
fn eti_frame_builder_eoh_crc_valid() {
    let mut builder = EtiFrameBuilder::new();
    let state = MultiplexState::new();
    let frame = builder.build_frame(&state, None);
    let expected = crc16_ccitt_bytes(&frame[4..10]);
    let actual = ((frame[10] as u16) << 8) | frame[11] as u16;
    assert_eq!(actual, expected);
}

#[test]
fn eti_frame_builder_eof_crc_valid() {
    let mut builder = EtiFrameBuilder::new();
    let state = MultiplexState::new();
    let frame = builder.build_frame(&state, None);
    let expected = crc16_ccitt_bytes(&frame[12..6136]);
    let actual = ((frame[6136] as u16) << 8) | frame[6137] as u16;
    assert_eq!(actual, expected);
}

#[test]
fn eti_frame_builder_fsync_alternates() {
    let mut builder = EtiFrameBuilder::new();
    let state = MultiplexState::new();
    let f0 = builder.build_frame(&state, None);
    let f1 = builder.build_frame(&state, None);
    assert_eq!(&f0[0..4], &ETI_SYNC_EVEN);
    assert_eq!(&f1[0..4], &ETI_SYNC_ODD);
}

#[test]
fn eti_frame_builder_embeds_fic_bytes() {
    let mut builder = EtiFrameBuilder::new();
    let state = MultiplexState::new();
    let fic = vec![0xABu8; ETI_FIC_BYTES];
    let frame = builder.build_frame(&state, Some(&fic));
    assert_eq!(&frame[12..12 + ETI_FIC_BYTES], fic.as_slice());
}

#[test]
fn pipeline_eti_mode_emits_frame_without_fib() {
    let mut pipeline = DabPipeline::new(PipelineMode::Eti);
    let chunk = vec![128u8; 4096];
    let mut out = Vec::new();

    pipeline.process_chunk(&chunk, &mut out).expect("process");

    assert_eq!(out.len(), ETI_FRAME_BYTES);
    assert_eq!(pipeline.last_report().eti_frames_built, 0);
    assert_eq!(pipeline.last_report().eti_frames_emitted, 1);
    assert!(!pipeline.last_report().eti_fic_cache_valid);
}

#[test]
fn pipeline_eti_mode_emission_counter_accumulates() {
    let mut pipeline = DabPipeline::new(PipelineMode::Eti);
    let chunk = vec![128u8; 4096];
    let mut out = Vec::new();

    // process_chunk fait out.clear() en interne : out ne contient que la dernière trame
    pipeline.process_chunk(&chunk, &mut out).expect("process first");
    assert_eq!(out.len(), ETI_FRAME_BYTES);

    out.clear();
    pipeline.process_chunk(&chunk, &mut out).expect("process second");
    assert_eq!(out.len(), ETI_FRAME_BYTES);

    assert_eq!(pipeline.last_report().eti_frames_emitted, 2);
}
