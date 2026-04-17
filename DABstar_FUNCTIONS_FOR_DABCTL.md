# DABstar functions relevant for dabctl

This file isolates the DABstar symbols that are actually useful for the current dabctl scope: direct RTL-SDR to OFDM to FIC or MSC to DAB+ AAC to PCM audio, with DLS and slideshow metadata.

## Selection policy

- Selected DABstar files: 88
- Selected DABstar functions or methods: 826
- Kept: OFDM sync and demod, FIC and FIG parsing, protection and Viterbi related code, DAB+ audio path, PAD and MOT metadata.
- De-prioritized: Qt GUI, audio output UI, hardware-specific device wrappers, old legacy files, ETI tooling, and other non-runtime helpers.
- Out of current scope: classic DAB MP2 playback, since dabctl currently targets DAB+ only.

## Priority 1 — OFDM synchronisation and demodulation

These are the closest DABstar references for the current dabctl live RF risk area in the Rust OFDM path.

- DABstar files kept here: 14
- DABstar functions or methods in this group: 126
- Closest dabctl Rust modules:
  - src/pipeline/ofdm/time_syncer.rs
  - src/pipeline/ofdm/phase_reference.rs
  - src/pipeline/ofdm/phase_table.rs
  - src/pipeline/ofdm/freq_interleaver.rs
  - src/pipeline/ofdm/ofdm_processor.rs

### `ofdm/timesyncer.cpp`

- `TimeSyncer::TimeSyncer`
- `TimeSyncer::read_samples_until_end_of_level_drop`

### `ofdm/timesyncer.h`

- `TimeSyncer`
- `~TimeSyncer`
- `read_samples_until_end_of_level_drop`

### `ofdm/phasereference.cpp`

- `PhaseReference::PhaseReference`
- `PhaseReference::~PhaseReference`
- `PhaseReference::correlate_with_phase_ref_and_find_max_peak`
- `PhaseReference::estimate_carrier_offset_from_sync_symbol_0`
- `PhaseReference::set_sync_on_strongest_peak`
- `PhaseReference::CalculateRelativePhase`

### `ofdm/phasereference.h`

- `PhaseReference`
- `~PhaseReference`
- `correlate_with_phase_ref_and_find_max_peak`
- `estimate_carrier_offset_from_sync_symbol_0`
- `phase`
- `set_sync_on_strongest_peak`
- `CalculateRelativePhase`
- `signal_show_correlation`

### `ofdm/phasetable.cpp`

- `PhaseTable::PhaseTable`
- `PhaseTable::h_table`
- `PhaseTable::get_phi`

### `ofdm/phasetable.h`

- `PhaseTable`
- `~PhaseTable`
- `h_table`
- `get_phi`

### `ofdm/freq-interleaver.cpp`

- `FreqInterleaver::FreqInterleaver`
- `FreqInterleaver::createMapper`

### `ofdm/freq-interleaver.h`

- `FreqInterleaver`
- `~FreqInterleaver`
- `map_k_to_fft_bin`
- `createMapper`

### `ofdm/ofdm-decoder.cpp`

- `OfdmDecoder::OfdmDecoder`
- `OfdmDecoder::~OfdmDecoder`
- `OfdmDecoder::cmplx_from_phase2`
- `OfdmDecoder::reset`
- `OfdmDecoder::store_null_symbol_with_tii`
- `OfdmDecoder::store_null_symbol_without_tii`
- `OfdmDecoder::store_reference_symbol_0`
- `OfdmDecoder::decode_symbol`
- `OfdmDecoder::_compute_noise_Power`
- `OfdmDecoder::_eval_null_symbol_statistics`
- `OfdmDecoder::_reset_null_symbol_statistics`
- `OfdmDecoder::set_select_carrier_plot_type`
- `OfdmDecoder::set_select_iq_plot_type`
- `OfdmDecoder::set_soft_bit_gen_type`
- `OfdmDecoder::set_show_nominal_carrier`
- `OfdmDecoder::_interpolate_2d_plane`

### `ofdm/ofdm-decoder.h`

- `OfdmDecoder`
- `~OfdmDecoder`
- `reset`
- `store_null_symbol_with_tii`
- `store_null_symbol_without_tii`
- `store_reference_symbol_0`
- `decode_symbol`
- `set_select_carrier_plot_type`
- `set_select_iq_plot_type`
- `set_soft_bit_gen_type`
- `set_show_nominal_carrier`
- `set_dc_offset`
- `_compute_noise_Power`
- `_eval_null_symbol_statistics`
- `_reset_null_symbol_statistics`
- `cmplx_from_phase2`
- `_interpolate_2d_plane`
- `signal_slot_show_iq`
- `signal_show_lcd_data`

### `ofdm/ofdm-decoder-simd.cpp`

