# DABstar replacement tracker for dabctl

This file tracks the dabctl functions and processing stages that should be aligned with DABstar during the ongoing refactor.

## Status legend

- ✅ Done or already extracted in a DABstar-shaped form
- 🟡 Partially aligned, still needs parity review
- ⬜ To do
- 🚫 Keep as a native Rust wrapper, do not replace directly

## Recommended execution order

1. OFDM synchronisation and demodulation
2. FIC and FIG parsing completeness
3. MSC protection and DAB+ superframe parity
4. PAD and MOT slideshow edge cases
5. Runtime polish and diagnostics

---

## 1. OFDM synchronisation and demodulation

Primary source area in DABstar:
- TimeSyncer::read_samples_until_end_of_level_drop
- PhaseReference::correlate_with_phase_ref_and_find_max_peak
- PhaseReference::estimate_carrier_offset_from_sync_symbol_0
- OfdmDecoder::store_reference_symbol_0
- OfdmDecoder::decode_symbol
- OfdmDecoder::_compute_noise_Power
- OfdmDecoder::_eval_null_symbol_statistics

Target file:
- [src/pipeline/ofdm/ofdm_processor.rs](src/pipeline/ofdm/ofdm_processor.rs)

| Status | dabctl function | DABstar reference | Action to track |
|---|---|---|---|
| ✅ | SyncLoopControl::new | TimeSyncer plus OfdmDecoder state flow | Extracted for clearer acquisition and tracking control |
| ✅ | SyncLoopControl::on_time_sync_established | TimeSyncer plus PRS lock transition | Keep and verify live RF parity |
| ✅ | SyncLoopControl::on_frame_processed | OfdmDecoder tracking loop | Keep and verify live RF parity |
| ✅ | SyncLoopControl::on_sync_lost | DABstar reacquisition path | Keep current immediate reacquisition behaviour |
| ✅ | wait_for_time_sync_marker | TimeSyncer::read_samples_until_end_of_level_drop | Refactored to a pure follow-up plan and reverified with regression tests |
| ✅ | eval_sync_symbol | PhaseReference::correlate_with_phase_ref_and_find_max_peak | Refactored to explicit alignment planning with reuse-budget tests |
| ✅ | update_coarse_frequency_from_sync_symbol_0 | PhaseReference::estimate_carrier_offset_from_sync_symbol_0 | Refactored to explicit coarse-correction planning and reverified |
| ✅ | update_fine_frequency_from_cyclic_prefix | OfdmDecoder tracking correction | CP coherence gating is isolated and covered by regression tests |
| ✅ | process_null_symbol | OfdmDecoder::store_null_symbol_without_tii and _eval_null_symbol_statistics | Refactored to explicit telemetry planning with finite-SNR guard |
| ✅ | store_reference_symbol_0 | OfdmDecoder::store_reference_symbol_0 | Reference-symbol capture now has direct regression coverage |
| ✅ | process_block | OfdmDecoder::decode_symbol | Soft-bit weighting and scaling are now extracted and reverified |
| ✅ | process_ofdm_symbols_1_to_l | OfdmDecoder symbol loop | Split into explicit per-symbol read and correlation helpers and reverified |
| ✅ | process_frame_rest | OfdmDecoder frame runner | Final per-frame follow-up is now decomposed and verified |
| ✅ | run | SampleReader plus TimeSyncer plus OfdmDecoder orchestration | State-machine orchestration is now decomposed and verified by the OFDM suite |

---

## 2. OFDM block sequencing, FIC buffering, and CIF assembly

Primary source area in DABstar:
- FicDecoder::process_block
- Backend::process
- BackendDriver::add_to_frame
- MscHandler::process_block

Target file:
- [src/pipeline/dab_pipeline.rs](src/pipeline/dab_pipeline.rs)

| Status | dabctl function | DABstar reference | Action to track |
|---|---|---|---|
| ✅ | OfdmFrameSync::advance | Backend frame continuity logic | Already extracted and tested |
| ✅ | FicFrameAssembler::store_block | FicDecoder::process_block | Already extracted and tested |
| ✅ | FicFrameAssembler::decode_slots | FicDecoder FIC slot decode path | Already extracted and tested |
| ✅ | CifAssembler::store_msc_block | MscHandler::process_block | Already extracted and tested |
| ✅ | CifAssembler::finish_loaded_cif | Backend CIF completion path | Already extracted and tested |
| ✅ | CifAssembler::note_sync_loss | Backend resync propagation | Already extracted and tested |
| ✅ | run_loop | Backend and MscHandler orchestration | Split into focused FIC and CIF helpers and verified by tests |
| ✅ | process_cif_to_frames | BackendDeconvolver::deconvolve | CIF-to-subchannel decode is now split into explicit layout, PRBS descrambler, and per-subchannel stages and verified by regression tests |
| ✅ | adjust_cif_counter | FibDecoder::get_cif_count usage | ETSI-safe counter handling is covered and verified |
| 🚫 | send_frame_to_consumer | dabctl-specific Rust channel wrapper | Keep native, not a DABstar replacement target |