- `OfdmDecoder::reset`
- `OfdmDecoder::store_null_symbol_with_tii`
- `OfdmDecoder::store_null_symbol_without_tii`
- `OfdmDecoder::store_reference_symbol_0`
- `OfdmDecoder::decode_symbol`
- `OfdmDecoder::_compute_noise_Power`
- `OfdmDecoder::_eval_null_symbol_statistics`
- `OfdmDecoder::_reset_null_symbol_statistics`
- `OfdmDecoder::set_select_carrier_plot_type`
- `OfdmDecoder::set_select_iq_plot_type`
- `OfdmDecoder::set_soft_bit_gen_type`
- `OfdmDecoder::set_show_nominal_carrier`
- `OfdmDecoder::_interpolate_2d_plane`
- `OfdmDecoder::_display_iq_and_carr_vectors`

### `ofdm/ofdm-decoder-simd.h`

- `OfdmDecoder`
- `~OfdmDecoder`
- `reset`
- `store_null_symbol_with_tii`
- `store_null_symbol_without_tii`
- `store_reference_symbol_0`
- `decode_symbol`
- `set_select_carrier_plot_type`
- `set_select_iq_plot_type`
- `set_soft_bit_gen_type`
- `set_show_nominal_carrier`
- `set_dc_offset`
- `_compute_noise_Power`
- `_eval_null_symbol_statistics`
- `_reset_null_symbol_statistics`
- `_display_iq_and_carr_vectors`
- `_interpolate_2d_plane`
- `signal_slot_show_iq`
- `signal_show_lcd_data`

### `ofdm/sample-reader.cpp`

- `SampleReader::SampleReader`
- `SampleReader::setRunning`
- `SampleReader::get_sLevel`
- `SampleReader::getSample`
- `SampleReader::getSamples`
- `SampleReader::_dump_samples_to_file`
- `SampleReader::startDumping`
- `SampleReader::stop_dumping`
- `SampleReader::get_linear_peak_level_and_clear`
- `SampleReader::set_dc_and_iq_correction`
- `SampleReader::set_cir_buffer`

### `ofdm/sample-reader.h`

- `SampleReader`
- `~SampleReader`
- `setRunning`
- `get_sLevel`
- `get_linear_peak_level_and_clear`
- `getSample`
- `getSamples`
- `startDumping`
- `stop_dumping`
- `set_dc_and_iq_correction`
- `set_cir_buffer`
- `get_dc_offset`
- `_dump_samples_to_file`
- `signal_show_spectrum`
- `signal_show_cir`

## Priority 2 — FIC, FIB and service discovery

These functions map to ensemble detection, FIG parsing, service lookup and sub-channel selection in dabctl.

- DABstar files kept here: 14
- DABstar functions or methods in this group: 267
- Closest dabctl Rust modules:
  - src/pipeline/fic_handler.rs
  - src/pipeline/fib_processor.rs
  - src/audio/fic_decoder.rs
  - src/pipeline/subchannel_pool.rs

### `decoder/fib-config-fig0.cpp`

- `FibConfigFig0::FibConfigFig0`
- `FibConfigFig0::reset`
- `FibConfigFig0::get_Fig0s1_BasicSubChannelOrganization_of_SubChId`
- `FibConfigFig0::get_Fig0s2_BasicService_ServiceCompDef_of_SId`
- `FibConfigFig0::get_Fig0s2_BasicService_ServiceCompDef_of_SId_ScIdx`
- `FibConfigFig0::get_Fig0s2_BasicService_ServiceCompDef_of_SCId`
- `FibConfigFig0::get_Fig0s2_BasicService_ServiceCompDef_of_SId_TMId`
- `FibConfigFig0::get_Fig0s3_ServiceComponentPacketMode_of_SCId`
- `FibConfigFig0::get_Fig0s5_ServiceComponentLanguage_of_SubChId`
- `FibConfigFig0::get_Fig0s5_ServiceComponentLanguage_of_SCId`
- `FibConfigFig0::get_Fig0s7_ConfigurationInformation`
- `FibConfigFig0::get_Fig0s8_ServiceCompGlobalDef_of_SId_SCIdS`
- `FibConfigFig0::get_Fig0s8_ServiceCompGlobalDef_of_SId_with_SubChId`
- `FibConfigFig0::get_Fig0s8_ServiceCompGlobalDef_of_SId_with_SCId`
- `FibConfigFig0::get_Fig0s9_CountryLtoInterTab`
- `FibConfigFig0::get_Fig0s13_UserApplicationInformation_of_SId_SCIdS`
- `FibConfigFig0::get_Fig0s14_SubChannelOrganization_of_SubChId`
- `FibConfigFig0::get_Fig0s17_ProgrammeType_of_SId`
- `FibConfigFig0::print_Fig0s1_BasicSubChannelOrganization`
- `FibConfigFig0::print_Fig0s2_BasicService_ServiceCompDef`
- `FibConfigFig0::print_Fig0s3_ServiceComponentPacketMode`
- `FibConfigFig0::print_Fig0s5_ServiceComponentLanguage`
- `FibConfigFig0::print_Fig0s7_ConfigurationInformation`
- `FibConfigFig0::print_Fig0s8_ServiceCompGlobalDef`
- `FibConfigFig0::print_Fig0s9_CountryLtoInterTab`
- `FibConfigFig0::print_Fig0s13_UserApplicationInformation`
- `FibConfigFig0::print_Fig0s14_SubChannelOrganization`
- `FibConfigFig0::print_Fig0s17_ProgrammeType`

### `decoder/fib-config-fig0.h`

- `FibConfigFig0`
- `~FibConfigFig0`
- `reset`
- `get_SId`
- `get_Fig0s1_BasicSubChannelOrganization_of_SubChId`
- `get_Fig0s2_BasicService_ServiceCompDef_of_SId_TMId`
- `get_Fig0s2_BasicService_ServiceCompDef_of_SId`
- `get_Fig0s2_BasicService_ServiceCompDef_of_SId_ScIdx`
- `get_Fig0s2_BasicService_ServiceCompDef_of_SCId`
- `get_Fig0s3_ServiceComponentPacketMode_of_SCId`
- `get_Fig0s5_ServiceComponentLanguage_of_SubChId`
- `get_Fig0s5_ServiceComponentLanguage_of_SCId`
- `get_Fig0s7_ConfigurationInformation`
- `get_Fig0s8_ServiceCompGlobalDef_of_SId_with_SubChId`
- `get_Fig0s8_ServiceCompGlobalDef_of_SId_with_SCId`
- `get_Fig0s8_ServiceCompGlobalDef_of_SId_SCIdS`
- `get_Fig0s9_CountryLtoInterTab`
- `get_Fig0s13_UserApplicationInformation_of_SId_SCIdS`
- `get_Fig0s14_SubChannelOrganization_of_SubChId`
- `get_Fig0s17_ProgrammeType_of_SId`
- `print_Fig0s1_BasicSubChannelOrganization`
- `print_Fig0s2_BasicService_ServiceCompDef`
- `print_Fig0s3_ServiceComponentPacketMode`
- `print_Fig0s5_ServiceComponentLanguage`
- `print_Fig0s7_ConfigurationInformation`
- `print_Fig0s8_ServiceCompGlobalDef`
- `print_Fig0s9_CountryLtoInterTab`
- `print_Fig0s13_UserApplicationInformation`
- `print_Fig0s14_SubChannelOrganization`
- `print_Fig0s17_ProgrammeType`
- `hex_str`

### `decoder/fib-config-fig1.cpp`

- `FibConfigFig1::FibConfigFig1`
- `FibConfigFig1::reset`
- `FibConfigFig1::get_Fig1s1_ProgrammeServiceLabel_of_SId`
- `FibConfigFig1::get_Fig1s4_ServiceComponentLabel_of_SId_SCIdS`
- `FibConfigFig1::get_Fig1s5_DataServiceLabel_of_SId`
- `FibConfigFig1::get_service_label_of_SId_from_all_Fig1`
- `FibConfigFig1::get_SId_SCIdS_from_service_label`
- `FibConfigFig1::print_Fig1s0_EnsembleLabel`
- `FibConfigFig1::print_Fig1s1_ProgrammeServiceLabelVec`
- `FibConfigFig1::print_Fig1s4_ServiceComponentLabel`
- `FibConfigFig1::print_Fig1s5_DataServiceLabel`

### `decoder/fib-config-fig1.h`

- `FibConfigFig1`
- `~FibConfigFig1`
- `reset`
- `hex_str`
- `get_Fig1s4_ServiceComponentLabel_of_SId_SCIdS`
- `get_Fig1s5_DataServiceLabel_of_SId`
- `get_service_label_of_SId_from_all_Fig1`
- `get_SId_SCIdS_from_service_label`
- `print_Fig1s0_EnsembleLabel`
- `print_Fig1s1_ProgrammeServiceLabelVec`
- `print_Fig1s4_ServiceComponentLabel`
- `print_Fig1s5_DataServiceLabel`

### `decoder/fib-decoder.cpp`

- `FibDecoderFactory::create`
- `FibDecoder::FibDecoder`
- `FibDecoder::process_FIB`
- `FibDecoder::_reset`
- `FibDecoder::connect_channel`
- `FibDecoder::disconnect_channel`
- `FibDecoder::set_SId_for_fast_audio_selection`
- `FibDecoder::get_data_for_audio_service`
- `FibDecoder::get_data_for_audio_service_addon`
- `FibDecoder::_get_data_for_audio_service`
- `FibDecoder::_get_data_for_audio_service_addon`
- `FibDecoder::get_data_for_packet_service`
- `FibDecoder::_get_data_for_packet_service`
- `FibDecoder::get_service_list`
- `FibDecoder::get_service_label_from_SId_SCIdS`
- `FibDecoder::get_SId_SCIdS_from_service_label`
- `FibDecoder::get_ensembleId`
- `FibDecoder::get_ensemble_name`
- `FibDecoder::get_sub_channel_id_list`
- `FibDecoder::get_cif_count`
- `FibDecoder::get_ecc`
- `FibDecoder::set_epg_data`
- `FibDecoder::get_timeTable`
- `FibDecoder::has_time_table`
- `FibDecoder::find_epg_data`
- `FibDecoder::get_mod_julian_date`
- `FibDecoder::get_sub_channel_info`
- `FibDecoder::_set_cluster`
- `FibDecoder::_get_cluster`
- `FibDecoder::_extract_character_set_label`
- `FibDecoder::_retrigger_timer_data_loaded_fast`
- `FibDecoder::_retrigger_timer_data_loaded_slow`
- `FibDecoder::_process_fast_audio_selection`
- `FibDecoder::_check_audio_data_completeness`
- `FibDecoder::_check_packet_data_completeness`
- `FibDecoder::_slot_timer_data_consistency_check`
- `FibDecoder::_slot_timer_check_state_and_print_FIGs`
- `FibDecoder::_get_fig_header`