---

## 3. FIC, FIG, ensemble, and service discovery

Primary source area in DABstar:
- FibDecoder::process_FIB
- FibDecoder::_process_Fig0
- FibDecoder::_process_Fig1
- FibDecoder::get_data_for_audio_service
- FibDecoder::get_service_label_from_SId_SCIdS

Target file:
- [src/audio/fic_decoder.rs](src/audio/fic_decoder.rs)

| Status | dabctl function | DABstar reference | Action to track |
|---|---|---|---|
| ✅ | FicHandler::process_block | FicDecoder::process_block | The FIC path now follows the explicit collect → depuncture → Viterbi → descramble staging used by DABstar and is verified |
| ✅ | FicHandler::process_fic_input | FicDecoder::_process_fic_input | Per-FIC decode staging and validity propagation are now isolated and verified |
| ✅ | FicHandler::get_fib_bits | FicDecoder::get_fib_bits | Snapshot export of the decoded FIC frame is now available in the DABstar shape and verified |
| ✅ | FibProcessor::process_fib | FibDecoder::process_FIB | FIB chunking and end-to-end dispatch are now refactored and verified |
| ✅ | process_fig0 | FibDecoder::_process_Fig0 | Structured dispatch plus subtype parsing is now refactored and verified |
| ✅ | process_fig0_0 | FibDecoder::_process_Fig0s0 | Ensemble update path now preserves existing labels and is verified |
| ✅ | process_fig0_1 | FibDecoder::_subprocess_Fig0s1 | Subchannel organisation parsing is now extracted and verified |
| ✅ | process_fig0_2 | FibDecoder::_subprocess_Fig0s2 | Service component mapping parsing is now extracted and verified |
| ✅ | process_fig1 | FibDecoder::_process_Fig1 | Label dispatch is now structured and covered by regression tests |
| ✅ | parse_fig_header | FibDecoder::_get_fig_header | Explicit FIG header extraction now drives DABstar-style dispatch and is regression-tested |
| ✅ | process_fig1_0 | FibDecoder::_process_Fig1s0 | Ensemble label update path cleaned and verified |
| ✅ | process_fig1_1 | FibDecoder::_process_Fig1s1 | Service label upsert path cleaned and verified |
| ✅ | find_audio_service | FibDecoder::get_data_for_audio_service | Deterministic fallback selection is now verified |
| ✅ | find_service_by_label | FibDecoder::get_SId_SCIdS_from_service_label | Normalized label lookup is now verified |

---

## 4. Protection, Reed-Solomon, and DAB+ superframe path

Primary source area in DABstar:
- Protection::deconvolve
- EepProtection::_extract_viterbi_block_addresses
- UepProtection::_extract_viterbi_block_addresses
- ReedSolomon::decode_rs
- FirecodeChecker::check
- Mp4Processor::add_to_frame
- Mp4Processor::_process_super_frame
- Mp4Processor::_build_aac_stream

Target files:
- [src/pipeline/viterbi_handler.rs](src/pipeline/viterbi_handler.rs)
- [src/pipeline/protection.rs](src/pipeline/protection.rs)
- [src/audio/rs_decoder.rs](src/audio/rs_decoder.rs)
- [src/audio/superframe.rs](src/audio/superframe.rs)
- [src/audio/audio_runtime.rs](src/audio/audio_runtime.rs)