### `decoder/fib-decoder.h`

- `FibDecoder`
- `~FibDecoder`
- `process_FIB`
- `connect_channel`
- `disconnect_channel`
- `set_SId_for_fast_audio_selection`
- `get_data_for_audio_service`
- `get_data_for_audio_service_addon`
- `get_data_for_packet_service`
- `get_service_list`
- `get_service_label_from_SId_SCIdS`
- `get_SId_SCIdS_from_service_label`
- `get_ecc`
- `get_ensembleId`
- `get_ensemble_name`
- `get_sub_channel_id_list`
- `get_sub_channel_info`
- `get_cif_count`
- `get_mod_julian_date`
- `get_fib_content_str_list`
- `_reset`
- `_get_config_ptr`
- `_process_Fig0`
- `_process_Fig1`
- `_process_fig0_loop`
- `_process_Fig0s0`
- `_subprocess_Fig0s1`
- `_subprocess_Fig0s2`
- `_subprocess_Fig0s3`
- `_subprocess_Fig0s5`
- `_process_Fig0s7`
- `_subprocess_Fig0s8`
- `_process_Fig0s9`
- `_process_Fig0s10`
- `_subprocess_Fig0s13`
- `_subprocess_Fig0s14`
- `_process_Fig0s17`
- `_process_Fig0s18`
- `_process_Fig0s19`
- `_subprocess_Fig0s21`
- `_process_Fig1s0`
- `_process_Fig1s1`
- `_process_Fig1s4`
- `_process_Fig1s5`
- `_set_cluster`
- `_get_cluster`
- `_get_data_for_audio_service`
- `_get_data_for_audio_service_addon`
- `_get_data_for_packet_service`
- `_get_audio_data_str`
- `_get_packet_data_str`
- `_extract_character_set_label`
- `_retrigger_timer_data_loaded_fast`
- `_retrigger_timer_data_loaded_slow`
- `_process_fast_audio_selection`
- `_check_audio_data_completeness`
- `_check_packet_data_completeness`
- `hex_str`
- `_slot_timer_check_state_and_print_FIGs`

### `decoder/fib-decoder-fig0.cpp`

- `FibDecoder::_process_Fig0`
- `FibDecoder::_process_fig0_loop`
- `FibDecoder::_process_Fig0s0`
- `FibDecoder::_subprocess_Fig0s1`
- `FibDecoder::_subprocess_Fig0s2`
- `FibDecoder::_subprocess_Fig0s3`
- `FibDecoder::_subprocess_Fig0s5`
- `FibDecoder::_process_Fig0s7`
- `FibDecoder::_subprocess_Fig0s8`
- `FibDecoder::_process_Fig0s9`
- `FibDecoder::_process_Fig0s10`
- `FibDecoder::_subprocess_Fig0s13`
- `FibDecoder::_subprocess_Fig0s14`
- `FibDecoder::_process_Fig0s17`
- `FibDecoder::_process_Fig0s18`
- `FibDecoder::_process_Fig0s19`
- `FibDecoder::_subprocess_Fig0s21`

### `decoder/fib-decoder-fig1.cpp`

- `FibDecoder::_process_Fig1`
- `FibDecoder::_process_Fig1s0`
- `FibDecoder::_process_Fig1s1`
- `FibDecoder::_process_Fig1s4`
- `FibDecoder::_process_Fig1s5`

### `decoder/fib-decoder_if.h`

- `~IFibDecoder`
- `process_FIB`
- `connect_channel`
- `disconnect_channel`
- `set_SId_for_fast_audio_selection`
- `get_data_for_audio_service`
- `get_data_for_audio_service_addon`
- `get_data_for_packet_service`
- `get_service_list`
- `get_service_label_from_SId_SCIdS`
- `get_SId_SCIdS_from_service_label`
- `get_ecc`
- `get_ensembleId`
- `get_ensemble_name`
- `get_sub_channel_id_list`
- `get_sub_channel_info`
- `get_cif_count`
- `get_mod_julian_date`
- `get_fib_content_str_list`
- `IFibDecoder`
- `signal_name_of_ensemble`
- `signal_change_in_configuration`
- `signal_start_announcement`
- `signal_stop_announcement`
- `signal_fib_time_info`
- `signal_fib_loaded_state`
- `create`

### `decoder/fib-decoder-string-getter.cpp`

- `hex_str`
- `FibDecoder::_get_audio_data_str`
- `FibDecoder::_get_packet_data_str`

### `decoder/fib-helper.cpp`

- `FibHelper::get_statistics`
- `FibHelper::print_duration_and_get_statistics`
- `FibHelper::print_statistic_header`
- `FibHelper::print_statistic`

### `decoder/fib-helper.h`

- `set_current_time`
- `SStatistic`
- `TT::max`
- `TT::min`
- `get_statistics`
- `print_duration_and_get_statistics`
- `print_statistic_header`
- `print_statistic`

### `decoder/fic-decoder.cpp`

- `FicDecoder::FicDecoder`
- `FicDecoder::process_block`
- `FicDecoder::_process_fic_input`
- `FicDecoder::stop`
- `FicDecoder::restart`
- `FicDecoder::start_fic_dump`
- `FicDecoder::stop_fic_dump`
- `FicDecoder::_dump_fib_to_file`
- `FicDecoder::get_fib_bits`
- `FicDecoder::get_fic_decode_ratio_percent`

### `decoder/fic-decoder.h`

- `FicDecoder`
- `~FicDecoder`
- `process_block`
- `stop`
- `restart`
- `get_fib_bits`
- `get_fic_decode_ratio_percent`
- `reset_fic_decode_success_ratio`
- `start_fic_dump`
- `stop_fic_dump`
- `get_fib_decoder`
- `_process_fic_input`
- `_dump_fib_to_file`
- `signal_fic_status`

## Priority 3 — Error protection, MSC and DAB+ audio decode

These functions are the core references for deinterleaving, protection profiles, Reed-Solomon, firecode and AAC frame handling.

- DABstar files kept here: 38
- DABstar functions or methods in this group: 240
- Closest dabctl Rust modules:
  - src/pipeline/protection.rs
  - src/pipeline/prot_tables.rs
  - src/pipeline/viterbi_handler.rs
  - src/audio/rs_decoder.rs
  - src/audio/superframe.rs
  - src/audio/aac_decoder/mod.rs
  - src/audio/aac_decoder/faad2.rs
  - src/audio/aac_decoder/fdkaac.rs

### `protection/eep-protection.cpp`

- `EepProtection::EepProtection`
- `EepProtection::_extract_viterbi_block_addresses`

### `protection/eep-protection.h`

- `EepProtection`
- `~EepProtection`
- `_extract_viterbi_block_addresses`

### `protection/protection.cpp`

- `Protection::Protection`
- `Protection::deconvolve`

### `protection/protection.h`

- `Protection`
- `~Protection`
- `deconvolve`

### `protection/protTables.cpp`

- `get_PI_codes`

### `protection/protTables.h`

- `get_PI_codes`

### `protection/uep-protection.cpp`

- `find_index`
- `UepProtection::UepProtection`
- `UepProtection::_extract_viterbi_block_addresses`

### `protection/uep-protection.h`

- `UepProtection`
- `~UepProtection`
- `_extract_viterbi_block_addresses`

### `backend/backend.cpp`

- `Backend::Backend`
- `Backend::~Backend`
- `Backend::process`
- `Backend::_process_segment`
- `Backend::run`
- `Backend::stop_running`

### `backend/backend.h`

- `Backend`
- `~Backend`
- `process`
- `stop_running`
- `run`
- `_process_segment`

### `backend/backend-driver.cpp`

- `BackendDriver::BackendDriver`
- `BackendDriver::add_to_frame`

### `backend/backend-driver.h`

- `BackendDriver`
- `~BackendDriver`
- `add_to_frame`

### `backend/backend-deconvolver.cpp`

- `BackendDeconvolver::BackendDeconvolver`
- `BackendDeconvolver::deconvolve`

### `backend/backend-deconvolver.h`

- `BackendDeconvolver`
- `~BackendDeconvolver`
- `deconvolve`

### `backend/msc-handler.cpp`

- `MscHandler::MscHandler`
- `MscHandler::~MscHandler`
- `MscHandler::reset_channel`
- `MscHandler::stop_service`
- `MscHandler::stop_all_services`
- `MscHandler::is_service_running`
- `MscHandler::set_channel`
- `MscHandler::process_block`

### `backend/msc-handler.h`

- `MscHandler`
- `~MscHandler`
- `process_block`
- `set_channel`
- `reset_channel`
- `stop_service`
- `stop_all_services`
- `is_service_running`
- `processMsc`

### `backend/firecode-checker.cpp`

- `FirecodeChecker::FirecodeChecker`
- `FirecodeChecker::fill_crc_table`
- `FirecodeChecker::fill_syndrome_table`
- `FirecodeChecker::crc16`
- `FirecodeChecker::check`
- `FirecodeChecker::check_and_correct_6bits`

### `backend/firecode-checker.h`

- `FirecodeChecker`
- `~FirecodeChecker`
- `check`
- `check_and_correct_6bits`
- `fill_syndrome_table`
- `crc16`
- `fill_crc_table`

### `backend/reed-solomon.cpp`

- `ReedSolomon::encode_rs`
- `ReedSolomon::enc`
- `ReedSolomon::dec`
- `ReedSolomon::decode_rs`
- `ReedSolomon::getSyndrome`
- `ReedSolomon::computeSyndromes`
- `ReedSolomon::computeLambda`
- `ReedSolomon::computeErrors`
- `ReedSolomon::computeOmega`