| Status | dabctl function | DABstar reference | Action to track |
|---|---|---|---|
| ✅ | ViterbiSpiral::deconvolve | ViterbiSpiral::deconvolve | Refactored to the DABstar spiral layout with rate-major branch tables, packed survivor decisions, and explicit chainback helpers; verified |
| ✅ | ViterbiSpiral::calculate_ber | ViterbiSpiral::calculate_BER | BER helper parity is preserved and covered by dedicated regression tests |
| ✅ | EepProtection::new | EepProtection constructor | Profile planning is now isolated into explicit ETSI-safe segment resolution and verified by regressions |
| ✅ | EepProtection::deconvolve | Protection::deconvolve | Shared depuncture and Viterbi staging are now extracted and reverified |
| ✅ | UepProtection::new | UepProtection constructor | Same-bitrate fallback and puncture-table assembly are now explicit and regression-tested |
| ✅ | UepProtection::deconvolve | Protection::deconvolve | Shared depuncture path now covers empty-profile and fallback edge cases |
| ✅ | RsDecoder::decode_superframe | ReedSolomon::decode_rs | Interleaved column staging is now isolated and full correction-count parity is verified |
| ✅ | SuperframeFilter::feed | Mp4Processor::add_to_frame and _process_super_frame | Rolling-window staging and decode flow are now decomposed and reverified |
| ✅ | SuperframeFilter::parse_format | Mp4Processor format parser | Header-bit interpretation is now isolated and regression-tested |
| ✅ | SuperframeFilter::compute_au_starts | Mp4Processor AU boundary builder | AU offset extraction and sanitising are now isolated and verified |
| ✅ | SuperframeFilter::reset | DABstar superframe reset on loss | Reset path remains test-covered and stable |
| ✅ | AudioFrameProcessor::process_selected_subchannel | Mp4Processor main ingest path | Refactored into smaller follow-up/silence helpers and reverified |
| ✅ | AudioFrameProcessor::configure_decoder_if_needed | faadDecoder::initialize and FdkAAC::convert_mp4_to_pcm | Now avoids redundant decoder rebuilds and keeps format tracking explicit |
| ✅ | AudioFrameProcessor::decode_access_units | faadDecoder::convert_mp4_to_pcm | Isolated and reverified with follow-up planning tests |

---

## 5. PAD, DLS, and MOT slideshow metadata

Primary source area in DABstar:
- PadHandler::process_PAD
- PadHandler::_handle_variable_PAD
- PadHandler::_dynamic_label
- MotHandler::add_MSC_data_group
- MotObject::set_header
- MotObject::add_body_segment

Target files:
- [src/audio/pad_decoder.rs](src/audio/pad_decoder.rs)
- [src/audio/mot_decoder.rs](src/audio/mot_decoder.rs)
- [src/audio/mot_manager.rs](src/audio/mot_manager.rs)

| Status | dabctl function | DABstar reference | Action to track |
|---|---|---|---|
| ✅ | PadDecoder::process | PadHandler::process_PAD | PAD routing now delegates through explicit CI parsing and verified continuation handling |
| ✅ | PadDecoder::process_full | PadHandler variable PAD path | Full XPAD walk is now split into reversal, CI collection, and subfield dispatch stages and regression-tested |
| ✅ | decode_label_text | DABstar charset helpers | Charset sanitation and UTF handling are now isolated and verified by direct label-decoding regressions |
| ✅ | MotDecoder::process_subfield | MotHandler::add_MSC_data_group | Start validation, accumulation, and CRC completion checks are now explicit and reverified |
| ✅ | MotDecoder::get_data_group | MotHandler object handoff | Incomplete or CRC-unsafe groups are no longer handed off and this is regression-tested |
| ✅ | MotManager::handle_data_group | MotHandler and MotObject integration | Transport routing now flows through an explicit parsed group context and completion logic is reverified |
| ✅ | MotManager::parse_dg_header | MotDirectory and MotObject parser | Required-flag validation is now directly unit-tested |
| ✅ | MotManager::parse_session_header | MotObject session handling | Session metadata extraction is now covered through parsed transport-group regressions |
| ✅ | MotManager::parse_segmentation_header | MotObject segment extraction | Segment-size and CRC-guarded payload extraction remain verified through direct MOT tests |

---

## 6. Native Rust wrappers to keep

These functions are useful orchestration wrappers for dabctl and should remain Rust-native even if their internals are informed by DABstar:

- [src/audio/audio_runtime.rs](src/audio/audio_runtime.rs) — spawn_status_thread
- [src/audio/audio_runtime.rs](src/audio/audio_runtime.rs) — snapshot_and_reset
- [src/iq2pcm_cmd.rs](src/iq2pcm_cmd.rs) — run
- [src/pipeline/dab_pipeline.rs](src/pipeline/dab_pipeline.rs) — send_frame_to_consumer

---

## Short next-step checklist

- [x] Finish parity review of OFDM DSP helpers in the OFDM processor
- [x] Complete FIG 0 and FIG 1 comparison against DABstar
- [x] Verify DAB+ superframe and AU boundary parity
- [ ] Review PAD and MOT edge cases with real captures
- [ ] Mark each row above as done only after live verification or passing regression tests