### `backend/reed-solomon.h`

- `ReedSolomon`
- `~ReedSolomon`
- `dec`
- `enc`
- `computeSyndromes`
- `getSyndrome`
- `computeLambda`
- `computeErrors`
- `computeOmega`
- `encode_rs`
- `decode_rs`

### `backend/rscodec.cpp`

- `rscodec::rscodec`
- `rscodec::~rscodec`
- `rscodec::create_polynomials`
- `rscodec::dec`
- `rscodec::dec_poly`
- `rscodec::add_poly`
- `rscodec::add_power`
- `rscodec::multiply_poly`
- `rscodec::multiply_power`
- `rscodec::divide_poly`
- `rscodec::divide_power`
- `rscodec::pow_poly`
- `rscodec::pow_power`
- `rscodec::poly2tuple`
- `rscodec::power2tuple`
- `rscodec::round_mod`
- `rscodec::power2poly`
- `rscodec::poly2power`
- `rscodec::inverse_poly`
- `rscodec::inverse_power`
- `rscodec::enc`
- `rscodec::enc_poly`

### `backend/rscodec.h`

- `rscodec::rscodec`
- `rscodec::~rscodec`
- `rscodec::dec`
- `rscodec::enc`
- `rscodec::add_poly`
- `rscodec::add_power`
- `rscodec::multiply_poly`
- `rscodec::multiply_power`
- `rscodec::divide_poly`
- `rscodec::divide_power`
- `rscodec::pow_poly`
- `rscodec::pow_power`
- `rscodec::power2poly`
- `rscodec::poly2power`
- `rscodec::inverse_poly`
- `rscodec::inverse_power`
- `rscodec::poly2tuple`
- `rscodec::power2tuple`
- `rscodec::round_mod`
- `rscodec::enc_poly`
- `rscodec::dec_poly`
- `rscodec::create_polynomials`

### `backend/galois.cpp`

- `Galois::Galois`
- `Galois::modnn`
- `Galois::add_poly`
- `Galois::poly2power`
- `Galois::power2poly`
- `Galois::add_power`
- `Galois::multiply_power`
- `Galois::multiply_poly`
- `Galois::divide_power`
- `Galois::divide_poly`
- `Galois::inverse_poly`
- `Galois::inverse_power`
- `Galois::pow_poly`
- `Galois::pow_power`

### `backend/galois.h`

- `Galois`
- `~Galois`
- `modnn`
- `add_poly`
- `add_power`
- `multiply_poly`
- `multiply_power`
- `divide_poly`
- `divide_power`
- `pow_poly`
- `pow_power`
- `power2poly`
- `poly2power`
- `inverse_poly`
- `inverse_power`

### `backend/crc.cpp`

- `calc_crc`
- `check_crc_bytes`
- `check_CRC_bits`

### `backend/crc.h`

- `calc_crc`
- `check_crc_bytes`
- `check_CRC_bits`

### `backend/charsets.cpp`

- `to_QString_using_charset`

### `backend/charsets.h`

- `is_charset_valid`
- `to_QString_using_charset`

### `backend/audio/bit-writer.cpp`

- `BitWriter::Reset`
- `BitWriter::AddBits`
- `BitWriter::AddBytes`
- `BitWriter::WriteAudioMuxLengthBytes`

### `backend/audio/bit-writer.h`

- `BitWriter`
- `Reset`
- `AddBits`
- `AddBytes`
- `WriteAudioMuxLengthBytes`

### `backend/audio/faad-decoder.cpp`

- `faadDecoder::faadDecoder`
- `faadDecoder::~faadDecoder`
- `get_aac_channel_configuration`
- `faadDecoder::initialize`
- `faadDecoder::convert_mp4_to_pcm`

### `backend/audio/faad-decoder.h`

- `faadDecoder`
- `~faadDecoder`
- `convert_mp4_to_pcm`
- `initialize`
- `signal_new_audio`

### `backend/audio/fdk-aac.cpp`

- `FdkAAC::FdkAAC`
- `FdkAAC::~FdkAAC`
- `FdkAAC::convert_mp4_to_pcm`

### `backend/audio/fdk-aac.h`

- `FdkAAC`
- `~FdkAAC`
- `convert_mp4_to_pcm`
- `signal_new_audio`

### `backend/audio/mp4processor.cpp`

- `Mp4Processor::Mp4Processor`
- `Mp4Processor::~Mp4Processor`
- `Mp4Processor::add_to_frame`
- `Mp4Processor::_process_reed_solomon_frame`
- `Mp4Processor::_process_super_frame`
- `Mp4Processor::_build_aac_stream`

### `backend/audio/mp4processor.h`

- `Mp4Processor`
- `~Mp4Processor`
- `add_to_frame`
- `_process_reed_solomon_frame`
- `_process_super_frame`
- `_build_aac_stream`
- `signal_show_frame_errors`
- `signal_show_rs_errors`
- `signal_show_aac_errors`
- `signal_is_stereo`
- `signal_new_aac_frame`
- `signal_show_rs_corrections`

### `support/dab-tables.cpp`

- `getASCTy`
- `getDSCTy`
- `getLanguage`
- `getCountry`
- `getProgramType`
- `getProgramType_For_NorthAmerica`
- `getUserApplicationType`
- `getFECscheme`
- `getProtectionLevel`
- `getCodeRate`
- `get_announcement_type_str`
- `get_DSCTy_AppType`

### `support/dab-tables.h`

- `getASCTy`
- `getDSCTy`
- `getLanguage`
- `getCountry`
- `getProgramType`
- `getProgramType_For_NorthAmerica`
- `getUserApplicationType`
- `getFECscheme`
- `getProtectionLevel`
- `getCodeRate`
- `get_announcement_type_str`
- `get_DSCTy_AppType`

## Priority 4 — PAD, DLS and MOT slideshow metadata

These functions are the right DABstar references for the current dabctl metadata path on fd 3.

- DABstar files kept here: 8
- DABstar functions or methods in this group: 66
- Closest dabctl Rust modules:
  - src/audio/pad_decoder.rs
  - src/audio/pad_output.rs
  - src/audio/mot_decoder.rs
  - src/audio/mot_manager.rs
  - src/audio/ebu_latin.rs

### `backend/data/pad-handler.cpp`

- `PadHandler::PadHandler`
- `PadHandler::process_PAD`
- `PadHandler::_handle_short_PAD`
- `PadHandler::_handle_variable_PAD`
- `PadHandler::_dynamic_label`
- `PadHandler::_new_MSC_element`
- `PadHandler::_add_MSC_element`
- `PadHandler::_build_MSC_segment`
- `PadHandler::_reset_charset_change`
- `PadHandler::_check_charset_change`

### `backend/data/pad-handler.h`

- `PadHandler`
- `~PadHandler`
- `process_PAD`
- `_handle_variable_PAD`
- `_handle_short_PAD`
- `_dynamic_label`
- `_new_MSC_element`
- `_add_MSC_element`
- `_build_MSC_segment`
- `_pad_crc`
- `_reset_charset_change`
- `_check_charset_change`
- `signal_show_label`
- `signal_show_mot_handling`

### `backend/data/mot/mot-dir.cpp`

- `MotDirectory::MotDirectory`
- `MotDirectory::~MotDirectory`
- `MotDirectory::getHandle`
- `MotDirectory::setHandle`
- `MotDirectory::directorySegment`
- `MotDirectory::analyse_theDirectory`
- `MotDirectory::get_transportId`

### `backend/data/mot/mot-dir.h`

- `MotDirectory::MotDirectory`
- `MotDirectory::~MotDirectory`
- `MotDirectory::getHandle`
- `MotDirectory::setHandle`
- `MotDirectory::directorySegment`
- `MotDirectory::get_transportId`
- `MotDirectory::analyse_theDirectory`

### `backend/data/mot/mot-handler.cpp`

- `MotHandler::MotHandler`
- `MotHandler::~MotHandler`
- `MotHandler::add_MSC_data_group`
- `MotHandler::getHandle`
- `MotHandler::setHandle`

### `backend/data/mot/mot-handler.h`

- `MotHandler`
- `~MotHandler`
- `add_MSC_data_group`
- `setHandle`
- `getHandle`

### `backend/data/mot/mot-object.cpp`

- `MotObject::MotObject`
- `MotObject::set_header`
- `MotObject::add_body_segment`
- `MotObject::_process_parameter_id`
- `MotObject::_check_if_complete`
- `MotObject::_handle_complete`
- `MotObject::get_header_size`
- `MotObject::reset`

### `backend/data/mot/mot-object.h`

- `MotObject`
- `~MotObject`
- `set_header`
- `add_body_segment`
- `get_header_size`
- `reset`
- `_check_if_complete`
- `_handle_complete`
- `_process_parameter_id`
- `signal_new_MOT_object`

## Useful later, but not core to current dabctl scope

Keep these as secondary references only if dabctl expands beyond the current direct DAB+ audio plus metadata target.

- DABstar files kept here: 14
- DABstar functions or methods in this group: 127
- Closest dabctl Rust modules:
  - src/pipeline/ringbuffer.rs
  - src/audio/pad_decoder.rs

### `backend/audio/mp2processor.cpp`

- `Mp2Processor::Mp2Processor`
- `Mp2Processor::~Mp2Processor`
- `Mp2Processor::_set_sample_rate`
- `Mp2Processor::_get_mp2_sample_rate`
- `Mp2Processor::_read_allocation`
- `Mp2Processor::_read_samples`
- `Mp2Processor::_mp2_decode_frame`
- `Mp2Processor::_process_pad_data`
- `Mp2Processor::add_to_frame`
- `Mp2Processor::_add_bit_to_mp2`

### `backend/audio/mp2processor.h`

- `Mp2Processor`
- `~Mp2Processor`
- `add_to_frame`
- `_get_mp2_sample_rate`
- `_mp2_decode_frame`
- `_set_sample_rate`
- `_read_allocation`
- `_read_samples`
- `_get_bits`
- `_add_bit_to_mp2`
- `_process_pad_data`
- `signal_show_frameErrors`
- `signal_new_audio`
- `signal_new_mp2_frame`
- `signal_is_stereo`

### `backend/data/data-processor.cpp`

- `DataProcessor::DataProcessor`
- `DataProcessor::add_to_frame`
- `DataProcessor::_handle_packets`
- `DataProcessor::_handle_packet`
- `DataProcessor::_handle_TDC_async_stream`

### `backend/data/data-processor.h`

- `DataProcessor`
- `~DataProcessor`
- `add_to_frame`
- `_handle_TDC_async_stream`
- `_handle_packets`
- `_handle_packet`
- `signal_show_MSC_errors`

### `backend/data/ip-datahandler.cpp`

- `IpDataHandler::IpDataHandler`
- `IpDataHandler::add_MSC_data_group`
- `IpDataHandler::process_ipVector`
- `IpDataHandler::process_udpVector`

### `backend/data/ip-datahandler.h`

- `IpDataHandler`
- `~IpDataHandler`
- `add_MSC_data_group`
- `process_ipVector`
- `process_udpVector`
- `signal_write_datagramm`

### `backend/data/tdc-datahandler.cpp`

- `tdc_dataHandler::tdc_dataHandler`
- `tdc_dataHandler::add_MSC_data_group`
- `tdc_dataHandler::handleFrame_type_0`
- `tdc_dataHandler::handleFrame_type_1`
- `tdc_dataHandler::serviceComponentFrameheaderCRC`

### `backend/data/tdc-datahandler.h`

- `tdc_dataHandler`
- `~tdc_dataHandler`
- `add_MSC_data_group`
- `handleFrame_type_0`
- `handleFrame_type_1`
- `serviceComponentFrameheaderCRC`
- `signal_bytes_out`

### `backend/data/journaline-datahandler.cpp`

- `callback_func`
- `JournalineDataHandler::JournalineDataHandler`
- `JournalineDataHandler::~JournalineDataHandler`
- `JournalineDataHandler::add_MSC_data_group`
- `JournalineDataHandler::_destroy_database`
- `JournalineDataHandler::add_to_dataBase`

### `backend/data/journaline-datahandler.h`

- `JournalineDataHandler`
- `~JournalineDataHandler`
- `add_MSC_data_group`
- `add_to_dataBase`
- `_destroy_database`
- `signal_new_data`

### `backend/data/journaline-viewer.cpp`

- `JournalineViewer::JournalineViewer`
- `JournalineViewer::~JournalineViewer`
- `JournalineViewer::slot_new_data`
- `JournalineViewer::_slot_colorize_receive_marker_timeout`
- `JournalineViewer::_slot_html_rebuild_timeout`
- `JournalineViewer::_slot_html_link_activated`
- `JournalineViewer::_build_html_tree_recursive`
- `JournalineViewer::_get_journaline_as_HTML`
- `JournalineViewer::_set_receiver_marker_color`

### `backend/data/journaline-viewer.h`

- `JournalineViewer`
- `~JournalineViewer`
- `_get_journaline_as_HTML`
- `_build_html_tree_recursive`
- `_set_receiver_marker_color`
- `slot_new_data`
- `_slot_colorize_receive_marker_timeout`
- `_slot_html_rebuild_timeout`
- `_slot_html_link_activated`
- `signal_window_closed`

### `support/ringbuffer.cpp`

- `RingBufferFactory`
- `RingBufferFactoryBase::_show_progress_bar`
- `RingBufferFactoryBase::_print_line`
- `RingBufferFactoryBase::_calculate_ring_buffer_statistics`
- `print_status`

### `support/ringbuffer.h`

- `Apple_MemoryBarrier`
- `std::atomic_thread_fence`
- `OSMemoryBarrier`
- `RingBuffer`
- `~RingBuffer`
- `get_ring_buffer_read_available`
- `get_ring_buffer_write_available`
- `get_fill_state_in_percent`
- `get_fill_state`
- `flush_ring_buffer`
- `advance_ring_buffer_write_index`
- `PaUtil_WriteMemoryBarrier`
- `advance_ring_buffer_read_index`
- `PaUtil_FullMemoryBarrier`
- `put_data_into_ring_buffer`
- `get_data_from_ring_buffer`
- `skip_data_in_ring_buffer`
- `get_writable_ring_buffer_segment`
- `_get_ring_buffer_write_regions`
- `_get_ring_buffer_read_regions`
- `PaUtil_ReadMemoryBarrier`
- `_round_up_to_next_power_of_2`
- `RingBufferFactoryBase`
- `~RingBufferFactoryBase`
- `_show_progress_bar`
- `_print_line`
- `_calculate_ring_buffer_statistics`
- `RingBufferFactory`
- `~RingBufferFactory`
- `create_ringbuffer`
- `get_ringbuffer`
- `print_status`

## Explicitly not selected

The following DABstar areas were intentionally left out of this filtered file because they do not match the current dabctl runtime target closely enough:

- All sections under _old_files
- Most of src/devices, because dabctl already has its own RTL-SDR backend abstraction
- Qt GUI and spectrum viewer modules
- Audio output device UI glue
- Update, configuration and service-list helpers
- ETI or non-direct-output related code
