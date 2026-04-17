# DABstar function inventory

Auto-generated inventory of the DABstar source tree scanned from a local clone of https://github.com/tomneda/DABstar.

## Scan summary

- Generated: 2026-04-16 20:12 UTC
- Source root scanned: `/tmp/DABstar/src`
- Source/header files scanned: 432
- Files with detected functions: 411
- Detected functions and methods: 5889
- Files with no detected function signatures: 21

## Directory summary

| Directory | Files scanned | Functions detected |
|---|---:|---:|
| `_old_files` | 98 | 1290 |
| `audio` | 6 | 70 |
| `backend` | 72 | 637 |
| `configuration` | 2 | 10 |
| `decoder` | 15 | 267 |
| `devices` | 92 | 1445 |
| `eti-handler` | 2 | 18 |
| `file-devices` | 2 | 11 |
| `main` | 13 | 488 |
| `ofdm` | 16 | 166 |
| `protection` | 8 | 18 |
| `scopes` | 10 | 86 |
| `server-thread` | 2 | 8 |
| `service-list` | 4 | 80 |
| `specials` | 3 | 13 |
| `spectrum-viewer` | 10 | 91 |
| `support` | 72 | 1171 |
| `update` | 5 | 20 |

## Files without detected function signatures

- `_old_files/devices/pluto-handler/dabFilter.h`
- `_old_files/devices/pluto-rxtx/dabFilter.h`
- `_old_files/devices/pluto-rxtx/fft/_kiss_fft_guts.h`
- `_old_files/devices/soapy/SoapySDR/Config.h`
- `_old_files/devices/soapy/SoapySDR/Config.hpp`
- `_old_files/devices/soapy/SoapySDR/Constants.h`
- `_old_files/fft/_kiss_fft_guts.h`
- `_old_files/main/country-codes.h`
- `_old_files/output/audiofifo.cpp`
- `audio/audiofifo.h`
- `backend/data/journaline/cpplog.h`
- `backend/mm_malloc.h`
- `decoder/fib-table.h`
- `devices/colibri-handler/common.h`
- `devices/pluto-handler-2/dabFilter.h`
- `devices/spy-server/spyserver-protocol.h`
- `main/glob_data_types.h`
- `main/glob_enums.h`
- `support/converted_map.h`
- `support/process-params.h`
- `support/setting-helper.cnf.h`

## Full function list by file

### `_old_files/backend/converter-2.h`

- `converter_2::sincPI`
- `converter_2::HannCoeff`
- `converter_2::getInterpolate`
- `converter_2::converter_2`
- `converter_2::~converter_2`
- `converter_2::add`
- `converter_2::getOutputSize`

### `_old_files/devices/filereaders/xml-filereader/element-reader.h`

- `elementReader::elementReader`
- `elementReader::~elementReader`
- `int8_reader::int8_reader`
- `int8_reader::~int8_reader`
- `int8_reader::readElement`
- `int8_reader::fread`
- `uint8_reader::uint8_reader`
- `uint8_reader::~uint8_reader`
- `uint8_reader::readElement`
- `uint8_reader::fread`
- `int16_reader::int16_reader`
- `int16_reader::~int16_reader`
- `int16_reader::readElement`
- `int16_reader::fread`
- `int24_reader::int24_reader`
- `int24_reader::~int24_reader`
- `int24_reader::fread`
- `int32_reader::int32_reader`
- `int32_reader::~int32_reader`
- `int32_reader::fread`
- `float_reader::float_reader`
- `float_reader::~float_reader`
- `float_reader::fread`

### `_old_files/devices/pluto-handler/dabFilter.h`

- No function declarations detected

### `_old_files/devices/pluto-handler/iio.h`

- `iio_create_scan_context`
- `iio_scan_context_destroy`
- `iio_scan_context_get_info_list`
- `iio_context_info_list_free`
- `iio_context_info_get_description`
- `iio_context_info_get_uri`
- `iio_library_get_version`
- `iio_strerror`
- `iio_has_backend`
- `iio_get_backends_count`
- `iio_get_backend`
- `iio_create_default_context`
- `iio_create_local_context`
- `iio_create_xml_context`
- `iio_create_xml_context_mem`
- `iio_create_network_context`
- `iio_create_context_from_uri`
- `iio_context_clone`
- `iio_context_destroy`
- `iio_context_get_version`
- `iio_context_get_xml`
- `iio_context_get_name`
- `iio_context_get_description`
- `iio_context_get_attrs_count`
- `iio_context_get_attr`
- `iio_context_get_attr_value`
- `iio_context_get_devices_count`
- `iio_context_get_device`
- `iio_context_find_device`
- `iio_context_set_timeout`
- `iio_device_get_context`
- `iio_device_get_id`
- `iio_device_get_name`
- `iio_device_get_channels_count`
- `iio_device_get_attrs_count`
- `iio_device_get_buffer_attrs_count`
- `iio_device_get_channel`
- `iio_device_get_attr`
- `iio_device_get_buffer_attr`
- `iio_device_find_channel`
- `iio_device_find_attr`
- `iio_device_find_buffer_attr`
- `iio_device_attr_read`
- `iio_device_attr_read_all`
- `iio_device_attr_read_bool`
- `iio_device_attr_read_longlong`
- `iio_device_attr_read_double`
- `iio_device_attr_write`
- `iio_device_attr_write_raw`
- `iio_device_attr_write_all`
- `iio_device_attr_write_bool`
- `iio_device_attr_write_longlong`
- `iio_device_attr_write_double`
- `iio_device_buffer_attr_read`
- `iio_device_buffer_attr_read_all`
- `iio_device_buffer_attr_read_bool`
- `iio_device_buffer_attr_read_longlong`
- `iio_device_buffer_attr_read_double`
- `iio_device_buffer_attr_write`
- `iio_device_buffer_attr_write_raw`
- `iio_device_buffer_attr_write_all`
- `iio_device_buffer_attr_write_bool`
- `iio_device_buffer_attr_write_longlong`
- `iio_device_buffer_attr_write_double`
- `iio_device_set_data`
- `iio_device_get_data`
- `iio_device_get_trigger`
- `iio_device_set_trigger`
- `iio_device_is_trigger`
- `iio_device_set_kernel_buffers_count`
- `iio_channel_get_device`
- `iio_channel_get_id`
- `iio_channel_get_name`
- `iio_channel_is_output`
- `iio_channel_is_scan_element`
- `iio_channel_get_attrs_count`
- `iio_channel_get_attr`
- `iio_channel_find_attr`
- `iio_channel_attr_get_filename`
- `iio_channel_attr_read`
- `iio_channel_attr_read_all`
- `iio_channel_attr_read_bool`
- `iio_channel_attr_read_longlong`
- `iio_channel_attr_read_double`
- `iio_channel_attr_write`
- `iio_channel_attr_write_raw`
- `iio_channel_attr_write_all`
- `iio_channel_attr_write_bool`
- `iio_channel_attr_write_longlong`
- `iio_channel_attr_write_double`
- `iio_channel_enable`
- `iio_channel_disable`
- `iio_channel_is_enabled`
- `iio_channel_read_raw`
- `iio_channel_read`
- `iio_channel_write_raw`
- `iio_channel_write`
- `iio_channel_set_data`
- `iio_channel_get_data`
- `iio_channel_get_type`
- `iio_channel_get_modifier`
- `iio_buffer_get_device`
- `iio_device_create_buffer`
- `iio_buffer_destroy`
- `iio_buffer_get_poll_fd`
- `iio_buffer_set_blocking_mode`
- `iio_buffer_refill`
- `iio_buffer_push`
- `iio_buffer_push_partial`
- `iio_buffer_cancel`
- `iio_buffer_start`
- `iio_buffer_first`
- `iio_buffer_step`
- `iio_buffer_end`
- `iio_buffer_foreach_sample`
- `iio_buffer_set_data`
- `iio_buffer_get_data`
- `iio_device_get_sample_size`
- `iio_channel_get_index`
- `iio_channel_get_data_format`
- `iio_channel_convert`
- `iio_channel_convert_inverse`
- `iio_device_get_debug_attrs_count`
- `iio_device_get_debug_attr`
- `iio_device_find_debug_attr`
- `iio_device_debug_attr_read`
- `iio_device_debug_attr_read_all`
- `iio_device_debug_attr_write`
- `iio_device_debug_attr_write_raw`
- `iio_device_debug_attr_write_all`
- `iio_device_debug_attr_read_bool`
- `iio_device_debug_attr_read_longlong`
- `iio_device_debug_attr_read_double`
- `iio_device_debug_attr_write_bool`
- `iio_device_debug_attr_write_longlong`
- `iio_device_debug_attr_write_double`
- `iio_device_identify_filename`
- `iio_device_reg_write`
- `iio_device_reg_read`

### `_old_files/devices/pluto-handler/pluto-handler.cpp`

- `get_ch_name`
- `ad9361_set_trx_fir_enable`
- `ad9361_get_trx_fir_enable`
- `get_ad9361_stream_dev`
- `get_ad9361_stream_ch`
- `get_phy_chan`
- `get_lo_chan`
- `cfg_ad9361_streaming_ch`
- `plutoHandler::plutoHandler`
- `plutoHandler::~plutoHandler`
- `plutoHandler::setVFOFrequency`
- `plutoHandler::getVFOFrequency`
- `plutoHandler::set_gainControl`
- `plutoHandler::set_agcControl`
- `plutoHandler::restartReader`
- `plutoHandler::stopReader`
- `plutoHandler::run`
- `plutoHandler::getSamples`
- `plutoHandler::Samples`
- `plutoHandler::set_filter`
- `plutoHandler::resetBuffer`
- `plutoHandler::bitDepth`
- `plutoHandler::deviceName`
- `plutoHandler::show`
- `plutoHandler::hide`
- `plutoHandler::isHidden`
- `plutoHandler::toggle_debugButton`
- `plutoHandler::set_xmlDump`
- `isValid`
- `plutoHandler::setup_xmlDump`
- `plutoHandler::close_xmlDump`
- `plutoHandler::record_gainSettings`
- `plutoHandler::update_gainSettings`

### `_old_files/devices/pluto-handler/pluto-handler.h`

- `plutoHandler::plutoHandler`
- `plutoHandler::~plutoHandler`
- `plutoHandler::setVFOFrequency`
- `plutoHandler::getVFOFrequency`
- `plutoHandler::restartReader`
- `plutoHandler::stopReader`
- `plutoHandler::getSamples`
- `plutoHandler::Samples`
- `plutoHandler::resetBuffer`
- `plutoHandler::bitDepth`
- `plutoHandler::show`
- `plutoHandler::hide`
- `plutoHandler::isHidden`
- `plutoHandler::deviceName`
- `plutoHandler::setup_xmlDump`
- `plutoHandler::close_xmlDump`
- `plutoHandler::run`
- `plutoHandler::record_gainSettings`
- `plutoHandler::update_gainSettings`
- `plutoHandler::new_gainValue`
- `plutoHandler::new_agcValue`
- `plutoHandler::set_gainControl`
- `plutoHandler::set_agcControl`
- `plutoHandler::toggle_debugButton`
- `plutoHandler::set_filter`
- `plutoHandler::set_xmlDump`

### `_old_files/devices/pluto-rxtx/ad9361.h`

- `ad9361_multichip_sync`
- `ad9361_fmcomms5_multichip_sync`
- `ad9361_set_bb_rate`
- `ad9361_set_trx_fir_enable`
- `ad9361_get_trx_fir_enable`
- `ad9361_generate_fir_taps`
- `ad9361_calculate_rf_clock_chain`
- `ad9361_calculate_rf_clock_chain_fdp`
- `ad9361_set_bb_rate_custom_filter_auto`
- `ad9361_set_bb_rate_custom_filter_manual`
- `ad9361_fmcomms5_phase_sync`

### `_old_files/devices/pluto-rxtx/dab-streamer/dab-streamer.cpp`

- `getMyTime`
- `dabStreamer::dabStreamer`
- `dabStreamer::~dabStreamer`
- `dabStreamer::stop`
- `dabStreamer::audioOutput`
- `dabStreamer::addRds`
- `dabStreamer::addName`
- `dabStreamer::run`
- `preemp`
- `lowPass`
- `dabStreamer::modulateData`
- `dabStreamer::rds_crc`
- `dabStreamer::rds_bits_to_values`
- `dabStreamer::rds_serialize`
- `dabStreamer::rds_init_groups`
- `dabStreamer::rds_group_0A_update`
- `dabStreamer::rds_group_2A_update`
- `dabStreamer::rds_group_3A_update`
- `dabStreamer::rds_group_8A_update`

### `_old_files/devices/pluto-rxtx/dab-streamer/dab-streamer.h`

- `dabStreamer`
- `~dabStreamer`
- `audioOutput`
- `addRds`
- `addName`
- `stop`
- `run`
- `modulateData`
- `rds_crc`
- `rds_bits_to_values`
- `rds_serialize`
- `rds_init_groups`
- `rds_group_0A_update`
- `rds_group_2A_update`
- `rds_group_3A_update`
- `rds_group_8A_update`

### `_old_files/devices/pluto-rxtx/dab-streamer/up-filter.cpp`

- `upFilter::upFilter`
- `upFilter::~upFilter`
- `upFilter::Filter`

### `_old_files/devices/pluto-rxtx/dab-streamer/up-filter.h`

- `upFilter::upFilter`
- `upFilter::~upFilter`
- `upFilter::Filter`

### `_old_files/devices/pluto-rxtx/dabFilter.h`

- No function declarations detected

### `_old_files/devices/pluto-rxtx/fft/_kiss_fft_guts.h`

- No function declarations detected

### `_old_files/devices/pluto-rxtx/fft/kiss_fft.c`

- `kf_bfly2`
- `kf_bfly4`
- `kf_bfly3`
- `kf_bfly5`
- `kf_bfly_generic`
- `kf_work`
- `kf_factor`
- `kiss_fft_alloc`
- `kiss_fft_stride`
- `kiss_fft`
- `kiss_fft_cleanup`
- `kiss_fft_next_fast_size`

### `_old_files/devices/pluto-rxtx/fft/kiss_fft.h`

- `kiss_fft_alloc`
- `kiss_fft`
- `kiss_fft_stride`
- `kiss_fft_cleanup`
- `kiss_fft_next_fast_size`

### `_old_files/devices/pluto-rxtx/fft/kiss_fftr.c`

- `kiss_fftr_alloc`
- `kiss_fftr`
- `kiss_fftri`

### `_old_files/devices/pluto-rxtx/fft/kiss_fftr.h`

- `kiss_fftr_alloc`
- `kiss_fftr`
- `kiss_fftri`

### `_old_files/devices/pluto-rxtx/iio.h`

- `iio_create_scan_context`
- `iio_scan_context_destroy`
- `iio_scan_context_get_info_list`
- `iio_context_info_list_free`
- `iio_context_info_get_description`
- `iio_context_info_get_uri`
- `iio_library_get_version`
- `iio_strerror`
- `iio_has_backend`
- `iio_get_backends_count`
- `iio_get_backend`
- `iio_create_default_context`
- `iio_create_local_context`
- `iio_create_xml_context`
- `iio_create_xml_context_mem`
- `iio_create_network_context`
- `iio_create_context_from_uri`
- `iio_context_clone`
- `iio_context_destroy`
- `iio_context_get_version`
- `iio_context_get_xml`
- `iio_context_get_name`
- `iio_context_get_description`
- `iio_context_get_attrs_count`
- `iio_context_get_attr`
- `iio_context_get_attr_value`
- `iio_context_get_devices_count`
- `iio_context_get_device`
- `iio_context_find_device`
- `iio_context_set_timeout`
- `iio_device_get_context`
- `iio_device_get_id`
- `iio_device_get_name`
- `iio_device_get_channels_count`
- `iio_device_get_attrs_count`
- `iio_device_get_buffer_attrs_count`
- `iio_device_get_channel`
- `iio_device_get_attr`
- `iio_device_get_buffer_attr`
- `iio_device_find_channel`
- `iio_device_find_attr`
- `iio_device_find_buffer_attr`
- `iio_device_attr_read`
- `iio_device_attr_read_all`
- `iio_device_attr_read_bool`
- `iio_device_attr_read_longlong`
- `iio_device_attr_read_double`
- `iio_device_attr_write`
- `iio_device_attr_write_raw`
- `iio_device_attr_write_all`
- `iio_device_attr_write_bool`
- `iio_device_attr_write_longlong`
- `iio_device_attr_write_double`
- `iio_device_buffer_attr_read`
- `iio_device_buffer_attr_read_all`
- `iio_device_buffer_attr_read_bool`
- `iio_device_buffer_attr_read_longlong`
- `iio_device_buffer_attr_read_double`
- `iio_device_buffer_attr_write`
- `iio_device_buffer_attr_write_raw`
- `iio_device_buffer_attr_write_all`
- `iio_device_buffer_attr_write_bool`
- `iio_device_buffer_attr_write_longlong`
- `iio_device_buffer_attr_write_double`
- `iio_device_set_data`
- `iio_device_get_data`
- `iio_device_get_trigger`
- `iio_device_set_trigger`
- `iio_device_is_trigger`
- `iio_device_set_kernel_buffers_count`
- `iio_channel_get_device`
- `iio_channel_get_id`
- `iio_channel_get_name`
- `iio_channel_is_output`
- `iio_channel_is_scan_element`
- `iio_channel_get_attrs_count`
- `iio_channel_get_attr`
- `iio_channel_find_attr`
- `iio_channel_attr_get_filename`
- `iio_channel_attr_read`
- `iio_channel_attr_read_all`
- `iio_channel_attr_read_bool`
- `iio_channel_attr_read_longlong`
- `iio_channel_attr_read_double`
- `iio_channel_attr_write`
- `iio_channel_attr_write_raw`
- `iio_channel_attr_write_all`
- `iio_channel_attr_write_bool`
- `iio_channel_attr_write_longlong`
- `iio_channel_attr_write_double`
- `iio_channel_enable`
- `iio_channel_disable`
- `iio_channel_is_enabled`
- `iio_channel_read_raw`
- `iio_channel_read`
- `iio_channel_write_raw`
- `iio_channel_write`
- `iio_channel_set_data`
- `iio_channel_get_data`
- `iio_channel_get_type`
- `iio_channel_get_modifier`
- `iio_buffer_get_device`
- `iio_device_create_buffer`
- `iio_buffer_destroy`
- `iio_buffer_get_poll_fd`
- `iio_buffer_set_blocking_mode`
- `iio_buffer_refill`
- `iio_buffer_push`
- `iio_buffer_push_partial`
- `iio_buffer_cancel`
- `iio_buffer_start`
- `iio_buffer_first`
- `iio_buffer_step`
- `iio_buffer_end`
- `iio_buffer_foreach_sample`
- `iio_buffer_set_data`
- `iio_buffer_get_data`
- `iio_device_get_sample_size`
- `iio_channel_get_index`
- `iio_channel_get_data_format`
- `iio_channel_convert`
- `iio_channel_convert_inverse`
- `iio_device_get_debug_attrs_count`
- `iio_device_get_debug_attr`
- `iio_device_find_debug_attr`
- `iio_device_debug_attr_read`
- `iio_device_debug_attr_read_all`
- `iio_device_debug_attr_write`
- `iio_device_debug_attr_write_raw`
- `iio_device_debug_attr_write_all`
- `iio_device_debug_attr_read_bool`
- `iio_device_debug_attr_read_longlong`
- `iio_device_debug_attr_read_double`
- `iio_device_debug_attr_write_bool`
- `iio_device_debug_attr_write_longlong`
- `iio_device_debug_attr_write_double`
- `iio_device_identify_filename`
- `iio_device_reg_write`
- `iio_device_reg_read`

### `_old_files/devices/pluto-rxtx/pluto-rxtx-handler.cpp`

- `get_ch_name`
- `plutoHandler::plutoHandler`
- `plutoHandler::~plutoHandler`
- `plutoHandler::setVFOFrequency`
- `plutoHandler::getVFOFrequency`
- `plutoHandler::set_gainControl`

### `_old_files/devices/pluto-rxtx/pluto-rxtx-handler.h`

- `plutoHandler`
- `~plutoHandler`
- `setVFOFrequency`
- `getVFOFrequency`
- `restartReader`
- `stopReader`
- `getSamples`
- `Samples`
- `sendSample`
- `startTransmitter`
- `stopTransmitter`
- `resetBuffer`
- `bitDepth`
- `show`
- `hide`
- `isHidden`
- `deviceName`
- `loadFunctions`
- `setup_xmlDump`
- `close_xmlDump`
- `run_receiver`
- `run_transmitter`
- `showBuffer`
- `ad9361_set_trx_fir_enable`
- `ad9361_get_trx_fir_enable`
- `get_ad9361_stream_dev`
- `get_ad9361_stream_ch`
- `get_phy_chan`
- `get_lo_chan`
- `cfg_ad9361_streaming_ch`
- `record_gainSettings`
- `update_gainSettings`
- `new_gainValue`
- `new_agcValue`
- `showSignal`
- `set_gainControl`
- `set_agcControl`
- `toggle_debugButton`
- `set_filter`
- `set_xmlDump`
- `handleSignal`
- `set_fmFrequency`

### `_old_files/devices/sdrplay-handler-v2/mirsdrapi-rsp.h`

- `mir_sdr_Init`
- `mir_sdr_Uninit`
- `mir_sdr_ReadPacket`
- `mir_sdr_SetRf`
- `mir_sdr_SetFs`
- `mir_sdr_SetGr`
- `mir_sdr_SetGrParams`
- `mir_sdr_SetDcMode`
- `mir_sdr_SetDcTrackTime`
- `mir_sdr_SetSyncUpdateSampleNum`
- `mir_sdr_SetSyncUpdatePeriod`
- `mir_sdr_ApiVersion`
- `mir_sdr_ResetUpdateFlags`
- `mir_sdr_SetTransferMode`
- `mir_sdr_DownConvert`
- `mir_sdr_SetParam`
- `mir_sdr_SetPpm`
- `mir_sdr_SetLoMode`
- `mir_sdr_SetGrAltMode`
- `mir_sdr_DCoffsetIQimbalanceControl`
- `mir_sdr_DecimateControl`
- `mir_sdr_AgcControl`
- `mir_sdr_StreamInit`
- `mir_sdr_StreamUninit`
- `mir_sdr_Reinit`
- `mir_sdr_GetGrByFreq`
- `mir_sdr_DebugEnable`
- `mir_sdr_GetCurrentGain`
- `mir_sdr_GainChangeCallbackMessageReceived`
- `mir_sdr_GetDevices`
- `mir_sdr_SetDeviceIdx`
- `mir_sdr_ReleaseDeviceIdx`
- `mir_sdr_GetHwVersion`
- `mir_sdr_RSPII_AntennaControl`
- `mir_sdr_RSPII_ExternalReferenceControl`
- `mir_sdr_RSPII_BiasTControl`
- `mir_sdr_RSPII_RfNotchEnable`
- `mir_sdr_RSP_SetGr`
- `mir_sdr_RSP_SetGrLimits`
- `mir_sdr_AmPortSelect`
- `mir_sdr_rsp1a_BiasT`
- `mir_sdr_rsp1a_DabNotch`
- `mir_sdr_rsp1a_BroadcastNotch`
- `mir_sdr_rspDuo_TunerSel`
- `mir_sdr_rspDuo_ExtRef`
- `mir_sdr_rspDuo_BiasT`
- `mir_sdr_rspDuo_Tuner1AmNotch`
- `mir_sdr_rspDuo_BroadcastNotch`
- `mir_sdr_rspDuo_DabNotch`

### `_old_files/devices/sdrplay-handler-v2/sdrplay-handler-v2.cpp`

- `get_lnaGRdB`
- `SdrPlayHandler_v2::SdrPlayHandler_v2`
- `SdrPlayHandler_v2::~SdrPlayHandler_v2`
- `SdrPlayHandler_v2::defaultFrequency`
- `SdrPlayHandler_v2::getVFOFrequency`
- `SdrPlayHandler_v2::set_ifgainReduction`
- `SdrPlayHandler_v2::set_lnagainReduction`
- `myStreamCallback`
- `myGainChangeCallback`
- `SdrPlayHandler_v2::adjustFreq`
- `SdrPlayHandler_v2::restartReader`
- `SdrPlayHandler_v2::voidSignal`
- `SdrPlayHandler_v2::stopReader`
- `SdrPlayHandler_v2::getSamples`
- `SdrPlayHandler_v2::Samples`
- `SdrPlayHandler_v2::resetBuffer`
- `SdrPlayHandler_v2::bitDepth`
- `SdrPlayHandler_v2::deviceName`
- `SdrPlayHandler_v2::set_agcControl`
- `SdrPlayHandler_v2::set_debugControl`
- `SdrPlayHandler_v2::set_ppmControl`
- `SdrPlayHandler_v2::set_antennaSelect`
- `SdrPlayHandler_v2::set_tunerSelect`
- `SdrPlayHandler_v2::loadFunctions`
- `SdrPlayHandler_v2::fetchLibrary`
- `SdrPlayHandler_v2::releaseLibrary`
- `SdrPlayHandler_v2::show`
- `SdrPlayHandler_v2::hide`
- `SdrPlayHandler_v2::isHidden`
- `SdrPlayHandler_v2::errorCodes`
- `SdrPlayHandler_v2::set_xmlDump`
- `isValid`
- `SdrPlayHandler_v2::setup_xmlDump`
- `SdrPlayHandler_v2::close_xmlDump`
- `SdrPlayHandler_v2::record_gainSettings`
- `SdrPlayHandler_v2::update_gainSettings`
- `SdrPlayHandler_v2::biasT_selectorHandler`
- `SdrPlayHandler_v2::get_coords`
- `SdrPlayHandler_v2::moveTo`
- `SdrPlayHandler_v2::setVFOFrequency`
- `SdrPlayHandler_v2::isFileInput`

### `_old_files/devices/sdrplay-handler-v2/sdrplay-handler-v2.h`

- `SdrPlayHandler_v2`
- `~SdrPlayHandler_v2`
- `setVFOFrequency`
- `getVFOFrequency`
- `restartReader`
- `stopReader`
- `getSamples`
- `Samples`
- `resetBuffer`
- `bitDepth`
- `deviceName`
- `show`
- `hide`
- `isHidden`
- `isFileInput`
- `adjustFreq`
- `defaultFrequency`
- `get_coords`
- `moveTo`
- `fetchLibrary`
- `releaseLibrary`
- `errorCodes`
- `loadFunctions`
- `setup_xmlDump`
- `close_xmlDump`
- `record_gainSettings`
- `update_gainSettings`
- `new_GRdBValue`
- `new_lnaValue`
- `new_agcSetting`
- `set_ifgainReduction`
- `set_lnagainReduction`
- `set_agcControl`
- `set_debugControl`
- `set_ppmControl`
- `set_antennaSelect`
- `set_tunerSelect`
- `set_xmlDump`
- `voidSignal`
- `biasT_selectorHandler`

### `_old_files/devices/sdrplay-handler-v2/sdrplayselect.cpp`

- `sdrplaySelect::sdrplaySelect`
- `sdrplaySelect::~sdrplaySelect`
- `sdrplaySelect::addtoList`
- `sdrplaySelect::select_rsp`

### `_old_files/devices/sdrplay-handler-v2/sdrplayselect.h`

- `sdrplaySelect::sdrplaySelect`
- `sdrplaySelect::~sdrplaySelect`
- `sdrplaySelect::addtoList`
- `sdrplaySelect::select_rsp`

### `_old_files/devices/sdrplay-handler-v2/xml-handler.cpp`

- `xmlHandler::xmlHandler`
- `xmlHandler::~xmlHandler`
- `xmlHandler::computeHeader`
- `xmlHandler::add`
- `xmlHandler::create_xmltree`

### `_old_files/devices/sdrplay-handler-v2/xml-handler.h`

- `Blocks::Blocks`
- `xmlHandler::xmlHandler`
- `xmlHandler::~xmlHandler`
- `xmlHandler::add`
- `xmlHandler::computeHeader`
- `xmlHandler::create_xmltree`

### `_old_files/devices/soapy/SoapySDR/Config.h`

- No function declarations detected

### `_old_files/devices/soapy/SoapySDR/Config.hpp`

- No function declarations detected

### `_old_files/devices/soapy/SoapySDR/Constants.h`

- No function declarations detected

### `_old_files/devices/soapy/SoapySDR/ConverterPrimitives.hpp`

- `F32toS32`
- `S32toF32`
- `F32toS16`
- `S16toF32`
- `F32toS8`
- `S8toF32`
- `U32toS32`
- `S32toU32`
- `U16toS16`
- `S16toU16`
- `U8toS8`
- `S8toU8`
- `S32toS16`
- `S16toS32`
- `S16toS8`
- `S8toS16`
- `F32toU32`
- `U32toF32`
- `F32toU16`
- `U16toF32`
- `F32toU8`
- `U8toF32`
- `S32toU16`
- `U16toS32`
- `S32toU8`
- `U8toS32`
- `S16toU8`
- `U8toS16`
- `S8toU16`
- `U16toS8`

### `_old_files/devices/soapy/SoapySDR/ConverterRegistry.hpp`

- `ConverterRegistry`
- `listTargetFormats`
- `listSourceFormats`
- `listPriorities`
- `getFunction`
- `listAvailableSourceFormats`

### `_old_files/devices/soapy/SoapySDR/Device.h`

- `SoapySDRDevice_lastStatus`
- `SoapySDRDevice_lastError`
- `SoapySDRDevice_enumerate`
- `SoapySDRDevice_enumerateStrArgs`
- `SoapySDRDevice_make`
- `SoapySDRDevice_makeStrArgs`
- `SoapySDRDevice_unmake`
- `SoapySDRDevice_make_list`
- `SoapySDRDevice_unmake_list`
- `SoapySDRDevice_getDriverKey`
- `SoapySDRDevice_getHardwareKey`
- `SoapySDRDevice_getHardwareInfo`
- `SoapySDRDevice_setFrontendMapping`
- `SoapySDRDevice_getFrontendMapping`
- `SoapySDRDevice_getNumChannels`
- `SoapySDRDevice_getChannelInfo`
- `SoapySDRDevice_getFullDuplex`
- `SoapySDRDevice_getStreamFormats`
- `SoapySDRDevice_getNativeStreamFormat`
- `SoapySDRDevice_getStreamArgsInfo`
- `SoapySDRDevice_setupStream`
- `SoapySDRDevice_closeStream`
- `SoapySDRDevice_getStreamMTU`
- `SoapySDRDevice_activateStream`
- `SoapySDRDevice_deactivateStream`
- `SoapySDRDevice_readStream`
- `SoapySDRDevice_writeStream`
- `SoapySDRDevice_readStreamStatus`
- `SoapySDRDevice_getNumDirectAccessBuffers`
- `SoapySDRDevice_getDirectAccessBufferAddrs`
- `SoapySDRDevice_acquireReadBuffer`
- `SoapySDRDevice_releaseReadBuffer`
- `SoapySDRDevice_acquireWriteBuffer`
- `SoapySDRDevice_releaseWriteBuffer`
- `SoapySDRDevice_listAntennas`
- `SoapySDRDevice_setAntenna`
- `SoapySDRDevice_getAntenna`
- `SoapySDRDevice_hasDCOffsetMode`
- `SoapySDRDevice_setDCOffsetMode`
- `SoapySDRDevice_getDCOffsetMode`
- `SoapySDRDevice_hasDCOffset`
- `SoapySDRDevice_setDCOffset`
- `SoapySDRDevice_getDCOffset`
- `SoapySDRDevice_hasIQBalance`
- `SoapySDRDevice_setIQBalance`
- `SoapySDRDevice_getIQBalance`
- `SoapySDRDevice_hasFrequencyCorrection`
- `SoapySDRDevice_setFrequencyCorrection`
- `SoapySDRDevice_getFrequencyCorrection`
- `SoapySDRDevice_listGains`
- `SoapySDRDevice_hasGainMode`
- `SoapySDRDevice_setGainMode`
- `SoapySDRDevice_getGainMode`
- `SoapySDRDevice_setGain`
- `SoapySDRDevice_setGainElement`
- `SoapySDRDevice_getGain`
- `SoapySDRDevice_getGainElement`
- `SoapySDRDevice_getGainRange`
- `SoapySDRDevice_getGainElementRange`
- `SoapySDRDevice_setFrequency`
- `SoapySDRDevice_setFrequencyComponent`
- `SoapySDRDevice_getFrequency`
- `SoapySDRDevice_getFrequencyComponent`
- `SoapySDRDevice_listFrequencies`
- `SoapySDRDevice_getFrequencyRange`
- `SoapySDRDevice_getFrequencyRangeComponent`
- `SoapySDRDevice_getFrequencyArgsInfo`
- `SoapySDRDevice_setSampleRate`
- `SoapySDRDevice_getSampleRate`
- `SoapySDRDevice_listSampleRates`
- `SoapySDRDevice_getSampleRateRange`
- `SoapySDRDevice_setBandwidth`
- `SoapySDRDevice_getBandwidth`
- `SoapySDRDevice_listBandwidths`
- `SoapySDRDevice_getBandwidthRange`
- `SoapySDRDevice_setMasterClockRate`
- `SoapySDRDevice_getMasterClockRate`
- `SoapySDRDevice_getMasterClockRates`
- `SoapySDRDevice_listClockSources`
- `SoapySDRDevice_setClockSource`
- `SoapySDRDevice_getClockSource`
- `SoapySDRDevice_listTimeSources`
- `SoapySDRDevice_setTimeSource`
- `SoapySDRDevice_getTimeSource`
- `SoapySDRDevice_hasHardwareTime`
- `SoapySDRDevice_getHardwareTime`
- `SoapySDRDevice_setHardwareTime`
- `SoapySDRDevice_setCommandTime`
- `SoapySDRDevice_listSensors`
- `SoapySDRDevice_getSensorInfo`
- `SoapySDRDevice_readSensor`
- `SoapySDRDevice_listChannelSensors`
- `SoapySDRDevice_getChannelSensorInfo`
- `SoapySDRDevice_readChannelSensor`
- `SoapySDRDevice_listRegisterInterfaces`
- `SoapySDRDevice_writeRegister`
- `SoapySDRDevice_readRegister`
- `SoapySDRDevice_writeRegisters`
- `SoapySDRDevice_readRegisters`
- `SoapySDRDevice_getSettingInfo`
- `SoapySDRDevice_writeSetting`
- `SoapySDRDevice_readSetting`
- `SoapySDRDevice_getChannelSettingInfo`
- `SoapySDRDevice_writeChannelSetting`
- `SoapySDRDevice_readChannelSetting`
- `SoapySDRDevice_listGPIOBanks`
- `SoapySDRDevice_writeGPIO`
- `SoapySDRDevice_writeGPIOMasked`
- `SoapySDRDevice_readGPIO`
- `SoapySDRDevice_writeGPIODir`
- `SoapySDRDevice_writeGPIODirMasked`
- `SoapySDRDevice_readGPIODir`
- `SoapySDRDevice_writeI2C`
- `SoapySDRDevice_readI2C`
- `SoapySDRDevice_transactSPI`
- `SoapySDRDevice_listUARTs`
- `SoapySDRDevice_writeUART`
- `SoapySDRDevice_readUART`

### `_old_files/devices/soapy/SoapySDR/Device.hpp`

- `~Device`
- `enumerate`
- `make`
- `unmake`
- `getDriverKey`
- `getHardwareKey`
- `getHardwareInfo`
- `setFrontendMapping`
- `getFrontendMapping`
- `getNumChannels`
- `getChannelInfo`
- `getFullDuplex`
- `getStreamFormats`
- `getNativeStreamFormat`
- `getStreamArgsInfo`
- `setupStream`
- `closeStream`
- `getStreamMTU`
- `activateStream`
- `deactivateStream`
- `readStream`
- `writeStream`
- `readStreamStatus`
- `getNumDirectAccessBuffers`
- `getDirectAccessBufferAddrs`
- `acquireReadBuffer`
- `releaseReadBuffer`
- `acquireWriteBuffer`
- `releaseWriteBuffer`
- `listAntennas`
- `setAntenna`
- `getAntenna`
- `hasDCOffsetMode`
- `setDCOffsetMode`
- `getDCOffsetMode`
- `hasDCOffset`
- `setDCOffset`
- `getDCOffset`
- `hasIQBalance`
- `setIQBalance`
- `getIQBalance`
- `hasFrequencyCorrection`
- `setFrequencyCorrection`
- `getFrequencyCorrection`
- `listGains`
- `hasGainMode`
- `setGainMode`
- `getGainMode`
- `setGain`
- `getGain`
- `getGainRange`
- `setFrequency`
- `getFrequency`
- `listFrequencies`
- `getFrequencyRange`
- `getFrequencyArgsInfo`
- `setSampleRate`
- `getSampleRate`
- `listSampleRates`
- `getSampleRateRange`
- `setBandwidth`
- `getBandwidth`
- `listBandwidths`
- `getBandwidthRange`
- `setMasterClockRate`
- `getMasterClockRate`
- `getMasterClockRates`
- `listClockSources`
- `setClockSource`
- `getClockSource`
- `listTimeSources`
- `setTimeSource`
- `getTimeSource`
- `hasHardwareTime`
- `getHardwareTime`
- `setHardwareTime`
- `setCommandTime`
- `listSensors`
- `getSensorInfo`
- `readSensor`
- `listRegisterInterfaces`
- `writeRegister`
- `readRegister`
- `writeRegisters`
- `readRegisters`
- `getSettingInfo`
- `writeSetting`
- `readSetting`
- `listGPIOBanks`
- `writeGPIO`
- `readGPIO`
- `writeGPIODir`
- `readGPIODir`
- `writeI2C`
- `readI2C`
- `transactSPI`
- `listUARTs`
- `writeUART`
- `readUART`

### `_old_files/devices/soapy/SoapySDR/Errors.h`

- `SoapySDR_errToStr`

### `_old_files/devices/soapy/SoapySDR/Errors.hpp`

- `errToStr`

### `_old_files/devices/soapy/SoapySDR/Formats.h`

- `SoapySDR_formatToSize`

### `_old_files/devices/soapy/SoapySDR/Formats.hpp`

- `formatToSize`

### `_old_files/devices/soapy/SoapySDR/Logger.h`

- `SoapySDR_log`
- `SoapySDR_vlogf`
- `SoapySDR_logf`
- `va_start`
- `va_end`
- `SoapySDR_registerLogHandler`
- `SoapySDR_setLogLevel`

### `_old_files/devices/soapy/SoapySDR/Logger.hpp`

- `log`
- `vlogf`
- `logf`
- `va_start`
- `SoapySDR::vlogf`
- `va_end`
- `registerLogHandler`
- `setLogLevel`

### `_old_files/devices/soapy/SoapySDR/Modules.h`

- `SoapySDR_getRootPath`
- `SoapySDR_listSearchPaths`
- `SoapySDR_listModules`
- `SoapySDR_listModulesPath`
- `SoapySDR_loadModule`
- `SoapySDR_getLoaderResult`
- `SoapySDR_getModuleVersion`
- `SoapySDR_unloadModule`
- `SoapySDR_loadModules`

### `_old_files/devices/soapy/SoapySDR/Modules.hpp`

- `getRootPath`
- `listSearchPaths`
- `listModules`
- `loadModule`
- `getLoaderResult`
- `getModuleVersion`
- `unloadModule`
- `loadModules`
- `ModuleVersion`

### `_old_files/devices/soapy/SoapySDR/Registry.hpp`

- `Registry`
- `~Registry`
- `listFindFunctions`
- `listMakeFunctions`

### `_old_files/devices/soapy/SoapySDR/Time.h`

- `SoapySDR_ticksToTimeNs`
- `SoapySDR_timeNsToTicks`

### `_old_files/devices/soapy/SoapySDR/Time.hpp`

- `ticksToTimeNs`
- `timeNsToTicks`
- `SoapySDR::ticksToTimeNs`
- `SoapySDR::timeNsToTicks`

### `_old_files/devices/soapy/SoapySDR/Types.h`

- `SoapySDRKwargs_fromString`
- `SoapySDRKwargs_toString`
- `SoapySDRStrings_clear`
- `SoapySDRKwargs_set`
- `SoapySDRKwargs_get`
- `SoapySDRKwargs_clear`
- `SoapySDRKwargsList_clear`
- `SoapySDRArgInfo_clear`
- `SoapySDRArgInfoList_clear`

### `_old_files/devices/soapy/SoapySDR/Types.hpp`

- `KwargsFromString`
- `KwargsToString`
- `Range`
- `minimum`
- `maximum`
- `step`
- `ArgInfo`
- `SoapySDR::Range::minimum`
- `SoapySDR::Range::maximum`
- `SoapySDR::Range::step`

### `_old_files/devices/soapy/SoapySDR/Version.h`

- `SoapySDR_getAPIVersion`
- `SoapySDR_getABIVersion`
- `SoapySDR_getLibVersion`

### `_old_files/devices/soapy/SoapySDR/Version.hpp`

- `getAPIVersion`
- `getABIVersion`
- `getLibVersion`

### `_old_files/devices/soapy/soapy-handler.cpp`

- `soapyHandler::soapyHandler`
- `soapyHandler::~soapyHandler`
- `contains`
- `soapyHandler::createDevice`
- `soapyHandler::setVFOFrequency`
- `soapyHandler::getVFOFrequency`
- `soapyHandler::defaultFrequency`
- `soapyHandler::stopReader`
- `soapyHandler::Samples`
- `soapyHandler::resetBuffer`
- `soapyHandler::handle_spinBox_2`
- `soapyHandler::set_agcControl`
- `soapyHandler::handleAntenna`
- `soapyHandler::show`
- `soapyHandler::hide`
- `soapyHandler::isHidden`

### `_old_files/devices/soapy/soapy-handler.h`

- `soapyHandler`
- `~soapyHandler`
- `setVFOFrequency`
- `getVFOFrequency`
- `defaultFrequency`
- `restartReader`
- `stopReader`
- `getSamples`
- `Samples`
- `resetBuffer`
- `show`
- `hide`
- `isHidden`
- `bitDepth`
- `createDevice`
- `handle_spinBox_1`
- `handle_spinBox_2`
- `set_agcControl`
- `handleAntenna`

### `_old_files/devices/soapy/soapy-worker.cpp`

- `soapyWorker::soapyWorker`

### `_old_files/devices/soapy/soapy-worker.h`

- `soapyWorker::soapyWorker`
- `soapyWorker::~soapyWorker`
- `soapyWorker::Samples`
- `soapyWorker::getSamples`

### `_old_files/devices/soapy/soapy_CF32.cpp`

- `soapy_CF32::soapy_CF32`
- `soapy_CF32::~soapy_CF32`
- `soapy_CF32::Samples`
- `soapy_CF32::getSamples`
- `soapy_CF32::run`

### `_old_files/devices/soapy/soapy_CF32.h`

- `soapy_CF32::soapy_CF32`
- `soapy_CF32::~soapy_CF32`
- `soapy_CF32::Samples`
- `soapy_CF32::getSamples`
- `soapy_CF32::run`

### `_old_files/devices/soapy/soapy_CS16.cpp`

- `soapy_CS16::soapy_CS16`
- `soapy_CS16::~soapy_CS16`
- `soapy_CS16::Samples`
- `soapy_CS16::getSamples`
- `soapy_CS16::run`

### `_old_files/devices/soapy/soapy_CS16.h`

- `soapy_CS16`
- `~soapy_CS16`
- `Samples`
- `getSamples`
- `run`

### `_old_files/devices/soapy/soapy_CS8.cpp`

- `soapy_CS8::soapy_CS8`
- `soapy_CS8::~soapy_CS8`
- `soapy_CS8::Samples`
- `soapy_CS8::getSamples`
- `soapy_CS8::run`

### `_old_files/devices/soapy/soapy_CS8.h`

- `soapy_CS8::soapy_CS8`
- `soapy_CS8::~soapy_CS8`
- `soapy_CS8::Samples`
- `soapy_CS8::getSamples`
- `soapy_CS8::run`

### `_old_files/fft/_kiss_fft_guts.h`

- No function declarations detected

### `_old_files/fft/fft-complex.cpp`

- `Fft_transform`
- `Fft_transformRadix2`
- `Fft_transformBluestein`
- `Fft_convolve`
- `reverse_bits`
- `memdup`

### `_old_files/fft/fft-complex.h`

- `Fft_transform`
- `Fft_transformRadix2`
- `Fft_transformBluestein`
- `Fft_convolve`

### `_old_files/fft/fft-handler.cpp`

- `FftHandler::FftHandler`
- `FftHandler::~FftHandler`
- `FftHandler::fft`

### `_old_files/fft/fft-handler.h`

- `FftHandler`
- `~FftHandler`
- `fft`

### `_old_files/fft/kiss_fft.c`

- `kf_bfly2`
- `kf_bfly4`
- `kf_bfly3`
- `kf_bfly5`
- `kf_bfly_generic`
- `kf_work`
- `kf_factor`
- `kiss_fft_alloc`
- `kiss_fft_stride`
- `kiss_fft`
- `kiss_fft_cleanup`
- `kiss_fft_next_fast_size`

### `_old_files/fft/kiss_fft.h`

- `kiss_fft_alloc`
- `kiss_fft`
- `kiss_fft_stride`
- `kiss_fft_cleanup`
- `kiss_fft_next_fast_size`

### `_old_files/fft/kiss_fftr.c`

- `kiss_fftr_alloc`
- `kiss_fftr`
- `kiss_fftri`

### `_old_files/fft/kiss_fftr.h`

- `kiss_fftr_alloc`
- `kiss_fftr`
- `kiss_fftri`

### `_old_files/helpers/map/mapConverter.cpp`

- `main`

### `_old_files/helpers/picture-converter/pictureConverter.cpp`

- `main`

### `_old_files/main/country-codes.h`

- No function declarations detected

### `_old_files/output/Qt-audio.cpp`

- `Qt_Audio::Qt_Audio`
- `Qt_Audio::_initialize_deviceList`
- `Qt_Audio::get_audio_devices_list`
- `Qt_Audio::audioOutput`
- `Qt_Audio::_initializeAudio`
- `Qt_Audio::stop`
- `Qt_Audio::restart`
- `Qt_Audio::selectDevice`
- `Qt_Audio::suspend`
- `Qt_Audio::resume`
- `Qt_Audio::setVolume`

### `_old_files/output/Qt-audio.h`

- `Qt_Audio`
- `~Qt_Audio`
- `stop`
- `restart`
- `suspend`
- `resume`
- `audioOutput`
- `selectDevice`
- `setVolume`
- `get_audio_devices_list`
- `_initialize_deviceList`
- `_initializeAudio`

### `_old_files/output/Qt-audiodevice.cpp`

- `QtAudioDevice::QtAudioDevice`
- `QtAudioDevice::start`
- `QtAudioDevice::stop`
- `QtAudioDevice::readData`
- `QtAudioDevice::writeData`

### `_old_files/output/Qt-audiodevice.h`

- `QtAudioDevice`
- `~QtAudioDevice`
- `start`
- `stop`
- `readData`
- `writeData`

### `_old_files/output/audio-base.cpp`

- `AudioBase::AudioBase`
- `AudioBase::restart`
- `AudioBase::stop`
- `AudioBase::audioOut`
- `AudioBase::audioOut_16000`
- `AudioBase::audioOut_24000`
- `AudioBase::audioOut_32000`
- `AudioBase::audioOut_48000`
- `AudioBase::startDumping`
- `AudioBase::stopDumping`
- `AudioBase::audioReady`
- `AudioBase::audioOutput`
- `AudioBase::hasMissed`

### `_old_files/output/audio-base.h`

- `AudioBase`
- `~AudioBase`
- `stop`
- `restart`
- `audioOut`
- `startDumping`
- `stopDumping`
- `hasMissed`
- `audioOut_16000`
- `audioOut_24000`
- `audioOut_32000`
- `audioOut_48000`
- `audioReady`
- `audioOutput`

### `_old_files/output/audio-player.cpp`

- `audioPlayer::audioPlayer`
- `audioPlayer::stop`

### `_old_files/output/audio-player.h`

- `audioPlayer`
- `~audioPlayer`
- `audioOutput`
- `stop`
- `restart`
- `suspend`
- `resume`
- `selectDevice`
- `hasMissed`
- `missed`

### `_old_files/output/audiofifo.cpp`

- No function declarations detected

### `_old_files/output/audiosink.cpp`

- `AudioSink::AudioSink`
- `AudioSink::~AudioSink`
- `AudioSink::selectDevice`
- `AudioSink::restart`
- `AudioSink::stop`
- `AudioSink::OutputrateIsSupported`
- `AudioSink::paCallback_o`
- `AudioSink::hasMissed`
- `AudioSink::missed`
- `AudioSink::audioOutput`
- `AudioSink::outputChannelwithRate`
- `AudioSink::invalidDevice`
- `AudioSink::isValidDevice`
- `AudioSink::selectDefaultDevice`
- `AudioSink::cardRate`
- `AudioSink::setupChannels`
- `AudioSink::numberofDevices`

### `_old_files/output/audiosink.h`

- `AudioSink`
- `~AudioSink`
- `setupChannels`
- `stop`
- `restart`
- `selectDevice`
- `selectDefaultDevice`
- `missed`
- `hasMissed`
- `numberofDevices`
- `outputChannelwithRate`
- `invalidDevice`
- `isValidDevice`
- `cardRate`
- `OutputrateIsSupported`
- `audioOutput`
- `paCallback_o`

### `_old_files/output/newconverter.cpp`

- `newConverter::newConverter`
- `newConverter::~newConverter`
- `newConverter::convert`
- `newConverter::getMaxOutputsize`

### `_old_files/output/newconverter.h`

- `newConverter`
- `~newConverter`
- `convert`
- `getMaxOutputsize`

### `_old_files/output/rtp-streamer.h`

- `rtpStreamer::rtpStreamer`
- `rtpStreamer::~rtpStreamer`
- `rtpStreamer::audioOutput`
- `rtpStreamer::sendBuffer`

### `_old_files/output/tcp-streamer.cpp`

- `tcpStreamer::tcpStreamer`
- `tcpStreamer::~tcpStreamer`
- `tcpStreamer::acceptConnection`
- `tcpStreamer::processSamples`

### `_old_files/output/tcp-streamer.h`

- `tcpStreamer::tcpStreamer`
- `tcpStreamer::~tcpStreamer`
- `tcpStreamer::audioOutput`
- `tcpStreamer::acceptConnection`
- `tcpStreamer::processSamples`
- `tcpStreamer::handleSamples`

### `_old_files/sound-client/audiosink.cpp`

- `audioSink::audioSink`
- `audioSink::~audioSink`
- `audioSink::selectDevice`
- `audioSink::restart`
- `audioSink::stop`
- `audioSink::OutputrateIsSupported`
- `audioSink::paCallback_o`
- `audioSink::putSample`
- `audioSink::putSamples`
- `audioSink::numberofDevices`
- `audioSink::outputChannelwithRate`
- `audioSink::invalidDevice`
- `audioSink::isValidDevice`
- `audioSink::selectDefaultDevice`

### `_old_files/sound-client/audiosink.h`

- `audioSink::audioSink`
- `audioSink::~audioSink`
- `audioSink::numberofDevices`
- `audioSink::outputChannelwithRate`
- `audioSink::stop`
- `audioSink::restart`
- `audioSink::putSample`
- `audioSink::putSamples`
- `audioSink::invalidDevice`
- `audioSink::isValidDevice`
- `audioSink::selectDefaultDevice`
- `audioSink::selectDevice`
- `audioSink::OutputrateIsSupported`
- `audioSink::paCallback_o`

### `_old_files/sound-client/main.cpp`

- `main`

### `_old_files/sound-client/ringbuffer.h`

- `RingBuffer`
- `~RingBuffer`
- `GetRingBufferReadAvailable`
- `GetRingBufferWriteAvailable`
- `WriteSpace`
- `FlushRingBuffer`
- `AdvanceRingBufferWriteIndex`
- `PaUtil_WriteMemoryBarrier`
- `AdvanceRingBufferReadIndex`
- `PaUtil_FullMemoryBarrier`
- `GetRingBufferWriteRegions`
- `GetRingBufferReadRegions`
- `putDataIntoBuffer`
- `getDataFromBuffer`
- `skipDataInBuffer`

### `_old_files/sound-client/sound-client.cpp`

- `soundClient::soundClient`
- `soundClient::~soundClient`
- `soundClient::wantConnect`
- `soundClient::setConnection`
- `soundClient::readData`
- `makeComplex`
- `soundClient::toBuffer`
- `soundClient::setupSoundOut`
- `soundClient::setStreamOutSelector`
- `soundClient::setGain`
- `soundClient::terminate`
- `soundClient::timerTick`

### `_old_files/sound-client/sound-client.h`

- `soundClient`
- `~soundClient`
- `wantConnect`
- `setConnection`
- `readData`
- `toBuffer`
- `terminate`
- `setGain`
- `setStreamOutSelector`
- `timerTick`
- `setupSoundOut`

### `_old_files/sound-client/sound-constants.h`

- `isInfinite`
- `get_db`

### `_old_files/support/converter_48000.cpp`

- `converter_48000::converter_48000`
- `converter_48000::convert`
- `converter_48000::convert_16000`
- `converter_48000::convert_24000`
- `converter_48000::convert_32000`
- `converter_48000::convert_48000`

### `_old_files/support/converter_48000.h`

- `converter_48000`
- `~converter_48000`
- `convert`
- `convert_16000`
- `convert_24000`
- `convert_32000`
- `convert_48000`

### `_old_files/support/dab-params.cpp`

- `DabParams::DabParams`

### `_old_files/support/dab-params.h`

- `DabParams`
- `~DabParams`
- `get_dab_par`

### `audio/audiofifo.h`

- No function declarations detected

### `audio/audioiodevice.cpp`

- `AudioIODevice::set_buffer`
- `AudioIODevice::start`
- `AudioIODevice::stop`
- `AudioIODevice::_fade`
- `AudioIODevice::_fade_in_audio_samples`
- `AudioIODevice::_fade_out_audio_samples`
- `AudioIODevice::readData`
- `AudioIODevice::writeData`
- `AudioIODevice::bytesAvailable`
- `AudioIODevice::size`
- `AudioIODevice::set_mute_state`
- `AudioIODevice::_eval_peak_audio_level`

### `audio/audioiodevice.h`

- `AudioIODevice`
- `start`
- `stop`
- `set_buffer`
- `set_mute_state`
- `is_muted`
- `writeData`
- `bytesAvailable`
- `size`
- `std::pow`
- `_fade`
- `_fade_in_audio_samples`
- `_fade_out_audio_samples`
- `_eval_peak_audio_level`
- `signal_show_audio_peak_level`
- `signal_audio_data_available`

### `audio/audiooutput.h`

- `~IAudioOutput`
- `get_audio_io_device`
- `get_audio_device_list`
- `slot_start`
- `slot_restart`
- `slot_stop`
- `slot_mute`
- `slot_setVolume`
- `slot_set_audio_device`
- `signal_audio_devices_list`
- `signal_audio_device_changed`

### `audio/audiooutputqt.cpp`

- `AudioOutputQt::~AudioOutputQt`
- `AudioOutputQt::slot_start`
- `AudioOutputQt::_print_audio_device_formats`
- `AudioOutputQt::slot_restart`
- `AudioOutputQt::slot_mute`
- `AudioOutputQt::slot_setVolume`
- `AudioOutputQt::slot_set_audio_device`
- `AudioOutputQt::slot_stop`
- `AudioOutputQt::_do_stop`
- `AudioOutputQt::_do_restart`
- `AudioOutputQt::_slot_restart_deferred`
- `AudioOutputQt::_slot_stop_deferred`
- `AudioOutputQt::_slot_state_changed`
- `AudioOutputQt::get_audio_device_list`
- `AudioOutputQt::_slot_update_audio_devices`

### `audio/audiooutputqt.h`

- `AudioOutputQt`
- `~AudioOutputQt`
- `get_audio_io_device`
- `_do_stop`
- `_do_restart`
- `_print_audio_device_formats`
- `slot_start`
- `slot_restart`
- `slot_stop`
- `slot_mute`
- `slot_setVolume`
- `slot_set_audio_device`
- `_slot_restart_deferred`
- `_slot_stop_deferred`
- `_slot_state_changed`
- `_slot_update_audio_devices`

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

### `backend/backend-deconvolver.cpp`

- `BackendDeconvolver::BackendDeconvolver`
- `BackendDeconvolver::deconvolve`

### `backend/backend-deconvolver.h`

- `BackendDeconvolver`
- `~BackendDeconvolver`
- `deconvolve`

### `backend/backend-driver.cpp`

- `BackendDriver::BackendDriver`
- `BackendDriver::add_to_frame`

### `backend/backend-driver.h`

- `BackendDriver`
- `~BackendDriver`
- `add_to_frame`

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

### `backend/charsets.cpp`

- `to_QString_using_charset`

### `backend/charsets.h`

- `is_charset_valid`
- `to_QString_using_charset`

### `backend/crc.cpp`

- `calc_crc`
- `check_crc_bytes`
- `check_CRC_bits`

### `backend/crc.h`

- `calc_crc`
- `check_crc_bytes`
- `check_CRC_bits`

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

### `backend/data/epg/epgdec.cpp`

- `CEPGDecoder::decode`
- `get_uint16`
- `get_uint24`
- `tag_length_value::tag_length_value`
- `decode_genre_href`
- `decode_string`
- `decode_uint16`
- `decode_uint24`
- `decode_sid`
- `decode_dateandtime`
- `decode_duration`
- `decode_bitrate`
- `decode_attribute_name`
- `decode_attribute_value`
- `attribute`
- `string_token_table`
- `CModJulDate::Set`

### `backend/data/epg/epgdec.h`

- `tag_length_value::tag_length_value`
- `tag_length_value::is_cdata`
- `CEPGDecoder::CEPGDecoder`
- `CEPGDecoder::decode`
- `CModJulDate`
- `GetYear`

### `backend/data/epg-2/epg-decoder.cpp`

- `EpgDecoder::EpgDecoder`
- `EpgDecoder::getBit`
- `EpgDecoder::getBits`
- `EpgDecoder::process_epg`
- `EpgDecoder::process_programGroups`
- `EpgDecoder::process_programGroup`
- `EpgDecoder::process_schedule`
- `EpgDecoder::process_program`
- `EpgDecoder::process_scope`
- `EpgDecoder::process_serviceScope`
- `EpgDecoder::process_mediaDescription`
- `EpgDecoder::process_ensemble`
- `EpgDecoder::process_service`
- `EpgDecoder::process_location`
- `EpgDecoder::process_bearer`
- `EpgDecoder::process_geoLocation`
- `EpgDecoder::process_programmeEvent`
- `EpgDecoder::process_onDemand`
- `EpgDecoder::process_genre`
- `EpgDecoder::process_keyWords`
- `EpgDecoder::process_link`
- `EpgDecoder::process_shortName`
- `EpgDecoder::process_mediumName`
- `EpgDecoder::process_longName`
- `EpgDecoder::process_shortDescription`
- `EpgDecoder::process_longDescription`
- `EpgDecoder::process_multiMedia`
- `EpgDecoder::process_radiodns`
- `EpgDecoder::process_time`
- `EpgDecoder::process_relativeTime`
- `EpgDecoder::process_memberOf`
- `EpgDecoder::process_presentationTime`
- `EpgDecoder::process_acquisitionTime`
- `EpgDecoder::process_country`
- `EpgDecoder::process_point`
- `EpgDecoder::process_polygon`
- `EpgDecoder::process_412`
- `EpgDecoder::process_440`
- `EpgDecoder::process_46`
- `EpgDecoder::process_471`
- `EpgDecoder::process_472`
- `EpgDecoder::process_473`
- `EpgDecoder::process_474`
- `EpgDecoder::process_475`
- `EpgDecoder::process_476`
- `EpgDecoder::process_481`
- `EpgDecoder::process_482`
- `EpgDecoder::process_483`
- `EpgDecoder::process_484`
- `EpgDecoder::process_485`
- `EpgDecoder::process_4171`
- `EpgDecoder::process_tokenTable`
- `EpgDecoder::process_token`
- `EpgDecoder::process_defaultLanguage`
- `EpgDecoder::process_obsolete`
- `EpgDecoder::record`
- `EpgDecoder::getCData`

### `backend/data/epg-2/epg-decoder.h`

- `progDesc`
- `clean`
- `EpgDecoder`
- `~EpgDecoder`
- `process_epg`
- `getBit`
- `getBits`
- `process_programGroups`
- `process_programGroup`
- `process_schedule`
- `process_program`
- `process_scope`
- `process_serviceScope`
- `process_mediaDescription`
- `process_ensemble`
- `process_service`
- `process_location`
- `process_bearer`
- `process_geoLocation`
- `process_programmeEvent`
- `process_onDemand`
- `process_genre`
- `process_keyWords`
- `process_link`
- `process_shortName`
- `process_mediumName`
- `process_longName`
- `process_shortDescription`
- `process_longDescription`
- `process_multiMedia`
- `process_radiodns`
- `process_time`
- `process_relativeTime`
- `process_memberOf`
- `process_presentationTime`
- `process_acquisitionTime`
- `process_country`
- `process_point`
- `process_polygon`
- `process_412`
- `process_440`
- `process_46`
- `process_471`
- `process_472`
- `process_473`
- `process_474`
- `process_475`
- `process_476`
- `process_481`
- `process_482`
- `process_483`
- `process_484`
- `process_485`
- `process_4171`
- `process_tokenTable`
- `process_token`
- `process_defaultLanguage`
- `process_obsolete`
- `record`
- `getCData`
- `signal_set_epg_data`

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

### `backend/data/journaline/NML.cpp`

- `NMLFactory::CreateErrorDump`
- `NMLFactory::CreateError`
- `DumpRaw`
- `operator<<`
- `NML::Dump`
- `NMLFactory::getNextSection`
- `NMLFactory::extract_link_data`
- `NMLFactory::CreateNML`
- `NML::NML`
- `operator==`
- `NML::SetErrorDump`
- `HexDump`
- `Inflate`
- `RemoveNMLEscapeSequences::RemoveNMLEscapeSequences`
- `NMLEscapeSequences2HTML::NMLEscapeSequences2HTML`

### `backend/data/journaline/NML.h`

- `HexDump`
- `Convert`
- `~NMLEscapeCodeHandler`
- `RemoveNMLEscapeSequences`
- `~RemoveNMLEscapeSequences`
- `NMLEscapeSequences2HTML`
- `~NMLEscapeSequences2HTML`
- `NML`
- `~NML`
- `operator==`
- `operator<<`
- `get_news_ptr`
- `Dump`
- `isValid`
- `isRootObject`
- `isMenu`
- `isStatic`
- `GetObjectType`
- `GetObjectTypeString`
- `GetObjectId`
- `GetRevisionIndex`
- `GetExtendedHeader`
- `GetTitle`
- `GetLinkUrlData`
- `GetNrOfItems`
- `GetItems`
- `GetItemText`
- `GetLinkId`
- `isLinkIdAvailable`
- `SetLinkAvailability`
- `SetError`
- `SetErrorDump`
- `DumpRaw`
- `CreateNML`
- `CreateError`
- `CreateErrorDump`
- `NMLFactory`
- `~NMLFactory`
- `operator=`
- `getNextSection`
- `extract_link_data`

### `backend/data/journaline/Splitter.cpp`

- `Splitter::Splitter`
- `Splitter::SetLineBreakCharacter`
- `Splitter::Split`

### `backend/data/journaline/Splitter.h`

- `Split`
- `~StringSplitter`
- `Splitter`
- `~Splitter`
- `SetLineBreakCharacter`

### `backend/data/journaline/cpplog.h`

- No function declarations detected

### `backend/data/journaline/crc_8_16.c`

- `CRC_Init_16`
- `CRC_Init_8`
- `CRC_Build_16`
- `CRC_Build_8`
- `CRC_Check_16`
- `CRC_Check_8`

### `backend/data/journaline/crc_8_16.h`

- `CRC_Build_16`
- `CRC_Build_8`
- `CRC_Check_16`
- `CRC_Check_8`

### `backend/data/journaline/dabdatagroupdecoder.h`

- `DAB_DATAGROUP_DECODER_createDec`
- `DAB_DATAGROUP_DECODER_deleteDec`
- `DAB_DATAGROUP_DECODER_putData`

### `backend/data/journaline/dabdgdec_impl.c`

- `DAB_DATAGROUP_DECODER_createDec`
- `DAB_DATAGROUP_DECODER_deleteDec`
- `DAB_DATAGROUP_DECODER_putData`
- `DAB_DGDEC_IMPL_checkCrc`
- `DAB_DGDEC_IMPL_extractMscDatagroupHeader`
- `DAB_DGDEC_IMPL_showMscDatagroupHeader`

### `backend/data/journaline/dabdgdec_impl.h`

- `DAB_DGDEC_IMPL_extractMscDatagroupHeader`
- `DAB_DGDEC_IMPL_showMscDatagroupHeader`
- `DAB_DGDEC_IMPL_checkCrc`

### `backend/data/journaline/log.c`

- `logit`

### `backend/data/journaline/log.h`

- `logit`

### `backend/data/journaline/newsobject.cpp`

- `NewsObject::NewsObject`
- `NewsObject::~NewsObject`
- `NewsObject::getObjectId`
- `NewsObject::setReceptionTime`
- `NewsObject::isStatic`
- `NewsObject::isCompressed`
- `NewsObject::isUpdated`
- `NewsObject::setUpdateFlag`
- `NewsObject::getRevisionIndex`
- `NewsObject::getObjectType`
- `NewsObject::convertObjectType`
- `NewsObject::copyNml`

### `backend/data/journaline/newsobject.h`

- `NewsObject`
- `~NewsObject`
- `getObjectId`
- `setReceptionTime`
- `isStatic`
- `isCompressed`
- `setUpdateFlag`
- `isUpdated`
- `getRevisionIndex`
- `getObjectType`
- `copyNml`
- `convertObjectType`

### `backend/data/journaline/newssvcdec.h`

- `NEWS_SVC_DEC_createDec`
- `NEWS_SVC_DEC_deleteDec`
- `NEWS_SVC_DEC_get_news_object`
- `NEWS_SVC_DEC_get_object_availability`
- `NEWS_SVC_DEC_putData`
- `NEWS_SVC_DEC_watch_objects`
- `NEWS_SVC_DEC_keep_in_cache`

### `backend/data/journaline/newssvcdec_impl.cpp`

- `NEWS_SVC_DEC_createDec`
- `NEWS_SVC_DEC_deleteDec`
- `NEWS_SVC_DEC_putData`
- `NEWS_SVC_DEC_IMPL_garbage_collection`
- `NEWS_SVC_DEC_get_news_object`
- `NEWS_SVC_DEC_watch_objects`
- `NEWS_SVC_DEC_get_object_availability`
- `NEWS_SVC_DEC_keep_in_cache`
- `NEWS_SVC_DEC_IMPL_printObjList`
- `FindMinFunctor::FindMinFunctor`
- `FindMinFunctor::~FindMinFunctor`
- `FindMinFunctor::operator`

### `backend/data/journaline/newssvcdec_impl.h`

- `NEWS_SVC_DEC_IMPL_garbage_collection`
- `NEWS_SVC_DEC_IMPL_printObjList`
- `FindMinFunctor`
- `~FindMinFunctor`
- `operator`

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

### `backend/data/virtual-datahandler.h`

- `VirtualDataHandler`
- `~VirtualDataHandler`
- `add_MSC_data_group`

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

### `backend/frame-processor.h`

- `FrameProcessor`
- `~FrameProcessor`
- `add_to_frame`

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

### `backend/mm_malloc.h`

- No function declarations detected

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

### `configuration/configuration.cpp`

- `Configuration::Configuration`
- `Configuration::save_position_and_config`
- `Configuration::_slot_handle_dc_corr`
- `Configuration::_slot_handle_dc_and_iq_corr`

### `configuration/configuration.h`

- `Configuration`
- `~Configuration`
- `save_position_and_config`
- `_slot_handle_dc_corr`
- `_slot_handle_dc_and_iq_corr`
- `signal_handle_dc_and_iq_corr`

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

### `decoder/fib-decoder-string-getter.cpp`

- `hex_str`
- `FibDecoder::_get_audio_data_str`
- `FibDecoder::_get_packet_data_str`

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

### `decoder/fib-table.h`

- No function declarations detected

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

### `devices/airspy-handler/airspy-handler.cpp`

- `AirspyHandler::AirspyHandler`
- `AirspyHandler::~AirspyHandler`
- `AirspyHandler::setVFOFrequency`
- `AirspyHandler::getVFOFrequency`
- `AirspyHandler::defaultFrequency`
- `AirspyHandler::set_filter`
- `AirspyHandler::restartReader`
- `AirspyHandler::stopReader`
- `AirspyHandler::callback`
- `AirspyHandler::data_available`
- `AirspyHandler::getSerial`
- `AirspyHandler::open`
- `AirspyHandler::resetBuffer`
- `AirspyHandler::getSamples`
- `AirspyHandler::Samples`
- `AirspyHandler::board_id_name`
- `AirspyHandler::load_airspyFunctions`
- `AirspyHandler::getBufferSpace`
- `AirspyHandler::deviceName`
- `AirspyHandler::startDumping`
- `AirspyHandler::setup_xmlDump`
- `AirspyHandler::stopDumping`
- `AirspyHandler::show`
- `AirspyHandler::hide`
- `AirspyHandler::isHidden`
- `AirspyHandler::record_gainSettings`
- `AirspyHandler::restore_gainSliders`
- `AirspyHandler::restore_gainSettings`
- `AirspyHandler::switch_tab`
- `AirspyHandler::set_linearity`
- `AirspyHandler::set_sensitivity`
- `AirspyHandler::set_lna_gain`
- `AirspyHandler::set_mixer_gain`
- `AirspyHandler::set_vga_gain`
- `AirspyHandler::set_lna_agc`
- `AirspyHandler::set_mixer_agc`
- `AirspyHandler::set_rf_bias`
- `AirspyHandler::hasDump`

### `devices/airspy-handler/airspy-handler.h`

- `AirspyHandler`
- `~AirspyHandler`
- `setVFOFrequency`
- `getVFOFrequency`
- `restartReader`
- `stopReader`
- `getSamples`
- `Samples`
- `resetBuffer`
- `show`
- `hide`
- `isHidden`
- `deviceName`
- `hasDump`
- `startDumping`
- `stopDumping`
- `defaultFrequency`
- `getBufferSpace`
- `setup_xmlDump`
- `record_gainSettings`
- `restore_gainSliders`
- `restore_gainSettings`
- `set_linearity`
- `set_sensitivity`
- `set_lna_gain`
- `set_mixer_gain`
- `set_vga_gain`
- `set_lna_agc`
- `set_mixer_agc`
- `set_rf_bias`
- `switch_tab`
- `set_filter`
- `new_tabSetting`
- `load_airspyFunctions`
- `board_id_name`
- `callback`
- `data_available`
- `getSerial`
- `open`

### `devices/airspy-handler/airspyfilter.cpp`

- `airspyFilter::airspyFilter`
- `airspyFilter::~airspyFilter`
- `airspyFilter::Pass`

### `devices/airspy-handler/airspyfilter.h`

- `airspyFilter::airspyFilter`
- `airspyFilter::~airspyFilter`
- `airspyFilter::Pass`

### `devices/colibri-handler/LibLoader.cpp`

- `LibLoader::load`
- `LibLoader::initialize`
- `LibLoader::finalize`
- `LibLoader::version`
- `LibLoader::information`
- `LibLoader::devices`
- `LibLoader::open`
- `LibLoader::close`
- `LibLoader::start`
- `LibLoader::stop`
- `LibLoader::setPream`
- `LibLoader::setFrequency`

### `devices/colibri-handler/LibLoader.h`

- `LibLoader::LibLoader`
- `LibLoader::load`
- `LibLoader::initialize`
- `LibLoader::finalize`
- `LibLoader::version`
- `LibLoader::information`
- `LibLoader::devices`
- `LibLoader::open`
- `LibLoader::close`
- `LibLoader::start`
- `LibLoader::stop`
- `LibLoader::setPream`
- `LibLoader::setFrequency`

### `devices/colibri-handler/colibri-handler.cpp`

- `colibriHandler::colibriHandler`
- `colibriHandler::~colibriHandler`
- `colibriHandler::setVFOFrequency`
- `colibriHandler::getVFOFrequency`
- `colibriHandler::set_gainControl`
- `colibriHandler::handle_iqSwitcher`
- `the_callBackRx`
- `colibriHandler::restartReader`
- `colibriHandler::stopReader`
- `colibriHandler::getSamples`
- `colibriHandler::Samples`
- `colibriHandler::resetBuffer`
- `colibriHandler::bitDepth`
- `colibriHandler::deviceName`
- `colibriHandler::sampleRate`
- `colibriHandler::show`
- `colibriHandler::hide`
- `colibriHandler::isHidden`

### `devices/colibri-handler/colibri-handler.h`

- `colibriHandler::colibriHandler`
- `colibriHandler::~colibriHandler`
- `colibriHandler::restartReader`
- `colibriHandler::stopReader`
- `colibriHandler::setVFOFrequency`
- `colibriHandler::getVFOFrequency`
- `colibriHandler::getSamples`
- `colibriHandler::Samples`
- `colibriHandler::resetBuffer`
- `colibriHandler::bitDepth`
- `colibriHandler::hide`
- `colibriHandler::show`
- `colibriHandler::isHidden`
- `colibriHandler::deviceName`
- `colibriHandler::sampleRate`
- `colibriHandler::set_gainControl`
- `colibriHandler::handle_iqSwitcher`

### `devices/colibri-handler/common.h`

- No function declarations detected

### `devices/device-exceptions.h`

- `std_exception_string`
- `what`

### `devices/device-handler.h`

- `~IDeviceHandler`
- `restartReader`
- `stopReader`
- `setVFOFrequency`
- `getVFOFrequency`
- `getSamples`
- `Samples`
- `resetBuffer`
- `hide`
- `show`
- `isHidden`
- `deviceName`
- `isFileInput`
- `hasDump`
- `startDumping`
- `stopDumping`

### `devices/device-selector.cpp`

- `DeviceSelector::DeviceSelector`
- `DeviceSelector::get_device_name_list`
- `DeviceSelector::create_device`
- `DeviceSelector::_create_device`
- `DeviceSelector::reset_file_input_last_file`

### `devices/device-selector.h`

- `DeviceSelector`
- `~DeviceSelector`
- `get_device_name_list`
- `create_device`
- `reset_file_input_last_file`
- `get_device_name`
- `_create_device`

### `devices/dongleselect.cpp`

- `dongleSelect::dongleSelect`
- `dongleSelect::~dongleSelect`
- `dongleSelect::selectDongle`

### `devices/dongleselect.h`

- `dongleSelect`
- `~dongleSelect`
- `addtoDongleList`
- `selectDongle`

### `devices/elad-files/elad-files.cpp`

- `eladFiles::eladFiles`
- `eladFiles::~eladFiles`
- `eladFiles::restartReader`
- `eladFiles::stopReader`
- `eladFiles::getSamples`
- `eladFiles::Samples`
- `eladFiles::setProgress`
- `eladFiles::show`
- `eladFiles::hide`
- `eladFiles::isHidden`
- `eladFiles::handle_iqButton`

### `devices/elad-files/elad-files.h`

- `eladFiles::eladFiles`
- `eladFiles::~eladFiles`
- `eladFiles::getSamples`
- `eladFiles::Samples`
- `eladFiles::restartReader`
- `eladFiles::stopReader`
- `eladFiles::show`
- `eladFiles::hide`
- `eladFiles::isHidden`
- `eladFiles::setProgress`
- `eladFiles::handle_iqButton`

### `devices/elad-files/elad-reader.cpp`

- `getMyTime`
- `eladReader::eladReader`
- `eladReader::~eladReader`
- `eladReader::stopReader`
- `eladReader::run`

### `devices/elad-files/elad-reader.h`

- `eladReader::eladReader`
- `eladReader::~eladReader`
- `eladReader::startReader`
- `eladReader::stopReader`
- `eladReader::run`
- `eladReader::setProgress`

### `devices/elad-s1-handler/elad-handler.cpp`

- `eladHandler::eladHandler`
- `eladHandler::~eladHandler`
- `eladHandler::defaultFrequency`
- `eladHandler::getVFOFrequency`
- `eladHandler::restartReader`
- `eladHandler::stopReader`
- `eladHandler::getSamples`
- `eladHandler::Samples`
- `eladHandler::resetBuffer`
- `eladHandler::bitDepth`
- `eladHandler::setGainReduction`
- `eladHandler::setFilter`
- `eladHandler::show_iqSwitch`
- `eladHandler::toggle_IQSwitch`
- `eladHandler::set_NyquistWidth`
- `eladHandler::set_Offset`

### `devices/elad-s1-handler/elad-handler.h`

- `eladHandler::eladHandler`
- `eladHandler::~eladHandler`
- `eladHandler::getVFOFrequency`
- `eladHandler::legalFrequency`
- `eladHandler::defaultFrequency`
- `eladHandler::restartReader`
- `eladHandler::stopReader`
- `eladHandler::getSamples`
- `eladHandler::Samples`
- `eladHandler::resetBuffer`
- `eladHandler::getRate`
- `eladHandler::bitDepth`
- `eladHandler::setGainReduction`
- `eladHandler::setFilter`
- `eladHandler::set_Offset`
- `eladHandler::set_NyquistWidth`
- `eladHandler::toggle_IQSwitch`
- `eladHandler::show_iqSwitch`

### `devices/elad-s1-handler/elad-loader.cpp`

- `eladLoader::eladLoader`
- `eladLoader::~eladLoader`
- `eladLoader::startUSB`
- `eladLoader::getHandle`
- `eladLoader::OK`

### `devices/elad-s1-handler/elad-loader.h`

- `eladLoader::eladLoader`
- `eladLoader::~eladLoader`
- `eladLoader::OK`
- `eladLoader::getHandle`
- `eladLoader::startUSB`

### `devices/elad-s1-handler/elad-worker.cpp`

- `eladWorker::eladWorker`
- `eladWorker::stop`
- `eladWorker::~eladWorker`
- `run`

### `devices/elad-s1-handler/elad-worker.h`

- `eladWorker::eladWorker`
- `eladWorker::~eladWorker`
- `eladWorker::stop`
- `eladWorker::run`

### `devices/extio-handler/common-readers.cpp`

- `reader_16::reader_16`
- `reader_16::~reader_16`
- `reader_16::processData`
- `reader_16::bitDepth`
- `reader_24::reader_24`
- `reader_24::~reader_24`
- `reader_24::processData`
- `reader_24::bitDepth`
- `reader_32::reader_32`
- `reader_32::~reader_32`
- `reader_32::processData`
- `reader_32::bitDepth`
- `reader_float::reader_float`
- `reader_float::~reader_float`
- `reader_float::processData`
- `reader_float::bitDepth`

### `devices/extio-handler/common-readers.h`

- `reader_16::reader_16`
- `reader_16::~reader_16`
- `reader_16::processData`
- `reader_16::bitDepth`
- `reader_24::reader_24`
- `reader_24::~reader_24`
- `reader_24::processData`
- `reader_24::bitDepth`
- `reader_32::reader_32`
- `reader_32::~reader_32`
- `reader_32::processData`
- `reader_32::bitDepth`
- `reader_float::reader_float`
- `reader_float::~reader_float`
- `reader_float::processData`
- `reader_float::bitDepth`

### `devices/extio-handler/extio-handler.cpp`

- `extioCallback`
- `extioHandler::extioHandler`
- `extioHandler::~extioHandler`
- `extioHandler::loadFunctions`
- `extioHandler::getRate`
- `extioHandler::setVFOFrequency`
- `extioHandler::getVFOFrequency`
- `extioHandler::ShowGUI`
- `extioHandler::HideGUI`
- `extioHandler::GetHWSR`
- `extioHandler::GetHWLO`
- `extioHandler::restartReader`
- `extioHandler::stopReader`
- `extioHandler::Samples`
- `extioHandler::getSamples`
- `extioHandler::bitDepth`
- `extioHandler::defaultFrequency`

### `devices/extio-handler/extio-handler.h`

- `extioHandler::extioHandler`
- `extioHandler::~extioHandler`
- `extioHandler::getRate`
- `extioHandler::setVFOFrequency`
- `extioHandler::getVFOFrequency`
- `extioHandler::defaultFrequency`
- `extioHandler::restartReader`
- `extioHandler::stopReader`
- `extioHandler::Samples`
- `extioHandler::getSamples`
- `extioHandler::bitDepth`
- `extioHandler::GetHWLO`
- `extioHandler::GetHWSR`
- `extioHandler::loadFunctions`
- `extioHandler::ShowGUI`
- `extioHandler::HideGUI`

### `devices/extio-handler/reader.h`

- `reader::reader`
- `reader::~reader`
- `reader::restartReader`
- `reader::stopReader`
- `reader::processData`
- `reader::bitDepth`
- `reader_16::reader_16`
- `reader_16::~reader_16`
- `reader_16::processData`
- `reader_16::bitDepth`
- `reader_24::reader_24`
- `reader_24::~reader_24`
- `reader_24::processData`
- `reader_24::bitDepth`
- `reader_32::reader_32`
- `reader_32::~reader_32`
- `reader_32::processData`
- `reader_32::bitDepth`
- `reader_float::reader_float`
- `reader_float::~reader_float`
- `reader_float::processData`
- `reader_float::bitDepth`

### `devices/extio-handler/virtual-reader.cpp`

- `virtualReader::virtualReader`
- `virtualReader::~virtualReader`
- `virtualReader::restartReader`
- `virtualReader::stopReader`
- `virtualReader::processData`
- `virtualReader::bitDepth`
- `virtualReader::setMapper`
- `virtualReader::convertandStore`

### `devices/extio-handler/virtual-reader.h`

- `virtualReader::virtualReader`
- `virtualReader::~virtualReader`
- `virtualReader::restartReader`
- `virtualReader::stopReader`
- `virtualReader::processData`
- `virtualReader::bitDepth`
- `virtualReader::convertandStore`
- `virtualReader::setMapper`

### `devices/filereaders/filereader/filereader-widget.h`

- `FileReaderWidget`
- `~FileReaderWidget`
- `setupUi`

### `devices/filereaders/raw-files/raw-reader.cpp`

- `RawReader::RawReader`
- `RawReader::~RawReader`
- `RawReader::start_reader`
- `RawReader::stop_reader`
- `RawReader::jump_to_relative_position_per_mill`
- `RawReader::run`
- `RawReader::handle_continuous_button`

### `devices/filereaders/raw-files/raw-reader.h`

- `RawReader`
- `~RawReader`
- `start_reader`
- `stop_reader`
- `jump_to_relative_position_per_mill`
- `handle_continuous_button`
- `run`
- `signal_set_progress`

### `devices/filereaders/raw-files/rawfiles.cpp`

- `RawFileHandler::RawFileHandler`
- `RawFileHandler::~RawFileHandler`
- `RawFileHandler::restartReader`
- `RawFileHandler::stopReader`
- `RawFileHandler::getSamples`
- `RawFileHandler::Samples`
- `RawFileHandler::show`
- `RawFileHandler::hide`
- `RawFileHandler::isHidden`
- `RawFileHandler::isFileInput`
- `RawFileHandler::setVFOFrequency`
- `RawFileHandler::getVFOFrequency`
- `RawFileHandler::resetBuffer`
- `RawFileHandler::deviceName`
- `RawFileHandler::slot_handle_cb_loop_file`
- `RawFileHandler::slot_set_progress`
- `RawFileHandler::slot_slider_pressed`
- `RawFileHandler::slot_slider_released`
- `RawFileHandler::slot_slider_moved`

### `devices/filereaders/raw-files/rawfiles.h`

- `RawFileHandler`
- `~RawFileHandler`
- `getSamples`
- `Samples`
- `restartReader`
- `stopReader`
- `show`
- `hide`
- `isHidden`
- `isFileInput`
- `setVFOFrequency`
- `getVFOFrequency`
- `resetBuffer`
- `deviceName`
- `slot_set_progress`
- `slot_handle_cb_loop_file`
- `slot_slider_pressed`
- `slot_slider_released`
- `slot_slider_moved`

### `devices/filereaders/wav-files/wav-reader.cpp`

- `getMyTime`
- `WavReader::WavReader`
- `WavReader::~WavReader`
- `WavReader::start_reader`
- `WavReader::stop_reader`
- `WavReader::jump_to_relative_position_per_mill`
- `WavReader::run`
- `WavReader::handle_continuous_button`

### `devices/filereaders/wav-files/wav-reader.h`

- `WavReader`
- `~WavReader`
- `start_reader`
- `stop_reader`
- `jump_to_relative_position_per_mill`
- `handle_continuous_button`
- `run`
- `signal_set_progress`

### `devices/filereaders/wav-files/wavfiles.cpp`

- `WavFileHandler::WavFileHandler`
- `WavFileHandler::~WavFileHandler`
- `WavFileHandler::restartReader`
- `WavFileHandler::stopReader`
- `WavFileHandler::getSamples`
- `WavFileHandler::Samples`
- `WavFileHandler::show`
- `WavFileHandler::hide`
- `WavFileHandler::isHidden`
- `WavFileHandler::isFileInput`
- `WavFileHandler::setVFOFrequency`
- `WavFileHandler::getVFOFrequency`
- `WavFileHandler::resetBuffer`
- `WavFileHandler::deviceName`
- `WavFileHandler::slot_handle_cb_loop_file`
- `WavFileHandler::slot_set_progress`
- `WavFileHandler::slot_slider_pressed`
- `WavFileHandler::slot_slider_released`
- `WavFileHandler::slot_slider_moved`

### `devices/filereaders/wav-files/wavfiles.h`

- `WavFileHandler`
- `~WavFileHandler`
- `getSamples`
- `Samples`
- `restartReader`
- `stopReader`
- `setVFOFrequency`
- `getVFOFrequency`
- `show`
- `hide`
- `isHidden`
- `isFileInput`
- `resetBuffer`
- `deviceName`
- `slot_set_progress`
- `slot_handle_cb_loop_file`
- `slot_slider_pressed`
- `slot_slider_released`
- `slot_slider_moved`

### `devices/filereaders/xml-filereader/xml-descriptor.cpp`

- `XmlDescriptor::printDescriptor`
- `XmlDescriptor::setSamplerate`
- `XmlDescriptor::setChannels`
- `XmlDescriptor::addChannelOrder`
- `XmlDescriptor::add_dataBlock`
- `XmlDescriptor::add_freqtoBlock`
- `XmlDescriptor::add_modtoBlock`
- `XmlDescriptor::XmlDescriptor`

### `devices/filereaders/xml-filereader/xml-descriptor.h`

- `Blocks`
- `~Blocks`
- `XmlDescriptor`
- `~XmlDescriptor`
- `printDescriptor`
- `setSamplerate`
- `setChannels`
- `addChannelOrder`
- `add_dataBlock`
- `add_freqtoBlock`
- `add_modtoBlock`

### `devices/filereaders/xml-filereader/xml-filereader.cpp`

- `XmlFileReader::XmlFileReader`
- `XmlFileReader::~XmlFileReader`
- `XmlFileReader::restartReader`
- `XmlFileReader::stopReader`
- `XmlFileReader::getSamples`
- `XmlFileReader::Samples`
- `XmlFileReader::slot_set_progress`
- `XmlFileReader::getVFOFrequency`
- `XmlFileReader::slot_handle_cb_loop_file`
- `XmlFileReader::show`
- `XmlFileReader::hide`
- `XmlFileReader::isHidden`
- `XmlFileReader::isFileInput`
- `XmlFileReader::setVFOFrequency`
- `XmlFileReader::resetBuffer`
- `XmlFileReader::deviceName`
- `XmlFileReader::slot_slider_pressed`
- `XmlFileReader::slot_slider_released`
- `XmlFileReader::slot_slider_moved`
- `XmlFileReader::compute_nrSamples`

### `devices/filereaders/xml-filereader/xml-filereader.h`

- `XmlFileReader`
- `~XmlFileReader`
- `getSamples`
- `Samples`
- `restartReader`
- `stopReader`
- `setVFOFrequency`
- `getVFOFrequency`
- `hide`
- `show`
- `isHidden`
- `isFileInput`
- `resetBuffer`
- `deviceName`
- `compute_nrSamples`
- `slot_set_progress`
- `slot_handle_cb_loop_file`
- `slot_slider_pressed`
- `slot_slider_released`
- `slot_slider_moved`

### `devices/filereaders/xml-filereader/xml-reader.cpp`

- `fread_chk`
- `shift`
- `currentTime`
- `XmlReader::XmlReader`
- `XmlReader::~XmlReader`
- `XmlReader::stopReader`
- `XmlReader::jump_to_relative_position`
- `XmlReader::run`
- `XmlReader::handle_continuousButton`
- `XmlReader::readSamples`
- `XmlReader::readElements_IQ`
- `XmlReader::readElements_QI`
- `XmlReader::readElements_I`
- `XmlReader::readElements_Q`

### `devices/filereaders/xml-filereader/xml-reader.h`

- `XmlReader`
- `~XmlReader`
- `stopReader`
- `jump_to_relative_position`
- `handle_continuousButton`
- `run`
- `readSamples`
- `readElements_IQ`
- `readElements_QI`
- `readElements_I`
- `readElements_Q`
- `signal_set_progress`

### `devices/hackrf-handler/hackrf-handler.cpp`

- `HackRfHandler::HackRfHandler`
- `HackRfHandler::~HackRfHandler`
- `HackRfHandler::setVFOFrequency`
- `HackRfHandler::getVFOFrequency`
- `HackRfHandler::slot_set_lna_gain`
- `HackRfHandler::slot_set_vga_gain`
- `HackRfHandler::slot_enable_bias_t`
- `HackRfHandler::slot_enable_amp`
- `HackRfHandler::slot_set_ppm_correction`
- `callback`
- `HackRfHandler::restartReader`
- `HackRfHandler::stopReader`
- `HackRfHandler::getSamples`
- `HackRfHandler::Samples`
- `HackRfHandler::resetBuffer`
- `HackRfHandler::deviceName`
- `HackRfHandler::load_hackrf_functions`
- `HackRfHandler::startDumping`
- `HackRfHandler::setup_xml_dump`
- `HackRfHandler::stopDumping`
- `HackRfHandler::show`
- `HackRfHandler::hide`
- `HackRfHandler::isHidden`
- `HackRfHandler::record_gain_settings`
- `HackRfHandler::update_gain_settings`
- `HackRfHandler::check_err_throw`
- `HackRfHandler::check_err`
- `HackRfHandler::load_method`
- `HackRfHandler::hasDump`

### `devices/hackrf-handler/hackrf-handler.h`

- `HackRfHandler`
- `~HackRfHandler`
- `setVFOFrequency`
- `getVFOFrequency`
- `restartReader`
- `stopReader`
- `getSamples`
- `Samples`
- `resetBuffer`
- `show`
- `hide`
- `isHidden`
- `deviceName`
- `hasDump`
- `startDumping`
- `stopDumping`
- `load_hackrf_functions`
- `setup_xml_dump`
- `record_gain_settings`
- `update_gain_settings`
- `check_err_throw`
- `check_err`
- `load_method`
- `signal_new_ant_enable`
- `signal_new_amp_enable`
- `signal_new_vga_value`
- `signal_new_lna_value`
- `slot_set_lna_gain`
- `slot_set_vga_gain`
- `slot_enable_bias_t`
- `slot_enable_amp`
- `slot_set_ppm_correction`

### `devices/lime-handler/LMS7002M_parameters.h`

- `LMS7ParameterCompare`

### `devices/lime-handler/LimeSuite.h`

- `LMS_GetDeviceList`
- `LMS_Open`
- `LMS_Close`
- `LMS_Init`
- `LMS_GetNumChannels`
- `LMS_EnableChannel`
- `LMS_SetSampleRate`
- `LMS_GetSampleRate`
- `LMS_GetSampleRateRange`
- `LMS_SetLOFrequency`
- `LMS_GetLOFrequency`
- `LMS_GetLOFrequencyRange`
- `LMS_GetAntennaList`
- `LMS_SetAntenna`
- `LMS_GetAntenna`
- `LMS_GetAntennaBW`
- `LMS_SetNormalizedGain`
- `LMS_SetGaindB`
- `LMS_GetNormalizedGain`
- `LMS_GetGaindB`
- `LMS_SetLPFBW`
- `LMS_GetLPFBW`
- `LMS_GetLPFBWRange`
- `LMS_SetLPF`
- `LMS_SetGFIRLPF`
- `LMS_Calibrate`
- `LMS_LoadConfig`
- `LMS_SaveConfig`
- `LMS_SetTestSignal`
- `LMS_GetTestSignal`
- `LMS_GetChipTemperature`
- `LMS_SetSampleRateDir`
- `LMS_SetNCOFrequency`
- `LMS_GetNCOFrequency`
- `LMS_SetNCOPhase`
- `LMS_GetNCOPhase`
- `LMS_SetNCOIndex`
- `LMS_GetNCOIndex`
- `LMS_SetGFIRCoeff`
- `LMS_GetGFIRCoeff`
- `LMS_SetGFIR`
- `LMS_EnableCalibCache`
- `LMS_EnableCache`
- `LMS_Reset`
- `LMS_ReadLMSReg`
- `LMS_WriteLMSReg`
- `LMS_ReadParam`
- `LMS_WriteParam`
- `LMS_ReadFPGAReg`
- `LMS_WriteFPGAReg`
- `LMS_ReadCustomBoardParam`
- `LMS_WriteCustomBoardParam`
- `LMS_GetClockFreq`
- `LMS_SetClockFreq`
- `LMS_VCTCXOWrite`
- `LMS_VCTCXORead`
- `LMS_Synchronize`
- `LMS_GPIORead`
- `LMS_GPIOWrite`
- `LMS_GPIODirRead`
- `LMS_GPIODirWrite`
- `LMS_SetupStream`
- `LMS_DestroyStream`
- `LMS_StartStream`
- `LMS_StopStream`
- `LMS_RecvStream`
- `LMS_GetStreamStatus`
- `LMS_SendStream`
- `LMS_UploadWFM`
- `LMS_EnableTxWFM`
- `LMS_GetProgramModes`
- `LMS_Program`
- `LMS_GetDeviceInfo`
- `LMS_GetLibraryVersion`
- `LMS_GetLastErrorMessage`
- `LMS_RegisterLogHandler`

### `devices/lime-handler/lime-handler.cpp`

- `LimeHandler::LimeHandler`
- `LimeHandler::~LimeHandler`
- `LimeHandler::setVFOFrequency`
- `LimeHandler::getVFOFrequency`
- `LimeHandler::setGain`
- `LimeHandler::setAntenna`
- `LimeHandler::set_filter`
- `LimeHandler::restartReader`
- `LimeHandler::stopReader`
- `LimeHandler::getSamples`
- `LimeHandler::Samples`
- `LimeHandler::resetBuffer`
- `LimeHandler::deviceName`
- `LimeHandler::showErrors`
- `LimeHandler::run`
- `LimeHandler::load_limeFunctions`
- `LimeHandler::startDumping`
- `LimeHandler::setup_xmlDump`
- `LimeHandler::stopDumping`
- `LimeHandler::show`
- `LimeHandler::hide`
- `LimeHandler::isHidden`
- `LimeHandler::record_gainSettings`
- `LimeHandler::update_gainSettings`
- `LimeHandler::hasDump`

### `devices/lime-handler/lime-handler.h`

- `LimeHandler`
- `~LimeHandler`
- `setVFOFrequency`
- `getVFOFrequency`
- `restartReader`
- `stopReader`
- `getSamples`
- `Samples`
- `resetBuffer`
- `show`
- `hide`
- `isHidden`
- `deviceName`
- `hasDump`
- `startDumping`
- `stopDumping`
- `load_limeFunctions`
- `run`
- `setup_xmlDump`
- `record_gainSettings`
- `update_gainSettings`
- `new_gainValue`
- `setGain`
- `setAntenna`
- `set_filter`
- `showErrors`

### `devices/lime-handler/lime-widget.h`

- `limeWidget::limeWidget`

### `devices/pluto-handler-2/dabFilter.h`

- No function declarations detected

### `devices/pluto-handler-2/iio.h`

- `iio_create_scan_context`
- `iio_scan_context_destroy`
- `iio_scan_context_get_info_list`
- `iio_context_info_list_free`
- `iio_context_info_get_description`
- `iio_context_info_get_uri`
- `iio_library_get_version`
- `iio_strerror`
- `iio_has_backend`
- `iio_get_backends_count`
- `iio_get_backend`
- `iio_create_default_context`
- `iio_create_local_context`
- `iio_create_xml_context`
- `iio_create_xml_context_mem`
- `iio_create_network_context`
- `iio_create_context_from_uri`
- `iio_context_clone`
- `iio_context_destroy`
- `iio_context_get_version`
- `iio_context_get_xml`
- `iio_context_get_name`
- `iio_context_get_description`
- `iio_context_get_attrs_count`
- `iio_context_get_attr`
- `iio_context_get_attr_value`
- `iio_context_get_devices_count`
- `iio_context_get_device`
- `iio_context_find_device`
- `iio_context_set_timeout`
- `iio_device_get_context`
- `iio_device_get_id`
- `iio_device_get_name`
- `iio_device_get_channels_count`
- `iio_device_get_attrs_count`
- `iio_device_get_buffer_attrs_count`
- `iio_device_get_channel`
- `iio_device_get_attr`
- `iio_device_get_buffer_attr`
- `iio_device_find_channel`
- `iio_device_find_attr`
- `iio_device_find_buffer_attr`
- `iio_device_attr_read`
- `iio_device_attr_read_all`
- `iio_device_attr_read_bool`
- `iio_device_attr_read_longlong`
- `iio_device_attr_read_double`
- `iio_device_attr_write`
- `iio_device_attr_write_raw`
- `iio_device_attr_write_all`
- `iio_device_attr_write_bool`
- `iio_device_attr_write_longlong`
- `iio_device_attr_write_double`
- `iio_device_buffer_attr_read`
- `iio_device_buffer_attr_read_all`
- `iio_device_buffer_attr_read_bool`
- `iio_device_buffer_attr_read_longlong`
- `iio_device_buffer_attr_read_double`
- `iio_device_buffer_attr_write`
- `iio_device_buffer_attr_write_raw`
- `iio_device_buffer_attr_write_all`
- `iio_device_buffer_attr_write_bool`
- `iio_device_buffer_attr_write_longlong`
- `iio_device_buffer_attr_write_double`
- `iio_device_set_data`
- `iio_device_get_data`
- `iio_device_get_trigger`
- `iio_device_set_trigger`
- `iio_device_is_trigger`
- `iio_device_set_kernel_buffers_count`
- `iio_channel_get_device`
- `iio_channel_get_id`
- `iio_channel_get_name`
- `iio_channel_is_output`
- `iio_channel_is_scan_element`
- `iio_channel_get_attrs_count`
- `iio_channel_get_attr`
- `iio_channel_find_attr`
- `iio_channel_attr_get_filename`
- `iio_channel_attr_read`
- `iio_channel_attr_read_all`
- `iio_channel_attr_read_bool`
- `iio_channel_attr_read_longlong`
- `iio_channel_attr_read_double`
- `iio_channel_attr_write`
- `iio_channel_attr_write_raw`
- `iio_channel_attr_write_all`
- `iio_channel_attr_write_bool`
- `iio_channel_attr_write_longlong`
- `iio_channel_attr_write_double`
- `iio_channel_enable`
- `iio_channel_disable`
- `iio_channel_is_enabled`
- `iio_channel_read_raw`
- `iio_channel_read`
- `iio_channel_write_raw`
- `iio_channel_write`
- `iio_channel_set_data`
- `iio_channel_get_data`
- `iio_channel_get_type`
- `iio_channel_get_modifier`
- `iio_buffer_get_device`
- `iio_device_create_buffer`
- `iio_buffer_destroy`
- `iio_buffer_get_poll_fd`
- `iio_buffer_set_blocking_mode`
- `iio_buffer_refill`
- `iio_buffer_push`
- `iio_buffer_push_partial`
- `iio_buffer_cancel`
- `iio_buffer_start`
- `iio_buffer_first`
- `iio_buffer_step`
- `iio_buffer_end`
- `iio_buffer_foreach_sample`
- `iio_buffer_set_data`
- `iio_buffer_get_data`
- `iio_device_get_sample_size`
- `iio_channel_get_index`
- `iio_channel_get_data_format`
- `iio_channel_convert`
- `iio_channel_convert_inverse`
- `iio_device_get_debug_attrs_count`
- `iio_device_get_debug_attr`
- `iio_device_find_debug_attr`
- `iio_device_debug_attr_read`
- `iio_device_debug_attr_read_all`
- `iio_device_debug_attr_write`
- `iio_device_debug_attr_write_raw`
- `iio_device_debug_attr_write_all`
- `iio_device_debug_attr_read_bool`
- `iio_device_debug_attr_read_longlong`
- `iio_device_debug_attr_read_double`
- `iio_device_debug_attr_write_bool`
- `iio_device_debug_attr_write_longlong`
- `iio_device_debug_attr_write_double`
- `iio_device_identify_filename`
- `iio_device_reg_write`
- `iio_device_reg_read`

### `devices/pluto-handler-2/pluto-handler.cpp`

- `get_ch_name`
- `PlutoHandler::PlutoHandler`
- `PlutoHandler::~PlutoHandler`
- `PlutoHandler::ad9361_set_trx_fir_enable`
- `PlutoHandler::ad9361_get_trx_fir_enable`
- `PlutoHandler::get_ad9361_stream_dev`
- `PlutoHandler::get_ad9361_stream_ch`
- `PlutoHandler::get_phy_chan`
- `PlutoHandler::get_lo_chan`
- `PlutoHandler::cfg_ad9361_streaming_ch`
- `PlutoHandler::setVFOFrequency`
- `PlutoHandler::getVFOFrequency`
- `PlutoHandler::set_gainControl`
- `PlutoHandler::set_agcControl`
- `PlutoHandler::restartReader`
- `PlutoHandler::stopReader`
- `PlutoHandler::run`
- `PlutoHandler::getSamples`
- `PlutoHandler::Samples`
- `PlutoHandler::set_filter`
- `PlutoHandler::resetBuffer`
- `PlutoHandler::deviceName`
- `PlutoHandler::show`
- `PlutoHandler::hide`
- `PlutoHandler::isHidden`
- `PlutoHandler::toggle_debugButton`
- `PlutoHandler::startDumping`
- `PlutoHandler::setup_xmlDump`
- `PlutoHandler::stopDumping`
- `PlutoHandler::record_gainSettings`
- `PlutoHandler::update_gainSettings`
- `PlutoHandler::loadFunctions`
- `PlutoHandler::hasDump`

### `devices/pluto-handler-2/pluto-handler.h`

- `PlutoHandler`
- `~PlutoHandler`
- `setVFOFrequency`
- `getVFOFrequency`
- `restartReader`
- `stopReader`
- `getSamples`
- `Samples`
- `resetBuffer`
- `show`
- `hide`
- `isHidden`
- `deviceName`
- `hasDump`
- `startDumping`
- `stopDumping`
- `loadFunctions`
- `setup_xmlDump`
- `run`
- `record_gainSettings`
- `update_gainSettings`
- `ad9361_set_trx_fir_enable`
- `ad9361_get_trx_fir_enable`
- `get_ad9361_stream_dev`
- `get_ad9361_stream_ch`
- `get_phy_chan`
- `get_lo_chan`
- `cfg_ad9361_streaming_ch`
- `new_gainValue`
- `new_agcValue`
- `set_gainControl`
- `set_agcControl`
- `toggle_debugButton`
- `set_filter`

### `devices/rtl_tcp/rtl_tcp_client.cpp`

- `RtlTcpClient::RtlTcpClient`
- `RtlTcpClient::~RtlTcpClient`
- `RtlTcpClient::wantConnect`
- `RtlTcpClient::defaultFrequency`
- `RtlTcpClient::setVFOFrequency`
- `RtlTcpClient::getVFOFrequency`
- `RtlTcpClient::restartReader`
- `RtlTcpClient::stopReader`
- `RtlTcpClient::getSamples`
- `RtlTcpClient::Samples`
- `RtlTcpClient::readData`
- `RtlTcpClient::sendVFO`
- `RtlTcpClient::sendRate`
- `RtlTcpClient::sendGain`
- `RtlTcpClient::set_fCorrection`
- `RtlTcpClient::setAgcMode`
- `RtlTcpClient::setBiasT`
- `RtlTcpClient::setBandwidth`
- `RtlTcpClient::setPort`
- `RtlTcpClient::setAddress`
- `RtlTcpClient::setDisconnect`
- `RtlTcpClient::show`
- `RtlTcpClient::hide`
- `RtlTcpClient::isHidden`
- `RtlTcpClient::resetBuffer`
- `RtlTcpClient::deviceName`
- `RtlTcpClient::handle_hw_agc`
- `RtlTcpClient::handle_sw_agc`
- `RtlTcpClient::handle_manual`
- `RtlTcpClient::startDumping`
- `RtlTcpClient::setup_xmlDump`
- `RtlTcpClient::stopDumping`
- `RtlTcpClient::hasDump`

### `devices/rtl_tcp/rtl_tcp_client.h`

- `RtlTcpClient`
- `~RtlTcpClient`
- `setVFOFrequency`
- `getVFOFrequency`
- `restartReader`
- `stopReader`
- `getSamples`
- `Samples`
- `show`
- `hide`
- `isHidden`
- `deviceName`
- `resetBuffer`
- `getRate`
- `defaultFrequency`
- `hasDump`
- `startDumping`
- `stopDumping`
- `sendVFO`
- `sendRate`
- `sendCommand`
- `isvalidRate`
- `setAgcMode`
- `setup_xmlDump`
- `sendGain`
- `set_fCorrection`
- `readData`
- `wantConnect`
- `setDisconnect`
- `setBiasT`
- `setBandwidth`
- `setPort`
- `setAddress`
- `handle_hw_agc`
- `handle_sw_agc`
- `handle_manual`

### `devices/rtlsdr-handler/rtlsdr-handler.cpp`

- `RTLSDRCallBack`
- `RtlSdrHandler::RtlSdrHandler`

### `devices/rtlsdr-handler/rtlsdr-handler.h`

- `RtlSdrHandler`
- `~RtlSdrHandler`
- `setVFOFrequency`
- `getVFOFrequency`
- `restartReader`
- `stopReader`
- `getSamples`
- `Samples`
- `resetBuffer`
- `deviceName`
- `show`
- `hide`
- `isHidden`
- `hasDump`
- `startDumping`
- `stopDumping`
- `detect_overload`
- `setup_xmlDump`
- `set_autogain`
- `enable_gainControl`
- `maxGain`
- `load_rtlFunctions`
- `set_ExternalGain`
- `set_ppmCorrection`
- `set_bandwidth`
- `set_filter`
- `set_biasControl`
- `handle_hw_agc`
- `handle_sw_agc`
- `handle_manual`
- `slot_timer`
- `signal_timer`

### `devices/sdrplay-handler/Rsp-device.cpp`

- `Rsp_device::Rsp_device`
- `Rsp_device::~Rsp_device`
- `Rsp_device::restart`
- `Rsp_device::set_agc`
- `Rsp_device::set_lna`
- `Rsp_device::set_GRdB`
- `Rsp_device::set_ppm`
- `Rsp_device::set_antenna`
- `Rsp_device::set_amPort`
- `Rsp_device::set_biasT`
- `Rsp_device::set_notch`

### `devices/sdrplay-handler/Rsp-device.h`

- `Rsp_device`
- `~Rsp_device`
- `lnaStates`
- `restart`
- `set_agc`
- `set_lna`
- `set_GRdB`
- `set_ppm`
- `set_antenna`
- `set_amPort`
- `set_biasT`
- `set_notch`
- `signal_set_lnabounds`
- `signal_set_antennaSelect`
- `signal_show_lnaGain`

### `devices/sdrplay-handler/Rsp1-handler.cpp`

- `Rsp1_handler::Rsp1_handler`
- `Rsp1_handler::lnaStates`
- `Rsp1_handler::restart`
- `Rsp1_handler::set_lna`

### `devices/sdrplay-handler/Rsp1-handler.h`

- `Rsp1_handler`
- `~Rsp1_handler`
- `lnaStates`
- `restart`
- `set_lna`

### `devices/sdrplay-handler/Rsp1A-handler.cpp`

- `Rsp1A_handler::Rsp1A_handler`
- `Rsp1A_handler::lnaStates`
- `Rsp1A_handler::restart`
- `Rsp1A_handler::set_lna`
- `Rsp1A_handler::set_biasT`
- `Rsp1A_handler::set_notch`

### `devices/sdrplay-handler/Rsp1A-handler.h`

- `Rsp1A_handler`
- `~Rsp1A_handler`
- `lnaStates`
- `restart`
- `set_lna`
- `set_biasT`
- `set_notch`

### `devices/sdrplay-handler/Rsp2-handler.cpp`

- `Rsp2_handler::Rsp2_handler`
- `Rsp2_handler::lnaStates`
- `Rsp2_handler::restart`
- `Rsp2_handler::set_lna`
- `Rsp2_handler::set_antenna`
- `Rsp2_handler::set_biasT`
- `Rsp2_handler::set_notch`

### `devices/sdrplay-handler/Rsp2-handler.h`

- `Rsp2_handler`
- `~Rsp2_handler`
- `lnaStates`
- `restart`
- `set_lna`
- `set_antenna`
- `set_biasT`
- `set_notch`

### `devices/sdrplay-handler/RspDuo-handler.cpp`

- `RspDuo_handler::RspDuo_handler`
- `RspDuo_handler::lnaStates`
- `RspDuo_handler::restart`
- `RspDuo_handler::set_lna`
- `RspDuo_handler::set_amPort`
- `RspDuo_handler::set_antenna`
- `RspDuo_handler::set_biasT`
- `RspDuo_handler::set_notch`

### `devices/sdrplay-handler/RspDuo-handler.h`

- `RspDuo_handler`
- `~RspDuo_handler`
- `lnaStates`
- `restart`
- `set_lna`
- `set_antenna`
- `set_biasT`
- `set_notch`
- `set_amPort`

### `devices/sdrplay-handler/RspDx-handler.cpp`

- `RspDx_handler::RspDx_handler`
- `RspDx_handler::lnaStates`
- `RspDx_handler::restart`
- `RspDx_handler::set_lna`
- `RspDx_handler::set_antenna`
- `RspDx_handler::set_amPort`
- `RspDx_handler::set_biasT`
- `RspDx_handler::set_notch`

### `devices/sdrplay-handler/RspDx-handler.h`

- `RspDx_handler`
- `~RspDx_handler`
- `lnaStates`
- `restart`
- `set_lna`
- `set_antenna`
- `set_amPort`
- `set_biasT`
- `set_notch`

### `devices/sdrplay-handler/sdrplay-commands.h`

- `generalCommand::generalCommand`
- `restartRequest::restartRequest`
- `restartRequest::~restartRequest`
- `stopRequest::stopRequest`
- `stopRequest::~stopRequest`
- `set_frequencyRequest::set_frequencyRequest`
- `set_frequencyRequest::~set_frequencyRequest`
- `agcRequest::agcRequest`
- `agcRequest::~agcRequest`
- `GRdBRequest::GRdBRequest`
- `GRdBRequest::~GRdBRequest`
- `ppmRequest::ppmRequest`
- `ppmRequest::~ppmRequest`
- `lnaRequest::lnaRequest`
- `lnaRequest::~lnaRequest`
- `antennaRequest::antennaRequest`
- `antennaRequest::~antennaRequest`
- `biasT_Request::biasT_Request`
- `biasT_Request::~biasT_Request`
- `notch_Request::notch_Request`
- `notch_Request::~notch_Request`

### `devices/sdrplay-handler/sdrplay-handler.cpp`

- `errorMessage`
- `SdrPlayHandler::SdrPlayHandler`
- `SdrPlayHandler::~SdrPlayHandler`
- `SdrPlayHandler::getVFOFrequency`
- `SdrPlayHandler::restartReader`
- `SdrPlayHandler::stopReader`
- `SdrPlayHandler::getSamples`
- `SdrPlayHandler::Samples`
- `SdrPlayHandler::resetBuffer`
- `SdrPlayHandler::deviceName`
- `SdrPlayHandler::set_lnabounds`
- `SdrPlayHandler::set_deviceName`
- `SdrPlayHandler::set_serial`
- `SdrPlayHandler::set_apiVersion`
- `SdrPlayHandler::show_lnaGain`
- `SdrPlayHandler::set_ifgainReduction`
- `SdrPlayHandler::set_lnagainReduction`
- `SdrPlayHandler::set_agcControl`
- `SdrPlayHandler::set_ppmControl`
- `SdrPlayHandler::set_biasT`
- `SdrPlayHandler::set_notch`
- `SdrPlayHandler::set_selectAntenna`
- `SdrPlayHandler::set_antennaSelect`
- `SdrPlayHandler::startDumping`
- `SdrPlayHandler::setup_xmlDump`
- `SdrPlayHandler::stopDumping`
- `SdrPlayHandler::messageHandler`
- `StreamACallback`
- `StreamBCallback`
- `EventCallback`
- `SdrPlayHandler::update_PowerOverload`
- `SdrPlayHandler::run`
- `SdrPlayHandler::loadFunctions`
- `SdrPlayHandler::show`
- `SdrPlayHandler::hide`
- `SdrPlayHandler::isHidden`
- `SdrPlayHandler::setVFOFrequency`
- `SdrPlayHandler::slot_overload_detected`
- `SdrPlayHandler::slot_tuner_gain`
- `SdrPlayHandler::hasDump`
- `SdrPlayHandler::record_lnaSettings`
- `SdrPlayHandler::update_lnaSettings`

### `devices/sdrplay-handler/sdrplay-handler.h`

- `SdrPlayHandler`
- `~SdrPlayHandler`
- `setVFOFrequency`
- `getVFOFrequency`
- `restartReader`
- `stopReader`
- `getSamples`
- `Samples`
- `resetBuffer`
- `show`
- `hide`
- `isHidden`
- `deviceName`
- `hasDump`
- `startDumping`
- `stopDumping`
- `update_PowerOverload`
- `run`
- `messageHandler`
- `loadFunctions`
- `set_deviceName`
- `set_serial`
- `set_apiVersion`
- `setup_xmlDump`
- `record_lnaSettings`
- `update_lnaSettings`
- `set_ifgainReduction`
- `set_lnagainReduction`
- `set_agcControl`
- `set_ppmControl`
- `set_selectAntenna`
- `set_biasT`
- `set_notch`
- `slot_overload_detected`
- `slot_tuner_gain`
- `set_lnabounds`
- `set_antennaSelect`
- `show_lnaGain`
- `set_antennaSelect_signal`
- `signal_overload_detected`
- `signal_tuner_gain`
- `new_lnaValue`

### `devices/soapy/soapy-converter.cpp`

- `SoapyConverter::SoapyConverter`
- `SoapyConverter::~SoapyConverter`
- `SoapyConverter::Samples`
- `SoapyConverter::getSamples`
- `SoapyConverter::run`

### `devices/soapy/soapy-converter.h`

- `SoapyConverter`
- `~SoapyConverter`
- `Samples`
- `getSamples`
- `run`

### `devices/soapy/soapy-handler.cpp`

- `SoapyHandler::SoapyHandler`
- `SoapyHandler::~SoapyHandler`
- `toString`
- `SoapyHandler::createDevice`

### `devices/soapy/soapy-handler.h`

- `SoapyHandler`
- `~SoapyHandler`
- `restartReader`
- `stopReader`
- `setVFOFrequency`
- `getVFOFrequency`
- `show`
- `hide`
- `isHidden`
- `deviceName`
- `resetBuffer`
- `getSamples`
- `Samples`
- `createDevice`
- `findDesiredSamplerate`
- `findDesiredBandwidth`
- `handle_spinBox_0`
- `handle_spinBox_1`
- `handle_spinBox_2`
- `set_agcControl`
- `handleAntenna`
- `set_ppmCorrection`

### `devices/soapy/soapy-worker.cpp`

- `soapyWorker::soapyWorker`

### `devices/soapy/soapy-worker.h`

- `soapyWorker`
- `~soapyWorker`
- `Samples`
- `getSamples`

### `devices/spy-server/spyserver-client.cpp`

- `SpyServerClient::~SpyServerClient`
- `SpyServerClient::_slot_handle_connect_button`
- `SpyServerClient::_setup_connection`
- `SpyServerClient::getRate`
- `SpyServerClient::restartReader`
- `SpyServerClient::stopReader`
- `SpyServerClient::getSamples`
- `SpyServerClient::Samples`
- `SpyServerClient::_slot_handle_gain`
- `SpyServerClient::_slot_handle_autogain`
- `SpyServerClient::slot_data_ready`
- `SpyServerClient::_slot_handle_checkTimer`
- `SpyServerClient::_check_and_cleanup_ip_address`

### `devices/spy-server/spyserver-client.h`

- `SpyServerClient`
- `~SpyServerClient`
- `restartReader`
- `stopReader`
- `getSamples`
- `Samples`
- `setVFOFrequency`
- `getVFOFrequency`
- `resetBuffer`
- `show`
- `hide`
- `isHidden`
- `deviceName`
- `getRate`
- `_slot_handle_connect_button`
- `_slot_handle_gain`
- `_slot_handle_autogain`
- `_slot_handle_checkTimer`
- `slot_data_ready`
- `_setup_connection`
- `_check_and_cleanup_ip_address`

### `devices/spy-server/spyserver-handler.cpp`

- `SpyServerHandler::~SpyServerHandler`
- `SpyServerHandler::_slot_no_device_info`
- `SpyServerHandler::run`
- `SpyServerHandler::readHeader`
- `SpyServerHandler::readBody`
- `SpyServerHandler::show_attendance`
- `SpyServerHandler::cleanRecords`
- `SpyServerHandler::send_command`
- `SpyServerHandler::process_device_info`
- `SpyServerHandler::process_client_sync`
- `SpyServerHandler::get_deviceInfo`
- `SpyServerHandler::set_sample_rate_by_decim_stage`
- `SpyServerHandler::get_sample_rate`
- `SpyServerHandler::set_iq_center_freq`
- `SpyServerHandler::set_gain_mode`
- `SpyServerHandler::set_gain`
- `SpyServerHandler::is_streaming`
- `SpyServerHandler::start_running`
- `SpyServerHandler::stop_running`
- `SpyServerHandler::set_setting`
- `SpyServerHandler::process_data`
- `SpyServerHandler::connection_set`
- `SpyServerHandler::deviceName`

### `devices/spy-server/spyserver-handler.h`

- `SpyServerHandler`
- `~SpyServerHandler`
- `get_deviceInfo`
- `set_sample_rate_by_decim_stage`
- `get_sample_rate`
- `set_iq_center_freq`
- `set_gain_mode`
- `set_gain`
- `is_streaming`
- `start_running`
- `stop_running`
- `connection_set`
- `deviceName`
- `run`
- `process_device_info`
- `process_client_sync`
- `cleanRecords`
- `show_attendance`
- `readHeader`
- `readBody`
- `process_data`
- `send_command`
- `set_setting`
- `std::string`
- `signal_call_parent`
- `signal_data_ready`
- `_slot_no_device_info`

### `devices/spy-server/spyserver-protocol.h`

- No function declarations detected

### `devices/spy-server/spyserver-tcp-client.cpp`

- `SpyServerTcpClient::~SpyServerTcpClient`
- `SpyServerTcpClient::is_connected`
- `SpyServerTcpClient::connect_conn`
- `SpyServerTcpClient::send_data`
- `SpyServerTcpClient::run`

### `devices/spy-server/spyserver-tcp-client.h`

- `run`
- `SpyServerTcpClient`
- `~SpyServerTcpClient`
- `is_connected`
- `connect_conn`
- `close_conn`
- `send_data`

### `devices/uhd/uhd-handler.cpp`

- `uhd_streamer::uhd_streamer`
- `uhd_streamer::stop`
- `uhd_streamer::run`
- `UhdHandler::UhdHandler`
- `UhdHandler::~UhdHandler`
- `UhdHandler::setVFOFrequency`
- `UhdHandler::getVFOFrequency`
- `UhdHandler::restartReader`
- `UhdHandler::stopReader`
- `UhdHandler::getSamples`
- `UhdHandler::Samples`
- `UhdHandler::resetBuffer`
- `UhdHandler::show`
- `UhdHandler::hide`
- `UhdHandler::isHidden`
- `UhdHandler::deviceName`
- `UhdHandler::_maxGain`
- `UhdHandler::_slot_set_external_gain`
- `UhdHandler::_slot_set_f_correction`
- `UhdHandler::_slot_set_khz_offset`
- `UhdHandler::_slot_handle_ant_selector`
- `UhdHandler::_load_save_combobox_settings`

### `devices/uhd/uhd-handler.h`

- `uhd_streamer`
- `stop`
- `run`
- `UhdHandler`
- `~UhdHandler`
- `setVFOFrequency`
- `getVFOFrequency`
- `restartReader`
- `stopReader`
- `getSamples`
- `Samples`
- `resetBuffer`
- `show`
- `hide`
- `isHidden`
- `deviceName`
- `_maxGain`
- `_load_save_combobox_settings`
- `_slot_set_external_gain`
- `_slot_set_f_correction`
- `_slot_set_khz_offset`
- `_slot_handle_ant_selector`

### `eti-handler/eti-generator.cpp`

- `EtiGenerator::EtiGenerator`
- `EtiGenerator::~EtiGenerator`
- `EtiGenerator::reset`
- `EtiGenerator::process_block`
- `EtiGenerator::_init_eti`
- `EtiGenerator::_process_cif`
- `EtiGenerator::_process_sub_channel`
- `EtiGenerator::start_eti_generator`
- `EtiGenerator::stop_eti_generator`

### `eti-handler/eti-generator.h`

- `EtiGenerator`
- `~EtiGenerator`
- `process_block`
- `reset`
- `start_eti_generator`
- `stop_eti_generator`
- `_init_eti`
- `_process_cif`
- `_process_sub_channel`

### `file-devices/xml-filewriter/xml-filewriter.cpp`

- `XmlFileWriter::XmlFileWriter`
- `XmlFileWriter::~XmlFileWriter`
- `XmlFileWriter::computeHeader`
- `XmlFileWriter::add`
- `XmlFileWriter::create_xmltree`

### `file-devices/xml-filewriter/xml-filewriter.h`

- `Blocks::Blocks`
- `XmlFileWriter::XmlFileWriter`
- `XmlFileWriter::~XmlFileWriter`
- `XmlFileWriter::add`
- `XmlFileWriter::computeHeader`
- `XmlFileWriter::create_xmltree`

### `main/bit-extractors.h`

- `getBits_1`
- `getBits_2`
- `getBits_3`
- `getBits_4`
- `getBits_5`
- `getBits_6`
- `getBits_7`
- `getBits_8`
- `getBits`
- `getLBits`
- `check_get_bits`

### `main/dab-constants.h`

- `SPacketData`
- `SAudioData`

### `main/dab-processor.cpp`

- `DabProcessor::DabProcessor`
- `DabProcessor::~DabProcessor`
- `DabProcessor::start`
- `DabProcessor::stop`
- `DabProcessor::run`
- `DabProcessor::_state_process_rest_of_frame`
- `DabProcessor::_process_null_symbol`
- `DabProcessor::_process_ofdm_symbols_1_to_L`
- `DabProcessor::_set_bb_freq_offs_Hz`
- `DabProcessor::_set_rf_freq_offs_Hz`
- `DabProcessor::_state_eval_sync_symbol`
- `DabProcessor::_state_wait_for_time_sync_marker`
- `DabProcessor::set_scan_mode`
- `DabProcessor::activate_cir_viewer`
- `DabProcessor::reset_services`
- `DabProcessor::is_service_running`
- `DabProcessor::stop_service`
- `DabProcessor::stop_all_services`
- `DabProcessor::set_audio_channel`
- `DabProcessor::set_data_channel`
- `DabProcessor::startDumping`
- `DabProcessor::stop_dumping`
- `DabProcessor::start_fic_dump`
- `DabProcessor::stop_fic_dump`
- `DabProcessor::start_eti_generator`
- `DabProcessor::stop_eti_generator`
- `DabProcessor::reset_eti_generator`
- `DabProcessor::slot_select_carrier_plot_type`
- `DabProcessor::slot_select_iq_plot_type`
- `DabProcessor::slot_soft_bit_gen_type`
- `DabProcessor::slot_show_nominal_carrier`
- `DabProcessor::set_dc_avoidance_algorithm`
- `DabProcessor::set_dc_and_iq_correction`
- `DabProcessor::set_sync_on_strongest_peak`
- `DabProcessor::set_tii_processing`
- `DabProcessor::set_tii_collisions`
- `DabProcessor::set_tii_sub_id`
- `DabProcessor::set_tii_threshold`

### `main/dab-processor.h`

- `DabProcessor`
- `~DabProcessor`
- `get_fib_decoder`
- `start`
- `stop`
- `startDumping`
- `stop_dumping`
- `start_eti_generator`
- `stop_eti_generator`
- `reset_eti_generator`
- `set_scan_mode`
- `add_bb_freq`
- `activate_cir_viewer`
- `start_fic_dump`
- `stop_fic_dump`
- `reset_services`
- `is_service_running`
- `stop_service`
- `stop_all_services`
- `set_audio_channel`
- `set_data_channel`
- `set_sync_on_strongest_peak`
- `set_dc_avoidance_algorithm`
- `set_dc_and_iq_correction`
- `set_tii_processing`
- `set_tii_threshold`
- `set_tii_sub_id`
- `set_tii_collisions`
- `alignas`
- `run`
- `_state_wait_for_time_sync_marker`
- `_state_eval_sync_symbol`
- `_state_process_rest_of_frame`
- `_process_ofdm_symbols_1_to_L`
- `_process_null_symbol`
- `_set_rf_freq_offs_Hz`
- `_set_bb_freq_offs_Hz`
- `slot_select_carrier_plot_type`
- `slot_select_iq_plot_type`
- `slot_show_nominal_carrier`
- `slot_soft_bit_gen_type`
- `signal_no_signal_found`
- `signal_show_tii`
- `signal_show_spectrum`
- `signal_show_clock_err`
- `signal_set_and_show_freq_corr_rf_Hz`
- `signal_show_freq_corr_bb_Hz`
- `signal_linear_peak_level`

### `main/dabradio.cpp`

- `DabRadio::~DabRadio`
- `DabRadio::_slot_do_start`
- `DabRadio::_slot_new_device`
- `DabRadio::_get_scan_message`
- `DabRadio::do_start`
- `DabRadio::_slot_scanning_no_signal_timeout`
- `DabRadio::_slot_scanning_security_timeout`
- `DabRadio::check_and_create_dir`
- `DabRadio::slot_handle_mot_object`
- `DabRadio::save_MOT_EPG_data`
- `DabRadio::save_MOT_text`
- `DabRadio::save_MOT_object`
- `DabRadio::generate_unique_file_path_from_hash`
- `DabRadio::generate_file_path`
- `DabRadio::show_MOT_image`
- `DabRadio::create_directory`
- `DabRadio::write_picture`
- `DabRadio::slot_send_datagram`
- `DabRadio::slot_handle_tdc_data`
- `DabRadio::slot_change_in_configuration`
- `DabRadio::_slot_terminate_process`
- `DabRadio::_seconds_to_timestring`
- `DabRadio::_slot_update_time_display`
- `DabRadio::_slot_handle_device_widget_button`
- `DabRadio::_slot_handle_tii_button`
- `DabRadio::slot_handle_tii_threshold`
- `DabRadio::slot_handle_tii_subid`
- `DabRadio::slot_fib_time`
- `DabRadio::slot_set_stream_selector`
- `DabRadio::slot_handle_mot_saving_selector`
- `DabRadio::_slot_handle_tech_detail_button`
- `DabRadio::_slot_handle_cir_button`
- `DabRadio::_slot_handle_open_pic_folder_button`
- `DabRadio::_slot_handle_reset_button`
- `DabRadio::start_source_dumping`
- `DabRadio::stop_source_dumping`
- `DabRadio::_slot_handle_source_dump_button`
- `DabRadio::_slot_handle_spectrum_button`
- `DabRadio::_connect_dab_processor`
- `DabRadio::_connect_dab_processor_signals`
- `DabRadio::_disconnect_dab_processor_signals`
- `DabRadio::closeEvent`
- `DabRadio::slot_start_announcement`
- `DabRadio::slot_stop_announcement`
- `DabRadio::_slot_favorite_changed`
- `DabRadio::_slot_service_changed`
- `DabRadio::slot_handle_fib_content_selector`
- `DabRadio::local_select`
- `DabRadio::stop_services`
- `DabRadio::_display_service_label`
- `DabRadio::start_primary_and_secondary_service`
- `DabRadio::_create_primary_backend_audio_service`
- `DabRadio::_create_secondary_backend_packet_service`
- `DabRadio::_create_primary_backend_packet_service`
- `DabRadio::_update_audio_data_addon`
- `DabRadio::_update_scan_statistics`
- `DabRadio::_slot_fib_loaded_state`
- `DabRadio::write_warning_message`
- `DabRadio::start_channel`
- `DabRadio::stop_channel`
- `DabRadio::_slot_handle_channel_selector`
- `DabRadio::_slot_handle_scan_button`
- `DabRadio::start_scanning`
- `DabRadio::stop_scanning`
- `DabRadio::_go_to_next_channel_while_scanning`
- `DabRadio::slot_epg_timer_timeout`
- `DabRadio::_extract_epg`
- `DabRadio::slot_set_epg_data`
- `DabRadio::_slot_handle_time_table`
- `DabRadio::_slot_handle_skip_list_button`
- `DabRadio::_slot_handle_skip_file_button`
- `DabRadio::slot_use_strongest_peak`
- `DabRadio::slot_handle_dc_avoidance_algorithm`
- `DabRadio::slot_handle_dc_and_iq_corr`
- `DabRadio::slot_handle_tii_collisions`
- `DabRadio::LOG`
- `DabRadio::slot_load_table`
- `DabRadio::_slot_handle_http_button`
- `DabRadio::slot_http_terminate`
- `DabRadio::_show_pause_slide`
- `DabRadio::slot_handle_port_selector`
- `DabRadio::_slot_handle_eti_button`
- `DabRadio::start_etiHandler`
- `DabRadio::stop_ETI_handler`
- `DabRadio::slot_test_slider`
- `DabRadio::_set_device_to_file_mode`
- `DabRadio::_convert_links_to_clickable`
- `DabRadio::_check_coordinates`
- `DabRadio::_get_last_service_from_config`
- `DabRadio::_get_local_position_from_config`
- `DabRadio::_initialize_paths`
- `DabRadio::_initialize_epg`
- `DabRadio::_initialize_time_table`
- `DabRadio::_initialize_tii_file`
- `DabRadio::_initialize_band_handler`
- `DabRadio::_initialize_and_start_timers`
- `DabRadio::slot_check_for_update`
- `DabRadio::_slot_check_for_update`
- `DabRadio::_check_on_github_for_update`

### `main/dabradio.h`

- `UTiiId`
- `clean_channel`
- `DabRadio`
- `~DabRadio`
- `get_techdata_widget`
- `LOG`
- `_get_soft_bit_gen_names`
- `_extract_epg`
- `_show_pause_slide`
- `_connect_dab_processor`
- `_connect_dab_processor_signals`
- `_disconnect_dab_processor_signals`
- `_get_YMD_from_mod_julian_date`
- `_get_local_time`
- `_conv_to_time_str`
- `clean_screen`
- `_show_hide_buttons`
- `start_etiHandler`
- `stop_ETI_handler`
- `check_and_create_dir`
- `_create_primary_backend_audio_service`
- `_create_primary_backend_packet_service`
- `_create_secondary_backend_packet_service`
- `start_scanning`
- `stop_scanning`
- `start_audio_dumping`
- `stop_audio_dumping`
- `start_source_dumping`
- `stop_source_dumping`
- `start_audio_frame_dumping`
- `stop_audio_frame_dumping`
- `start_channel`
- `stop_channel`
- `cleanup_ui`
- `stop_services`
- `start_primary_and_secondary_service`
- `local_select`
- `do_start`
- `save_MOT_EPG_data`
- `save_MOT_object`
- `save_MOT_text`
- `show_MOT_image`
- `create_directory`
- `generate_unique_file_path_from_hash`
- `generate_file_path`
- `enable_ui_elements_for_safety`
- `write_warning_message`
- `write_picture`
- `get_bg_style_sheet`
- `set_favorite_button_style`
- `_update_channel_selector`
- `_show_epg_label`
- `_set_http_server_button`
- `_set_clock_text`
- `_add_status_label_elem`
- `_set_status_info_status`
- `_reset_status_info`
- `_set_device_to_file_mode`
- `_setup_audio_output`
- `_get_scan_message`
- `_convert_links_to_clickable`
- `_check_coordinates`
- `_get_last_service_from_config`
- `_get_local_position_from_config`
- `_display_service_label`
- `_update_audio_data_addon`
- `_update_scan_statistics`
- `_show_or_hide_windows_from_config`
- `_go_to_next_channel_while_scanning`
- `_check_on_github_for_update`
- `_emphasize_pushbutton`
- `_seconds_to_timestring`
- `_initialize_ui_buttons`
- `_initialize_status_info`
- `_initialize_dynamic_label`
- `_initialize_thermo_peak_levels`
- `_initialize_audio_output`
- `_initialize_paths`
- `_initialize_epg`
- `_initialize_time_table`
- `_initialize_tii_file`
- `_initialize_version_and_copyright_info`
- `_initialize_band_handler`
- `_initialize_and_start_timers`
- `_initialize_device_selector`
- `signal_set_new_channel`
- `signal_dab_processor_started`
- `signal_test_slider_changed`
- `signal_audio_mute`
- `signal_start_audio`
- `signal_switch_audio`
- `signal_stop_audio`
- `signal_set_audio_device`
- `signal_audio_buffer_filled_state`
- `slot_name_of_ensemble`
- `slot_show_frame_errors`
- `slot_show_rs_errors`
- `slot_show_aac_errors`
- `slot_show_fic_status`
- `slot_show_label`
- `slot_handle_mot_object`
- `slot_send_datagram`
- `slot_handle_tdc_data`
- `slot_change_in_configuration`
- `slot_new_audio`
- `slot_set_stereo`
- `slot_set_stream_selector`
- `slot_handle_mot_saving_selector`
- `slot_show_mot_handling`
- `slot_show_correlation`
- `slot_show_spectrum`
- `slot_show_cir`
- `slot_show_iq`
- `slot_show_lcd_data`
- `slot_show_digital_peak_level`
- `slot_show_rs_corrections`
- `slot_show_tii`
- `slot_fib_time`
- `slot_start_announcement`
- `slot_stop_announcement`
- `slot_new_aac_mp2_frame`
- `slot_show_clock_error`
- `slot_set_epg_data`
- `slot_epg_timer_timeout`
- `slot_handle_fib_content_selector`
- `slot_set_and_show_freq_corr_rf_Hz`
- `slot_show_freq_corr_bb_Hz`
- `slot_test_slider`
- `slot_load_table`
- `slot_handle_dl_text_button`
- `slot_handle_logger_button`
- `slot_handle_port_selector`
- `slot_handle_set_coordinates_button`
- `slot_handle_journaline_viewer_closed`
- `slot_handle_tii_viewer_closed`
- `slot_use_strongest_peak`
- `slot_handle_dc_avoidance_algorithm`
- `slot_handle_dc_and_iq_corr`
- `slot_show_audio_peak_level`
- `slot_handle_tii_collisions`
- `slot_handle_tii_threshold`
- `slot_handle_tii_subid`
- `slot_http_terminate`
- `slot_check_for_update`
- `closeEvent`
- `_slot_handle_time_table`
- `_slot_handle_content_button`
- `_slot_handle_tech_detail_button`
- `_slot_handle_reset_button`
- `_slot_handle_scan_button`
- `_slot_handle_eti_button`
- `_slot_handle_spectrum_button`
- `_slot_handle_cir_button`
- `_slot_handle_open_pic_folder_button`
- `_slot_handle_device_widget_button`
- `_slot_do_start`
- `_slot_new_device`
- `_slot_handle_source_dump_button`
- `_slot_handle_frame_dump_button`
- `_slot_handle_audio_dump_button`
- `_slot_handle_tii_button`
- `_slot_handle_prev_service_button`
- `_slot_handle_next_service_button`
- `_slot_handle_target_service_button`
- `_slot_handle_channel_selector`
- `_slot_terminate_process`
- `_slot_update_time_display`
- `_slot_audio_level_decay_timeout`
- `_slot_scanning_security_timeout`
- `_slot_scanning_no_signal_timeout`
- `_slot_service_changed`
- `_slot_favorite_changed`
- `_slot_handle_favorite_button`
- `_slot_set_static_button_style`
- `_slot_fib_loaded_state`
- `_slot_handle_mute_button`
- `_slot_update_mute_state`
- `_slot_handle_config_button`
- `_slot_handle_http_button`
- `_slot_handle_skip_list_button`
- `_slot_handle_skip_file_button`
- `_slot_load_audio_device_list`
- `_slot_handle_volume_slider`
- `_slot_check_for_update`

### `main/dabradio_audio.cpp`

- `DabRadio::_initialize_audio_output`
- `DabRadio::slot_show_audio_peak_level`
- `DabRadio::_slot_audio_level_decay_timeout`
- `DabRadio::_setup_audio_output`
- `DabRadio::_slot_load_audio_device_list`
- `DabRadio::_slot_handle_volume_slider`
- `DabRadio::slot_new_audio`
- `DabRadio::_slot_handle_audio_dump_button`
- `DabRadio::start_audio_dumping`
- `DabRadio::stop_audio_dumping`
- `DabRadio::_slot_handle_frame_dump_button`
- `DabRadio::start_audio_frame_dumping`
- `DabRadio::stop_audio_frame_dumping`
- `DabRadio::slot_new_aac_mp2_frame`

### `main/dabradio_ui.cpp`

- `DabRadio::_add_status_label_elem`
- `DabRadio::_set_status_info_status`
- `constexpr`
- `DabRadio::_emphasize_pushbutton`
- `DabRadio::_reset_status_info`
- `DabRadio::_initialize_ui_buttons`
- `DabRadio::_initialize_status_info`
- `DabRadio::_initialize_dynamic_label`
- `DabRadio::_initialize_thermo_peak_levels`
- `DabRadio::_initialize_device_selector`
- `DabRadio::_initialize_version_and_copyright_info`
- `DabRadio::get_bg_style_sheet`
- `DabRadio::set_favorite_button_style`
- `DabRadio::_get_soft_bit_gen_names`
- `DabRadio::cleanup_ui`
- `DabRadio::_set_clock_text`
- `DabRadio::_show_epg_label`
- `DabRadio::_show_hide_buttons`
- `DabRadio::slot_handle_logger_button`
- `DabRadio::slot_handle_set_coordinates_button`
- `DabRadio::slot_handle_journaline_viewer_closed`
- `DabRadio::slot_handle_tii_viewer_closed`
- `DabRadio::_slot_handle_favorite_button`
- `DabRadio::_slot_set_static_button_style`
- `DabRadio::_show_or_hide_windows_from_config`
- `DabRadio::slot_handle_dl_text_button`
- `DabRadio::_slot_handle_config_button`
- `DabRadio::slot_set_and_show_freq_corr_rf_Hz`
- `DabRadio::slot_show_freq_corr_bb_Hz`
- `DabRadio::_get_YMD_from_mod_julian_date`
- `DabRadio::_get_local_time`
- `DabRadio::_conv_to_time_str`
- `DabRadio::clean_screen`
- `DabRadio::slot_show_frame_errors`
- `DabRadio::slot_show_rs_errors`
- `DabRadio::slot_show_aac_errors`
- `DabRadio::slot_show_fic_status`
- `DabRadio::slot_show_mot_handling`
- `DabRadio::slot_show_label`
- `DabRadio::slot_set_stereo`
- `tiiNumber`
- `DabRadio::slot_show_tii`
- `DabRadio::slot_show_spectrum`
- `DabRadio::slot_show_cir`
- `DabRadio::slot_show_iq`
- `DabRadio::slot_show_lcd_data`
- `DabRadio::slot_show_digital_peak_level`
- `DabRadio::slot_show_rs_corrections`
- `DabRadio::slot_show_clock_error`
- `DabRadio::slot_show_correlation`
- `hex_to_str`
- `DabRadio::slot_name_of_ensemble`
- `DabRadio::_slot_handle_content_button`
- `DabRadio::_slot_handle_prev_service_button`
- `DabRadio::_slot_handle_next_service_button`
- `DabRadio::_slot_handle_target_service_button`
- `DabRadio::enable_ui_elements_for_safety`
- `DabRadio::_slot_handle_mute_button`
- `DabRadio::_slot_update_mute_state`
- `DabRadio::_update_channel_selector`
- `DabRadio::_set_http_server_button`

### `main/glob_data_types.h`

- No function declarations detected

### `main/glob_defs.h`

- `conv_deg_to_rad`
- `limit_min_max`
- `limit_symmetrically`
- `fixround`
- `is_indeterminate`
- `fast_abs_with_clip_det`
- `norm_to_length_one`
- `std::abs`
- `cmplx_from_phase`
- `log10_times_10`
- `log10_times_20`
- `abs_log10_with_offset_and_phase`
- `abs_log10_with_offset`
- `turn_phase_to_first_quadrant`
- `turn_complex_phase_to_first_quadrant`
- `calc_adaptive_alpha`
- `std::log10`
- `std::pow`
- `mean_filter`
- `mean_filter_adaptive`
- `create_blackman_window`
- `create_flattop_window`
- `std::cos`
- `get_range_from_bit_depth`
- `fft_shift`
- `fft_shift_skip_dc`
- `cmplx_to_polar_str`
- `std::setw`

### `main/glob_enums.h`

- No function declarations detected

### `main/main.cpp`

- `main`

### `main/mot-content-types.h`

- `getContentBaseType`
- `getContentSubType`

### `ofdm/freq-interleaver.cpp`

- `FreqInterleaver::FreqInterleaver`
- `FreqInterleaver::createMapper`

### `ofdm/freq-interleaver.h`

- `FreqInterleaver`
- `~FreqInterleaver`
- `map_k_to_fft_bin`
- `createMapper`

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
- `alignas`
- `signal_show_correlation`

### `ofdm/phasetable.cpp`

- `PhaseTable::PhaseTable`
- `PhaseTable::h_table`
- `PhaseTable::get_phi`

### `ofdm/phasetable.h`

- `PhaseTable`
- `~PhaseTable`
- `alignas`
- `h_table`
- `get_phi`

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
- `alignas`
- `_dump_samples_to_file`
- `signal_show_spectrum`
- `signal_show_cir`

### `ofdm/tii-detector.cpp`

- `rev_bit_val`
- `TiiDetector::TiiDetector`
- `TiiDetector::~TiiDetector`
- `TiiDetector::set_detect_collisions`
- `TiiDetector::set_subid_for_collision_search`
- `TiiDetector::reset`
- `TiiDetector::add_to_tii_buffer`
- `TiiDetector::process_tii_data`
- `TiiDetector::_reset_null_symbol_buffer`
- `TiiDetector::_decode_and_accumulate_carrier_pairs`
- `TiiDetector::_remove_single_carrier_values`
- `TiiDetector::_turn_phase`
- `TiiDetector::_collapse_tii_groups`
- `TiiDetector::_find_exact_main_id_match`
- `TiiDetector::_find_best_main_id_match`
- `TiiDetector::_compare_etsi_and_non_etsi`
- `TiiDetector::_find_collisions`
- `TiiDetector::_get_float_table_and_max_abs_value`
- `TiiDetector::_calculate_average_noise`

### `ofdm/tii-detector.h`

- `TiiDetector`
- `~TiiDetector`
- `reset`
- `set_detect_collisions`
- `set_subid_for_collision_search`
- `add_to_tii_buffer`
- `process_tii_data`
- `_calculate_average_noise`
- `_get_float_table_and_max_abs_value`
- `_compare_etsi_and_non_etsi`
- `_find_collisions`
- `_find_exact_main_id_match`
- `_find_best_main_id_match`
- `_reset_null_symbol_buffer`
- `_remove_single_carrier_values`
- `_decode_and_accumulate_carrier_pairs`
- `_collapse_tii_groups`
- `_turn_phase`

### `ofdm/timesyncer.cpp`

- `TimeSyncer::TimeSyncer`
- `TimeSyncer::read_samples_until_end_of_level_drop`

### `ofdm/timesyncer.h`

- `TimeSyncer`
- `~TimeSyncer`
- `read_samples_until_end_of_level_drop`

### `protection/eep-protection.cpp`

- `EepProtection::EepProtection`
- `EepProtection::_extract_viterbi_block_addresses`

### `protection/eep-protection.h`

- `EepProtection`
- `~EepProtection`
- `_extract_viterbi_block_addresses`

### `protection/protTables.cpp`

- `get_PI_codes`

### `protection/protTables.h`

- `get_PI_codes`

### `protection/protection.cpp`

- `Protection::Protection`
- `Protection::deconvolve`

### `protection/protection.h`

- `Protection`
- `~Protection`
- `deconvolve`

### `protection/uep-protection.cpp`

- `find_index`
- `UepProtection::UepProtection`
- `UepProtection::_extract_viterbi_block_addresses`

### `protection/uep-protection.h`

- `UepProtection`
- `~UepProtection`
- `_extract_viterbi_block_addresses`

### `scopes/audio-display.cpp`

- `AudioDisplay::AudioDisplay`
- `AudioDisplay::~AudioDisplay`
- `AudioDisplay::create_spectrum`
- `AudioDisplay::_slot_rightMouseClick`

### `scopes/audio-display.h`

- `AudioDisplay`
- `~AudioDisplay`
- `create_spectrum`
- `static_assert`
- `alignas`
- `fftwf_plan_dft_1d`
- `fftwf_plan_dft_r2c_1d`
- `_slot_rightMouseClick`

### `scopes/carrier-display.cpp`

- `CarrierDisp::CarrierDisp`
- `CarrierDisp::display_carrier_plot`
- `CarrierDisp::select_plot_type`
- `CarrierDisp::_customize_plot`
- `CarrierDisp::_setup_x_axis`
- `CarrierDisp::_get_plot_type_data`
- `CarrierDisp::get_plot_type_names`

### `scopes/carrier-display.h`

- `CarrierDisp`
- `~CarrierDisp`
- `display_carrier_plot`
- `select_plot_type`
- `get_plot_type_names`
- `_customize_plot`
- `_get_plot_type_data`
- `_setup_x_axis`

### `scopes/cust_qwt_zoom_pan.cpp`

- `CustQwtZoomPan::CustQwtZoomPan`
- `CustQwtZoomPan::reset_x_zoom`
- `CustQwtZoomPan::reset_y_zoom`
- `CustQwtZoomPan::_set_range`
- `CustQwtZoomPan::set_x_range`
- `CustQwtZoomPan::set_y_range`
- `CustQwtZoomPan::eventFilter`
- `CustQwtZoomPan::_handle_mouse_press`
- `CustQwtZoomPan::_handle_mouse_release`
- `CustQwtZoomPan::_handle_mouse_move`
- `CustQwtZoomPan::_handle_wheel_event`

### `scopes/cust_qwt_zoom_pan.h`

- `SRange`
- `CustQwtZoomPan`
- `~CustQwtZoomPan`
- `reset_x_zoom`
- `reset_y_zoom`
- `set_x_range`
- `set_y_range`
- `eventFilter`
- `_set_range`
- `_handle_mouse_press`
- `_handle_mouse_release`
- `_handle_mouse_move`
- `_handle_wheel_event`

### `scopes/iqdisplay.cpp`

- `IQDisplay::IQDisplay`
- `IQDisplay::~IQDisplay`
- `IQDisplay::set_point`
- `IQDisplay::display_iq`
- `IQDisplay::clean_screen_from_old_data_points`
- `IQDisplay::draw_cross`
- `IQDisplay::draw_circle`
- `IQDisplay::repaint_circle`
- `IQDisplay::select_plot_type`
- `IQDisplay::set_map_1st_quad`
- `IQDisplay::customize_plot`
- `IQDisplay::_get_plot_type_data`
- `IQDisplay::get_plot_type_names`

### `scopes/iqdisplay.h`

- `IQDisplay`
- `~IQDisplay`
- `display_iq`
- `customize_plot`
- `select_plot_type`
- `set_map_1st_quad`
- `get_plot_type_names`
- `set_point`
- `clean_screen_from_old_data_points`
- `draw_cross`
- `draw_circle`
- `repaint_circle`
- `_get_plot_type_data`

### `scopes/spectrogramdata.cpp`

- `SpectrogramData::SpectrogramData`
- `SpectrogramData::initRaster`
- `SpectrogramData::value`
- `SpectrogramData::set_min_max_z_value`

### `scopes/spectrogramdata.h`

- `SpectrogramData`
- `~SpectrogramData`
- `set_min_max_z_value`
- `initRaster`
- `value`

### `server-thread/tcp-server.cpp`

- `tcpServer::tcpServer`
- `tcpServer::~tcpServer`
- `tcpServer::sendData`
- `tcpServer::run`

### `server-thread/tcp-server.h`

- `tcpServer::tcpServer`
- `tcpServer::~tcpServer`
- `tcpServer::sendData`
- `tcpServer::run`

### `service-list/service-db.cpp`

- `data`
- `ServiceDB::ServiceDB`
- `ServiceDB::~ServiceDB`
- `ServiceDB::open_db`
- `ServiceDB::create_table`
- `ServiceDB::delete_table`
- `ServiceDB::add_entry`
- `ServiceDB::delete_entry`
- `ServiceDB::create_model`
- `ServiceDB::sort_column`
- `ServiceDB::is_sort_desc`
- `ServiceDB::set_favorite`
- `ServiceDB::retrieve_favorites_from_backup_table`
- `ServiceDB::_set_favorite`
- `ServiceDB::_open_db`
- `ServiceDB::_delete_db_file`
- `ServiceDB::_error_str`
- `ServiceDB::_exec_simple_query`
- `ServiceDB::_check_if_entry_exists`
- `ServiceDB::_cur_tab_name`
- `ServiceDB::set_data_mode`

### `service-list/service-db.h`

- `ServiceDB`
- `~ServiceDB`
- `set_data_mode`
- `open_db`
- `create_table`
- `delete_table`
- `add_entry`
- `delete_entry`
- `sort_column`
- `is_sort_desc`
- `set_favorite`
- `retrieve_favorites_from_backup_table`
- `create_model`
- `_error_str`
- `_delete_db_file`
- `_open_db`
- `_exec_simple_query`
- `_check_if_entry_exists`
- `_set_favorite`
- `_cur_tab_name`

### `service-list/service-list-handler.cpp`

- `CustomItemDelegate::editorEvent`
- `ServiceListHandler::ServiceListHandler`
- `ServiceListHandler::add_entry`
- `ServiceListHandler::delete_not_existing_SId_at_channel`
- `ServiceListHandler::delete_table`
- `ServiceListHandler::create_new_table`
- `ServiceListHandler::set_selector`
- `ServiceListHandler::set_selector_channel_only`
- `ServiceListHandler::set_favorite_state`
- `ServiceListHandler::restore_favorites`
- `ServiceListHandler::_fill_table_view_from_db`
- `ServiceListHandler::jump_entries`
- `ServiceListHandler::get_list_of_SId_in_channel`
- `ServiceListHandler::_jump_to_list_entry_and_emit_fav_status`
- `ServiceListHandler::_slot_selection_changed_with_fav`
- `ServiceListHandler::_slot_header_clicked`
- `ServiceListHandler::set_data_mode`

### `service-list/service-list-handler.h`

- `set_current_service`
- `editorEvent`
- `signal_selection_changed_with_fav`
- `ServiceListHandler`
- `~ServiceListHandler`
- `set_data_mode`
- `delete_table`
- `create_new_table`
- `add_entry`
- `delete_not_existing_SId_at_channel`
- `set_selector`
- `set_selector_channel_only`
- `set_favorite_state`
- `restore_favorites`
- `jump_entries`
- `get_list_of_SId_in_channel`
- `_fill_table_view_from_db`
- `_jump_to_list_entry_and_emit_fav_status`
- `_slot_selection_changed_with_fav`
- `_slot_header_clicked`
- `signal_selection_changed`
- `signal_favorite_status`

### `specials/dumpviewer/dump-viewer.cpp`

- `dumpViewer::dumpViewer`
- `dumpViewer::~dumpViewer`
- `dumpViewer::handle_viewSlider`
- `dumpViewer::handle_amplitudeSlider`
- `dumpViewer::handle_compressor`
- `dumpViewer::show_segment`

### `specials/dumpviewer/dump-viewer.h`

- `dumpViewer`
- `~dumpViewer`
- `handle_viewSlider`
- `handle_amplitudeSlider`
- `handle_compressor`
- `show_segment`

### `specials/dumpviewer/main.cpp`

- `main`

### `spectrum-viewer/cir-viewer.cpp`

- `CirViewer::CirViewer`
- `CirViewer::~CirViewer`
- `CirViewer::show_cir`
- `CirViewer::show`
- `CirViewer::hide`
- `CirViewer::is_hidden`

### `spectrum-viewer/cir-viewer.h`

- `CirViewer`
- `~CirViewer`
- `show_cir`
- `show`
- `hide`
- `is_hidden`
- `alignas`
- `signal_frame_closed`

### `spectrum-viewer/correlation-viewer.cpp`

- `CorrelationViewer::CorrelationViewer`
- `CorrelationViewer::showCorrelation`
- `CorrelationViewer::_get_best_match_text`

### `spectrum-viewer/correlation-viewer.h`

- `CorrelationViewer`
- `~CorrelationViewer`
- `showCorrelation`
- `_get_best_match_text`

### `spectrum-viewer/spectrum-scope.cpp`

- `SpectrumScope::SpectrumScope`
- `SpectrumScope::~SpectrumScope`
- `SpectrumScope::show_spectrum`
- `SpectrumScope::slot_right_mouse_click`
- `SpectrumScope::slot_scaling_changed`

### `spectrum-viewer/spectrum-scope.h`

- `SpectrumScope`
- `~SpectrumScope`
- `show_spectrum`
- `slot_scaling_changed`
- `slot_right_mouse_click`

### `spectrum-viewer/spectrum-viewer.cpp`

- `SpectrumViewer::SpectrumViewer`
- `SpectrumViewer::~SpectrumViewer`
- `SpectrumViewer::_calc_spectrum_display_limits`
- `SpectrumViewer::show_spectrum`
- `SpectrumViewer::show`
- `SpectrumViewer::hide`
- `SpectrumViewer::is_hidden`
- `SpectrumViewer::show_iq`
- `SpectrumViewer::show_lcd_data`
- `SpectrumViewer::show_fic_ber`
- `SpectrumViewer::show_nominal_frequency_MHz`
- `SpectrumViewer::show_freq_corr_rf_Hz`
- `SpectrumViewer::show_freq_corr_bb_Hz`
- `SpectrumViewer::show_clock_error`
- `SpectrumViewer::show_correlation`
- `SpectrumViewer::_slot_handle_cmb_carrier`
- `SpectrumViewer::_slot_handle_cmb_iqscope`
- `SpectrumViewer::_slot_handle_cb_nom_carrier`
- `SpectrumViewer::_slot_handle_cb_map_1st_quad`
- `SpectrumViewer::slot_update_settings`
- `SpectrumViewer::show_digital_peak_level`
- `SpectrumViewer::set_spectrum_averaging_rate`

### `spectrum-viewer/spectrum-viewer.h`

- `SpectrumViewer`
- `~SpectrumViewer`
- `show_spectrum`
- `show_correlation`
- `show_nominal_frequency_MHz`
- `show_freq_corr_rf_Hz`
- `show_freq_corr_bb_Hz`
- `show_iq`
- `show_lcd_data`
- `show_fic_ber`
- `show_clock_error`
- `show`
- `hide`
- `is_hidden`
- `show_digital_peak_level`
- `set_spectrum_averaging_rate`
- `alignas`
- `fftwf_plan_dft_1d`
- `_calc_spectrum_display_limits`
- `slot_update_settings`
- `_slot_handle_cmb_carrier`
- `_slot_handle_cmb_iqscope`
- `_slot_handle_cb_nom_carrier`
- `_slot_handle_cb_map_1st_quad`
- `signal_cmb_carrier_changed`
- `signal_cmb_iqscope_changed`
- `signal_cb_nom_carrier_changed`
- `signal_window_closed`

### `spectrum-viewer/waterfall-scope.cpp`

- `WaterfallScope::WaterfallScope`
- `WaterfallScope::~WaterfallScope`
- `WaterfallScope::show_waterfall`
- `WaterfallScope::_gen_color_map`
- `WaterfallScope::slot_scaling_changed`

### `spectrum-viewer/waterfall-scope.h`

- `WaterfallScope`
- `~WaterfallScope`
- `show_waterfall`
- `_gen_color_map`
- `slot_scaling_changed`

### `support/ITU_Region_1.cpp`

- `find_ITU_code`
- `find_Country`

### `support/ITU_Region_1.h`

- `find_Country`
- `find_ITU_code`

### `support/Xtan2.cpp`

- `compAtan::~compAtan`
- `compAtan::atan2`
- `compAtan::argX`

### `support/Xtan2.h`

- `compAtan::compAtan`
- `compAtan::~compAtan`
- `compAtan::atan2`
- `compAtan::argX`

### `support/band-handler.cpp`

- `BandHandler::BandHandler`
- `BandHandler::~BandHandler`
- `BandHandler::setupChannels`
- `BandHandler::saveSettings`
- `BandHandler::setup_skipList`
- `BandHandler::default_skipList`
- `BandHandler::file_skipList`
- `BandHandler::updateEntry`
- `BandHandler::get_frequency_Hz`
- `BandHandler::firstChannel`
- `BandHandler::nextChannel`
- `BandHandler::slot_cell_selected`
- `BandHandler::show`
- `BandHandler::hide`
- `BandHandler::isHidden`

### `support/band-handler.h`

- `BandHandler`
- `~BandHandler`
- `saveSettings`
- `setupChannels`
- `setup_skipList`
- `get_frequency_Hz`
- `firstChannel`
- `nextChannel`
- `show`
- `hide`
- `isHidden`
- `slot_cell_selected`
- `default_skipList`
- `file_skipList`
- `updateEntry`

### `support/bandpass-filter.cpp`

- `BandPassFIR::BandPassFIR`
- `BandPassFIR::~BandPassFIR`
- `BandPassFIR::Pass`

### `support/bandpass-filter.h`

- `BandPassFIR::BandPassFIR`
- `BandPassFIR::~BandPassFIR`
- `BandPassFIR::Pass`

### `support/buttons/circlepushbutton.cpp`

- `CirclePushButton::CirclePushButton`
- `CirclePushButton::init`
- `CirclePushButton::start_animation`
- `CirclePushButton::stop_animation`
- `CirclePushButton::paintEvent`
- `CirclePushButton::_slot_update_position`

### `support/buttons/circlepushbutton.h`

- `CirclePushButton`
- `init`
- `start_animation`
- `stop_animation`
- `paintEvent`
- `_slot_update_position`

### `support/buttons/newpushbutton.cpp`

- `newPushButton::newPushButton`
- `newPushButton::~newPushButton`

### `support/buttons/newpushbutton.h`

- `newPushButton::newPushButton`
- `newPushButton::~newPushButton`
- `newPushButton::mousePressEvent`
- `newPushButton::rightClicked`

### `support/buttons/normalpushbutton.cpp`

- `normalPushButton::normalPushButton`
- `normalPushButton::sizeHint`
- `normalPushButton::mousePressEvent`

### `support/buttons/normalpushbutton.h`

- `normalPushButton`
- `~normalPushButton`
- `sizeHint`
- `mousePressEvent`
- `rightClicked`

### `support/color-selector.cpp`

- `ColorSelector::show_dialog`

### `support/color-selector.h`

- `show_dialog`

### `support/compass_direction.cpp`

- `CompassDirection::get_compass_direction`

### `support/compass_direction.h`

- `CompassDirection`
- `~CompassDirection`
- `get_compass_direction`

### `support/content-table.cpp`

- `operator<`
- `FibContentTable::FibContentTable`
- `FibContentTable::~FibContentTable`
- `FibContentTable::show`
- `FibContentTable::hide`
- `FibContentTable::is_visible`
- `FibContentTable::add_line`
- `FibContentTable::dump`
- `FibContentTable::_add_row`
- `FibContentTable::_hex_to_u32`
- `FibContentTable::_slot_select_service`
- `FibContentTable::_slot_dump`

### `support/content-table.h`

- `operator<`
- `FibContentTable`
- `~FibContentTable`
- `show`
- `hide`
- `is_visible`
- `add_line`
- `dump`
- `_add_row`
- `_hex_to_u32`
- `_slot_select_service`
- `_slot_dump`
- `signal_go_service_id`

### `support/converted_map.h`

- No function declarations detected

### `support/coordinates.cpp`

- `validate`
- `Coordinates::Coordinates`
- `Coordinates::~Coordinates`
- `Coordinates::slot_accept_button`

### `support/coordinates.h`

- `Coordinates`
- `~Coordinates`
- `slot_accept_button`

### `support/copyright_info.cpp`

- `hyperlink`
- `get_copyright_text`

### `support/copyright_info.h`

- `get_copyright_text`

### `support/custom_frame.cpp`

- `CustomFrame::closeEvent`

### `support/custom_frame.h`

- `closeEvent`
- `signal_frame_closed`

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

### `support/distance.cpp`

- `deg2rad`
- `distance`

### `support/dl-cache.cpp`

- `DynLinkCache::DynLinkCache`
- `DynLinkCache::add`
- `DynLinkCache::is_member`
- `DynLinkCache::add_if_new`

### `support/dl-cache.h`

- `DynLinkCache`
- `add`
- `is_member`
- `add_if_new`

### `support/fir-filters.cpp`

- `LowPassFIR::LowPassFIR`
- `LowPassFIR::~LowPassFIR`
- `LowPassFIR::theSize`
- `LowPassFIR::resize`
- `LowPassFIR::Pass`

### `support/fir-filters.h`

- `LowPassFIR::LowPassFIR`
- `LowPassFIR::~LowPassFIR`
- `LowPassFIR::Pass`
- `LowPassFIR::resize`
- `LowPassFIR::theSize`

### `support/halfbandfilter.cpp`

- `HalfBandFilter::HalfBandFilter`
- `HalfBandFilter::~HalfBandFilter`
- `HalfBandFilter::decimate`

### `support/halfbandfilter.h`

- `HalfBandFilter`
- `~HalfBandFilter`
- `decimate`

### `support/map-http-server.cpp`

- `MapHttpServer::MapHttpServer`
- `MapHttpServer::~MapHttpServer`
- `MapHttpServer::start`
- `MapHttpServer::stop`
- `MapHttpServer::_slot_new_connection`
- `MapHttpServer::_slot_ready_read`
- `MapHttpServer::_slot_ajax_request_timeout`
- `MapHttpServer::_gen_html_code`
- `MapHttpServer::_move_transmitter_list_to_json`
- `MapHttpServer::add_transmitter_location_entry`

### `support/map-http-server.h`

- `MapHttpServer`
- `~MapHttpServer`
- `start`
- `stop`
- `add_transmitter_location_entry`
- `_gen_html_code`
- `_move_transmitter_list_to_json`
- `_slot_new_connection`
- `_slot_ready_read`
- `_slot_ajax_request_timeout`
- `signal_terminating`

### `support/mapport.cpp`

- `MapPortHandler::MapPortHandler`
- `MapPortHandler::~MapPortHandler`
- `MapPortHandler::handle_acceptButton`

### `support/mapport.h`

- `MapPortHandler`
- `~MapPortHandler`
- `handle_acceptButton`

### `support/openfiledialog.cpp`

- `OpenFileDialog::OpenFileDialog`
- `OpenFileDialog::open_file`
- `OpenFileDialog::open_snd_file`
- `OpenFileDialog::open_content_dump_file_ptr`
- `OpenFileDialog::open_frame_dump_file_ptr`
- `OpenFileDialog::open_audio_dump_sndfile_ptr`
- `OpenFileDialog::open_raw_dump_sndfile_ptr`
- `OpenFileDialog::get_audio_dump_file_name`
- `OpenFileDialog::get_skip_file_file_name`
- `OpenFileDialog::get_dl_text_file_name`
- `OpenFileDialog::open_log_file_ptr`
- `OpenFileDialog::get_maps_file_name`
- `OpenFileDialog::get_eti_file_name`
- `OpenFileDialog::_open_file_dialog`
- `OpenFileDialog::open_sample_data_file_dialog_for_reading`
- `OpenFileDialog::get_file_type`
- `OpenFileDialog::_remove_invalid_characters`
- `OpenFileDialog::open_raw_dump_xmlfile_ptr`

### `support/openfiledialog.h`

- `OpenFileDialog`
- `~OpenFileDialog`
- `open_file`
- `open_snd_file`
- `open_content_dump_file_ptr`
- `open_frame_dump_file_ptr`
- `open_log_file_ptr`
- `open_raw_dump_xmlfile_ptr`
- `open_audio_dump_sndfile_ptr`
- `open_raw_dump_sndfile_ptr`
- `get_audio_dump_file_name`
- `get_skip_file_file_name`
- `get_dl_text_file_name`
- `get_maps_file_name`
- `get_eti_file_name`
- `open_sample_data_file_dialog_for_reading`
- `get_file_type`
- `_open_file_dialog`
- `_remove_invalid_characters`

### `support/process-params.h`

- No function declarations detected

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

### `support/setting-helper.cnf.h`

- No function declarations detected

### `support/setting-helper.cpp`

- `Variant::Variant`
- `Variant::read`
- `Variant::write`
- `Widget::Widget`
- `Widget::~Widget`
- `Widget::register_widget_and_update_ui_from_setting`
- `Widget::read`
- `Widget::get_combobox_index`
- `Widget::_update_ui_state_from_setting`
- `Widget::_update_ui_state_to_setting`
- `Widget::_update_ui_state_to_setting_deferred`
- `PosAndSize::PosAndSize`
- `PosAndSize::write_widget_geometry`

### `support/setting-helper.h`

- `instance`
- `Storage`
- `Variant`
- `define_default_value`
- `read`
- `write`
- `Widget`
- `~Widget`
- `register_widget_and_update_ui_from_setting`
- `get_combobox_index`
- `_update_ui_state_from_setting`
- `_update_ui_state_to_setting`
- `_update_ui_state_to_setting_deferred`
- `PosAndSize`
- `read_widget_geometry`
- `write_widget_geometry`

### `support/simd_extensions.h`

- `simd_abs`
- `_mm_store_ps`
- `simd_normalize`
- `SimdVec`
- `~SimdVec`
- `volk_free`
- `fill_zeros`
- `std::fill_n`
- `operator[]`
- `get`
- `operatorT*`
- `size`
- `set_normalize_each_element`
- `set_back_rotate_phase_each_element`
- `volk_32f_cos_32f_a`
- `volk_32f_sin_32f_a`
- `volk_32f_x2_interleave_32fc_a`
- `volk_32fc_x2_multiply_conjugate_32fc_a`
- `modify_multiply_scalar_each_element`
- `volk_32f_s32f_multiply_32f_a`
- `set_multiply_conj_each_element`
- `set_multiply_each_element`
- `volk_32f_x2_multiply_32f_a`
- `modify_multiply_each_element`
- `set_divide_each_element`
- `volk_32f_x2_divide_32f_a`
- `set_add_each_element`
- `volk_32f_x2_add_32f_a`
- `set_subtract_each_element`
- `volk_32f_x2_subtract_32f_a`
- `set_magnitude_each_element`
- `volk_32fc_magnitude_32f_a`
- `set_squared_magnitude_each_element`
- `volk_32fc_magnitude_squared_32f_a`
- `set_sqrt_each_element`
- `volk_32f_sqrt_32f_a`
- `modify_sqrt_each_element`
- `set_arg_each_element`
- `volk_32fc_s32f_atan2_32f_a`
- `set_wrap_4QPSK_to_phase_zero_each_element`
- `volk_32f_s32f_s32f_mod_range_32f_a`
- `volk_32f_s32f_add_32f_a`
- `modify_add_scalar_each_element`
- `set_add_vector_and_scalar_each_element`
- `set_multiply_vector_and_scalar_each_element`
- `modify_accumulate_each_element`
- `set_square_each_element`
- `modify_mean_filter_each_element`
- `get_sum_of_elements`
- `volk_32f_accumulator_s32f_a`
- `modify_limit_symmetrically_each_element`
- `volk_32f_s32f_x2_clamp_32f_a`
- `modify_check_negative_or_zero_values_and_fallback_each_element`
- `set_squared_distance_to_nearest_constellation_point_each_element`
- `set_squared_magnitude_of_elements`
- `store_to_real_and_imag_each_element`
- `volk_32fc_deinterleave_32f_x2_a`

### `support/techdata.cpp`

- `TechData::TechData`
- `TechData::~TechData`
- `TechData::cleanUp`
- `TechData::show_service_data`
- `TechData::show_service_data_addon`
- `TechData::show`
- `TechData::hide`
- `TechData::isHidden`
- `TechData::slot_show_frame_error_bar`
- `TechData::slot_show_aac_error_bar`
- `TechData::slot_show_rs_error_bar`
- `TechData::slot_show_rs_corrections`
- `TechData::slot_trigger_motHandling`
- `TechData::_slot_show_motHandling`
- `TechData::slot_show_timetableButton`
- `TechData::_show_service_label`
- `TechData::_show_SId`
- `TechData::_show_bitrate`
- `TechData::_show_CU_start_address`
- `TechData::_show_CU_size`
- `TechData::_show_subChId`
- `TechData::slot_show_language`
- `TechData::_show_ASCTy`
- `TechData::_show_uep_eep`
- `TechData::_show_coderate`
- `TechData::slot_show_fm`
- `TechData::slot_audio_data_available`
- `TechData::slot_show_sample_rate_and_audio_flags`

### `support/techdata.h`

- `TechData`
- `~TechData`
- `show_service_data`
- `show_service_data_addon`
- `cleanUp`
- `show`
- `hide`
- `isHidden`
- `_show_service_label`
- `_show_SId`
- `_show_bitrate`
- `_show_subChId`
- `_show_CU_start_address`
- `_show_CU_size`
- `_show_ASCTy`
- `_show_uep_eep`
- `_show_coderate`
- `slot_trigger_motHandling`
- `slot_show_frame_error_bar`
- `slot_show_aac_error_bar`
- `slot_show_rs_error_bar`
- `slot_show_rs_corrections`
- `slot_show_timetableButton`
- `slot_show_language`
- `slot_show_fm`
- `slot_show_sample_rate_and_audio_flags`
- `slot_audio_data_available`
- `_slot_show_motHandling`
- `signal_handle_timeTable`
- `signal_handle_audioDumping`
- `signal_handle_frameDumping`
- `signal_window_closed`

### `support/tii-library/tii-codes.cpp`

- `TiiHandler::~TiiHandler`
- `TiiHandler::_load_library`
- `TiiHandler::loadTable`
- `TiiHandler::fill_cache_from_tii_file`
- `TiiHandler::get_transmitter_data`
- `TiiHandler::_distance_2`
- `TiiHandler::distance`
- `TiiHandler::corner`
- `TiiHandler::_convert`
- `TiiHandler::_get_E_id`
- `TiiHandler::_get_main_id`
- `TiiHandler::_get_sub_id`
- `TiiHandler::_read_file`
- `TiiHandler::_read_columns`
- `TiiHandler::_eread`
- `TiiHandler::is_valid`
- `TiiHandler::_load_dyn_library_functions`

### `support/tii-library/tii-codes.h`

- `patch_channel_name`
- `make_key_base`
- `TiiHandler`
- `~TiiHandler`
- `fill_cache_from_tii_file`
- `get_transmitter_data`
- `distance`
- `corner`
- `is_valid`
- `loadTable`
- `_load_library`
- `_convert`
- `_get_E_id`
- `_get_main_id`
- `_get_sub_id`
- `_distance_2`
- `_read_columns`
- `_read_file`
- `_eread`
- `_load_dyn_library_functions`

### `support/tii_list_display.cpp`

- `CustomScrollArea::closeEvent`
- `TiiListDisplay::TiiListDisplay`
- `TiiListDisplay::~TiiListDisplay`
- `TiiListDisplay::set_window_title`
- `TiiListDisplay::start_adding`
- `TiiListDisplay::finish_adding`
- `TiiListDisplay::get_nr_rows`
- `TiiListDisplay::show`
- `TiiListDisplay::hide`
- `TiiListDisplay::add_row`

### `support/tii_list_display.h`

- `closeEvent`
- `signal_frame_closed`
- `TiiListDisplay`
- `~TiiListDisplay`
- `set_window_title`
- `add_row`
- `start_adding`
- `show`
- `hide`
- `get_nr_rows`
- `finish_adding`

### `support/time-table.cpp`

- `TimeTableHandler::TimeTableHandler`
- `TimeTableHandler::~TimeTableHandler`
- `TimeTableHandler::clear`

### `support/time-table.h`

- `TimeTableHandler`
- `~TimeTableHandler`
- `addElement`
- `clear`

### `support/time_meas.h`

- `TimeMeas`
- `_add_ticks`
- `~TimeMeas`
- `trigger_begin`
- `std::chrono::steady_clock::now`
- `std::chrono::high_resolution_clock::now`
- `trigger_end`
- `trigger_loop`
- `get_time_per_round_in_ns`
- `print_time_per_round`
- `std::chrono::nanoseconds::max`
- `std::chrono::nanoseconds::min`
- `std::chrono::nanoseconds::zero`
- `std::to_string`

### `support/viterbi-spiral/sse2neon/sse2neon.h`

- `_mm_prefetch`
- `__builtin_prefetch`
- `_mm_cvtss_f32`
- `_mm_setzero_si128`
- `_mm_setzero_ps`
- `_mm_set1_ps`
- `_mm_set_ps1`
- `_mm_set_ps`
- `ALIGN_STRUCT`
- `_mm_set_ss`
- `_mm_setr_ps`
- `_mm_setr_epi16`
- `_mm_setr_epi32`
- `_mm_set1_epi8`
- `_mm_set1_epi16`
- `_mm_set_epi8`
- `_mm_set_epi16`
- `_mm_setr_epi8`
- `_mm_set1_epi32`
- `_mm_set1_epi64`
- `_mm_set1_epi64x`
- `_mm_set_epi32`
- `_mm_set_epi64x`
- `_mm_store_ps`
- `vst1q_f32`
- `_mm_storeu_ps`
- `_mm_store_si128`
- `vst1q_s32`
- `_mm_storeu_si128`
- `_mm_store_ss`
- `vst1q_lane_f32`
- `_mm_storel_epi64`
- `_mm_storel_pi`
- `_mm_storeh_pi`
- `_mm_load1_ps`
- `_mm_loadl_pi`
- `_mm_loadh_pi`
- `_mm_load_ps`
- `_mm_loadu_ps`
- `_mm_load_sd`
- `_mm_load_ss`
- `_mm_loadl_epi64`
- `_mm_cmpneq_ps`
- `_mm_andnot_ps`
- `_mm_andnot_si128`
- `_mm_and_si128`
- `_mm_and_ps`
- `_mm_or_ps`
- `_mm_xor_ps`
- `_mm_or_si128`
- `_mm_xor_si128`
- `_mm_movehl_ps`
- `_mm_movelh_ps`
- `_mm_abs_epi32`
- `_mm_abs_epi16`
- `_mm_abs_epi8`
- `_mm_shuffle_ps_1032`
- `_mm_shuffle_ps_2301`
- `_mm_shuffle_ps_0321`
- `_mm_shuffle_ps_2103`
- `_mm_shuffle_ps_1010`
- `_mm_shuffle_ps_1001`
- `_mm_shuffle_ps_0101`
- `_mm_shuffle_ps_3210`
- `_mm_shuffle_ps_0011`
- `_mm_shuffle_ps_0022`
- `vdup_lane_f32`
- `_mm_shuffle_ps_2200`
- `_mm_shuffle_ps_3202`
- `_mm_shuffle_ps_1133`
- `_mm_shuffle_ps_2010`
- `_mm_shuffle_ps_2001`
- `_mm_shuffle_ps_2032`
- `_mm_shuffle_ps_default`
- `_mm_shuffle_epi_2301`
- `_mm_shuffle_epi_0321`
- `_mm_shuffle_epi_2103`
- `_mm_shuffle_epi_1010`
- `_mm_shuffle_epi_1001`
- `_mm_shuffle_epi_0101`
- `_mm_shuffle_epi_2211`
- `_mm_shuffle_epi_0122`
- `_mm_shuffle_epi_3332`
- `_mm_shuffle_epi8`
- `vandq_u8`
- `__volatile__`
- `_mm_shuffle_epi32_default`
- `vreinterpretq_u8_s8`
- `_mm_srai_epi32`
- `_mm_srai_epi16`
- `_mm_sll_epi32`
- `_mm_sll_epi64`
- `_mm_srl_epi16`
- `_mm_srl_epi32`
- `_mm_srl_epi64`
- `_mm_movemask_epi8`
- `vshlq_u8`
- `vreinterpretq_u32_u16`
- `vreinterpretq_u64_u32`
- `vreinterpretq_u8_u64`
- `_mm_movemask_ps`
- `_mm_test_all_zeros`
- `vandq_s64`
- `_mm_sub_ps`
- `_mm_sub_epi64`
- `_mm_sub_epi32`
- `_mm_sub_epi16`
- `_mm_sub_epi8`
- `_mm_subs_epu16`
- `_mm_subs_epu8`
- `_mm_subs_epi8`
- `_mm_subs_epi16`
- `_mm_adds_epu16`
- `_mm_sign_epi8`
- `_mm_sign_epi16`
- `_mm_sign_epi32`
- `_mm_avg_epu8`
- `_mm_avg_epu16`
- `_mm_add_ps`
- `_mm_add_ss`
- `_mm_add_epi64`
- `_mm_add_epi32`
- `_mm_add_epi16`
- `_mm_add_epi8`
- `_mm_adds_epi16`
- `_mm_adds_epu8`
- `_mm_mullo_epi16`
- `_mm_mullo_epi32`
- `_mm_mul_ps`
- `_mm_mul_epu32`
- `_mm_mul_epi32`
- `_mm_madd_epi16`
- `_mm_mulhrs_epi16`
- `_mm_maddubs_epi16`
- `_mm_sad_epu8`
- `_mm_div_ps`
- `vmulq_f32`
- `_mm_div_ss`
- `vgetq_lane_f32`
- `_mm_rcp_ps`
- `_mm_sqrt_ps`
- `_mm_sqrt_ss`
- `_mm_rsqrt_ps`
- `_mm_max_ps`
- `_mm_min_ps`
- `_mm_max_ss`
- `_mm_min_ss`
- `_mm_max_epu8`
- `_mm_min_epu8`
- `_mm_min_epi16`
- `_mm_max_epi16`
- `_mm_max_epi32`
- `_mm_min_epi32`
- `_mm_mulhi_epi16`
- `vuzpq_u16`
- `_mm_hadd_ps`
- `_mm_hadd_epi16`
- `_mm_hsub_epi16`
- `_mm_hadds_epi16`
- `_mm_hsubs_epi16`
- `_mm_hadd_epi32`
- `_mm_hsub_epi32`
- `_mm_cmplt_ps`
- `_mm_cmpgt_ps`
- `_mm_cmpge_ps`
- `_mm_cmple_ps`
- `_mm_cmpeq_ps`
- `_mm_cmpeq_epi8`
- `_mm_cmpeq_epi16`
- `_mm_cmpeq_epi32`
- `_mm_cmpeq_epi64`
- `vceqq_u32`
- `_mm_cmplt_epi8`
- `_mm_cmpgt_epi8`
- `_mm_cmplt_epi16`
- `_mm_cmpgt_epi16`
- `_mm_cmplt_epi32`
- `_mm_cmpgt_epi32`
- `_mm_cmpgt_epi64`
- `_mm_cmpord_ps`
- `vceqq_f32`
- `_mm_comilt_ss`
- `vcltq_f32`
- `_mm_comigt_ss`
- `vcgtq_f32`
- `_mm_comile_ss`
- `vcleq_f32`
- `_mm_comige_ss`
- `vcgeq_f32`
- `_mm_comieq_ss`
- `_mm_comineq_ss`
- `_mm_cvttps_epi32`
- `_mm_cvtepi32_ps`
- `_mm_cvtepu8_epi16`
- `_mm_cvtepu8_epi32`
- `_mm_cvtepu8_epi64`
- `_mm_cvtepi8_epi16`
- `_mm_cvtepi8_epi32`
- `_mm_cvtepi8_epi64`
- `_mm_cvtepi16_epi32`
- `_mm_cvtepi16_epi64`
- `_mm_cvtepu16_epi32`
- `_mm_cvtepu16_epi64`
- `_mm_cvtepu32_epi64`
- `_mm_cvtepi32_epi64`
- `_mm_cvtps_epi32`
- `vcvtq_s32_f32`
- `_mm_cvtsi128_si32`
- `_mm_cvtsi128_si64`
- `_mm_cvtsi32_si128`
- `_mm_cvtsi64_si128`
- `_mm_castps_si128`
- `_mm_castsi128_ps`
- `_mm_load_si128`
- `_mm_loadu_si128`
- `_mm_sra_epi16`
- `_mm_sra_epi32`
- `_mm_packs_epi16`
- `_mm_packus_epi16`
- `_mm_packs_epi32`
- `_mm_packus_epi32`
- `_mm_unpacklo_epi8`
- `_mm_unpacklo_epi16`
- `_mm_unpacklo_epi32`
- `_mm_unpacklo_epi64`
- `_mm_unpacklo_ps`
- `_mm_unpackhi_ps`
- `_mm_unpackhi_epi8`
- `vreinterpret_s8_s16`
- `_mm_unpackhi_epi16`
- `_mm_unpackhi_epi32`
- `_mm_unpackhi_epi64`
- `_mm_minpos_epu16`
- `vst1_u32`
- `_mm_popcnt_u64`
- `vst1_u64`
- `_sse2neon_vmull_p64`
- `vreinterpretq_u8_p16`
- `_mm_clmulepi64_si128`
- `abort`
- `_mm_sfence`
- `__sync_synchronize`
- `_mm_stream_si128`
- `vst1q_s64`
- `_mm_clflush`
- `_mm_malloc`
- `_mm_free`
- `free`
- `_mm_crc32_u8`
- `_mm_crc32_u16`
- `_mm_crc32_u32`
- `_mm_crc32_u64`

### `support/viterbi-spiral/sse2neon/tests/binding.cpp`

- `platformAlignedAlloc`
- `platformAlignedFree`

### `support/viterbi-spiral/sse2neon/tests/binding.h`

- `platformAlignedAlloc`
- `platformAlignedFree`

### `support/viterbi-spiral/sse2neon/tests/impl.cpp`

- `getNAN`
- `isNAN`
- `bankersRounding`
- `SSE2NEONTest::getInstructionTestString`
- `ranf`
- `validate128`
- `validateInt64`
- `validateUInt64`
- `validateInt32`
- `validateInt16`
- `validateUInt16`
- `validateInt8`
- `validateUInt8`
- `validateSingleFloatPair`
- `validateSingleDoublePair`
- `validateFloat`
- `validateFloatEpsilon`
- `validateDouble`
- `test_mm_setzero_si128`
- `test_mm_setzero_ps`
- `test_mm_set1_ps`
- `test_mm_set_ps`
- `test_mm_set_ss`
- `test_mm_set_epi8`
- `test_mm_set1_epi32`
- `testret_mm_set_epi32`
- `test_mm_set_epi32`
- `test_mm_store_ps`
- `test_mm_storel_pi`
- `test_mm_storeh_pi`
- `test_mm_load1_ps`
- `test_mm_loadl_pi`
- `test_mm_loadh_pi`
- `test_mm_load_ps`
- `test_mm_load_sd`
- `test_mm_andnot_ps`
- `test_mm_and_ps`
- `test_mm_or_ps`
- `test_mm_andnot_si128`
- `test_mm_and_si128`
- `test_mm_or_si128`
- `test_mm_movemask_ps`
- `test_mm_shuffle_ps`
- `test_mm_movemask_epi8`
- `test_mm_sub_ps`
- `test_mm_sub_epi32`
- `test_mm_sub_epi64`
- `test_mm_add_ps`
- `test_mm_add_epi32`
- `test_mm_mullo_epi16`
- `test_mm_mul_epu32`
- `test_mm_madd_epi16`
- `saturate_16`
- `test_mm_maddubs_epi16`
- `test_mm_shuffle_epi8`
- `test_mm_mul_ps`
- `test_mm_rcp_ps`
- `test_mm_max_ps`
- `test_mm_min_ps`
- `test_mm_min_epi16`
- `test_mm_mulhi_epi16`
- `test_mm_cmplt_ps`
- `test_mm_cmpgt_ps`
- `test_mm_cmpge_ps`
- `test_mm_cmple_ps`
- `test_mm_cmpeq_ps`
- `test_mm_cmplt_epi32`
- `test_mm_cmpgt_epi32`
- `compord`
- `test_mm_cmpord_ps`
- `comilt_ss`
- `test_mm_comilt_ss`
- `comigt_ss`
- `test_mm_comigt_ss`
- `comile_ss`
- `test_mm_comile_ss`
- `comige_ss`
- `test_mm_comige_ss`
- `comieq_ss`
- `test_mm_comieq_ss`
- `comineq_ss`
- `test_mm_comineq_ss`
- `test_mm_hadd_epi16`
- `test_mm_cvttps_epi32`
- `test_mm_cvtepi32_ps`
- `test_mm_cvtps_epi32`
- `test_mm_set1_epi16`
- `test_mm_set_epi16`
- `test_mm_sra_epi16`
- `test_mm_sra_epi32`
- `test_mm_slli_epi16`
- `test_mm_sll_epi16`
- `test_mm_sll_epi32`
- `test_mm_sll_epi64`
- `test_mm_srl_epi16`
- `test_mm_srl_epi32`
- `test_mm_srl_epi64`
- `test_mm_srli_epi16`
- `test_mm_cmpeq_epi16`
- `test_mm_cmpeq_epi64`
- `test_mm_set1_epi8`
- `test_mm_adds_epu8`
- `test_mm_subs_epu8`
- `test_mm_max_epu8`
- `test_mm_cmpeq_epi8`
- `test_mm_adds_epi16`
- `test_mm_max_epi16`
- `test_mm_subs_epu16`
- `test_mm_cmplt_epi16`
- `test_mm_cmpgt_epi16`
- `test_mm_loadu_si128`
- `test_mm_storeu_si128`
- `test_mm_add_epi8`
- `test_mm_cmpgt_epi8`
- `test_mm_cmplt_epi8`
- `test_mm_sub_epi8`
- `test_mm_setr_epi32`
- `test_mm_min_epu8`
- `test_mm_minpos_epu16`
- `test_mm_test_all_zeros`
- `test_mm_avg_epu8`
- `test_mm_avg_epu16`
- `test_mm_popcnt_u32`
- `test_mm_popcnt_u64`
- `MUL`
- `clmul_32`
- `clmul_64`
- `test_mm_clmulepi64_si128`
- `test_mm_malloc`
- `canonical_crc32_u8`
- `canonical_crc32_u16`
- `canonical_crc32_u32`
- `canonical_crc32_u64`
- `test_mm_crc32_u8`
- `test_mm_crc32_u16`
- `test_mm_crc32_u32`
- `test_mm_crc32_u64`
- `~SSE2NEONTestImpl`
- `loadTestFloatPointers`
- `loadTestIntPointers`
- `runSingleTest`
- `runTest`
- `SSE2NEONTest::create`

### `support/viterbi-spiral/sse2neon/tests/impl.h`

- `create`
- `getInstructionTestString`
- `runTest`
- `release`

### `support/viterbi-spiral/sse2neon/tests/main.cpp`

- `main`

### `support/viterbi-spiral/sse2neon.h`

- `_mm_prefetch`
- `__builtin_prefetch`
- `_mm_cvtss_f32`
- `_mm_setzero_si128`
- `_mm_setzero_ps`
- `_mm_set1_ps`
- `_mm_set_ps1`
- `_mm_set_ps`
- `ALIGN_STRUCT`
- `_mm_set_ss`
- `_mm_setr_ps`
- `_mm_setr_epi16`
- `_mm_setr_epi32`
- `_mm_set1_epi8`
- `_mm_set1_epi16`
- `_mm_set_epi8`
- `_mm_set_epi16`
- `_mm_setr_epi8`
- `_mm_set1_epi32`
- `_mm_set1_epi64`
- `_mm_set1_epi64x`
- `_mm_set_epi32`
- `_mm_set_epi64x`
- `_mm_store_ps`
- `vst1q_f32`
- `_mm_storeu_ps`
- `_mm_store_si128`
- `vst1q_s32`
- `_mm_storeu_si128`
- `_mm_store_ss`
- `vst1q_lane_f32`
- `_mm_storel_epi64`
- `_mm_storel_pi`
- `_mm_storeh_pi`
- `_mm_load1_ps`
- `_mm_loadl_pi`
- `_mm_loadh_pi`
- `_mm_load_ps`
- `_mm_loadu_ps`
- `_mm_load_sd`
- `_mm_load_ss`
- `_mm_loadl_epi64`
- `_mm_cmpneq_ps`
- `_mm_andnot_ps`
- `_mm_andnot_si128`
- `_mm_and_si128`
- `_mm_and_ps`
- `_mm_or_ps`
- `_mm_xor_ps`
- `_mm_or_si128`
- `_mm_xor_si128`
- `_mm_movehl_ps`
- `_mm_movelh_ps`
- `_mm_abs_epi32`
- `_mm_abs_epi16`
- `_mm_abs_epi8`
- `_mm_shuffle_ps_1032`
- `_mm_shuffle_ps_2301`
- `_mm_shuffle_ps_0321`
- `_mm_shuffle_ps_2103`
- `_mm_shuffle_ps_1010`
- `_mm_shuffle_ps_1001`
- `_mm_shuffle_ps_0101`
- `_mm_shuffle_ps_3210`
- `_mm_shuffle_ps_0011`
- `_mm_shuffle_ps_0022`
- `vdup_lane_f32`
- `_mm_shuffle_ps_2200`
- `_mm_shuffle_ps_3202`
- `_mm_shuffle_ps_1133`
- `_mm_shuffle_ps_2010`
- `_mm_shuffle_ps_2001`
- `_mm_shuffle_ps_2032`
- `_mm_shuffle_ps_default`
- `_mm_shuffle_epi_2301`
- `_mm_shuffle_epi_0321`
- `_mm_shuffle_epi_2103`
- `_mm_shuffle_epi_1010`
- `_mm_shuffle_epi_1001`
- `_mm_shuffle_epi_0101`
- `_mm_shuffle_epi_2211`
- `_mm_shuffle_epi_0122`
- `_mm_shuffle_epi_3332`
- `_mm_shuffle_epi8`
- `vandq_u8`
- `__volatile__`
- `_mm_shuffle_epi32_default`
- `vreinterpretq_u8_s8`
- `_mm_srai_epi32`
- `_mm_srai_epi16`
- `_mm_sll_epi32`
- `_mm_sll_epi64`
- `_mm_srl_epi16`
- `_mm_srl_epi32`
- `_mm_srl_epi64`
- `_mm_movemask_epi8`
- `vshlq_u8`
- `vreinterpretq_u32_u16`
- `vreinterpretq_u64_u32`
- `vreinterpretq_u8_u64`
- `_mm_movemask_ps`
- `_mm_test_all_zeros`
- `vandq_s64`
- `_mm_sub_ps`
- `_mm_sub_epi64`
- `_mm_sub_epi32`
- `_mm_sub_epi16`
- `_mm_sub_epi8`
- `_mm_subs_epu16`
- `_mm_subs_epu8`
- `_mm_subs_epi8`
- `_mm_subs_epi16`
- `_mm_adds_epu16`
- `_mm_sign_epi8`
- `_mm_sign_epi16`
- `_mm_sign_epi32`
- `_mm_avg_epu8`
- `_mm_avg_epu16`
- `_mm_add_ps`
- `_mm_add_ss`
- `_mm_add_epi64`
- `_mm_add_epi32`
- `_mm_add_epi16`
- `_mm_add_epi8`
- `_mm_adds_epi16`
- `_mm_adds_epu8`
- `_mm_mullo_epi16`
- `_mm_mullo_epi32`
- `_mm_mul_ps`
- `_mm_mul_epu32`
- `_mm_mul_epi32`
- `_mm_madd_epi16`
- `_mm_mulhrs_epi16`
- `_mm_maddubs_epi16`
- `_mm_sad_epu8`
- `_mm_div_ps`
- `vmulq_f32`
- `_mm_div_ss`
- `vgetq_lane_f32`
- `_mm_rcp_ps`
- `_mm_sqrt_ps`
- `_mm_sqrt_ss`
- `_mm_rsqrt_ps`
- `_mm_max_ps`
- `_mm_min_ps`
- `_mm_max_ss`
- `_mm_min_ss`
- `_mm_max_epu8`
- `_mm_min_epu8`
- `_mm_min_epi16`
- `_mm_max_epi16`
- `_mm_max_epi32`
- `_mm_min_epi32`
- `_mm_mulhi_epi16`
- `vuzpq_u16`
- `_mm_hadd_ps`
- `_mm_hadd_epi16`
- `_mm_hsub_epi16`
- `_mm_hadds_epi16`
- `_mm_hsubs_epi16`
- `_mm_hadd_epi32`
- `_mm_hsub_epi32`
- `_mm_cmplt_ps`
- `_mm_cmpgt_ps`
- `_mm_cmpge_ps`
- `_mm_cmple_ps`
- `_mm_cmpeq_ps`
- `_mm_cmpeq_epi8`
- `_mm_cmpeq_epi16`
- `_mm_cmpeq_epi32`
- `_mm_cmpeq_epi64`
- `vceqq_u32`
- `_mm_cmplt_epi8`
- `_mm_cmpgt_epi8`
- `_mm_cmplt_epi16`
- `_mm_cmpgt_epi16`
- `_mm_cmplt_epi32`
- `_mm_cmpgt_epi32`
- `_mm_cmpgt_epi64`
- `_mm_cmpord_ps`
- `vceqq_f32`
- `_mm_comilt_ss`
- `vcltq_f32`
- `_mm_comigt_ss`
- `vcgtq_f32`
- `_mm_comile_ss`
- `vcleq_f32`
- `_mm_comige_ss`
- `vcgeq_f32`
- `_mm_comieq_ss`
- `_mm_comineq_ss`
- `_mm_cvttps_epi32`
- `_mm_cvtepi32_ps`
- `_mm_cvtepu8_epi16`
- `_mm_cvtepu8_epi32`
- `_mm_cvtepu8_epi64`
- `_mm_cvtepi8_epi16`
- `_mm_cvtepi8_epi32`
- `_mm_cvtepi8_epi64`
- `_mm_cvtepi16_epi32`
- `_mm_cvtepi16_epi64`
- `_mm_cvtepu16_epi32`
- `_mm_cvtepu16_epi64`
- `_mm_cvtepu32_epi64`
- `_mm_cvtepi32_epi64`
- `_mm_cvtps_epi32`
- `vcvtq_s32_f32`
- `_mm_cvtsi128_si32`
- `_mm_cvtsi128_si64`
- `_mm_cvtsi32_si128`
- `_mm_cvtsi64_si128`
- `_mm_castps_si128`
- `_mm_castsi128_ps`
- `_mm_load_si128`
- `_mm_loadu_si128`
- `_mm_sra_epi16`
- `_mm_sra_epi32`
- `_mm_packs_epi16`
- `_mm_packus_epi16`
- `_mm_packs_epi32`
- `_mm_packus_epi32`
- `_mm_unpacklo_epi8`
- `_mm_unpacklo_epi16`
- `_mm_unpacklo_epi32`
- `_mm_unpacklo_epi64`
- `_mm_unpacklo_ps`
- `_mm_unpackhi_ps`
- `_mm_unpackhi_epi8`
- `vreinterpret_s8_s16`
- `_mm_unpackhi_epi16`
- `_mm_unpackhi_epi32`
- `_mm_unpackhi_epi64`
- `_mm_minpos_epu16`
- `vst1_u32`
- `_mm_popcnt_u64`
- `vst1_u64`
- `_sse2neon_vmull_p64`
- `vreinterpretq_u8_p16`
- `_mm_clmulepi64_si128`
- `abort`
- `_mm_sfence`
- `__sync_synchronize`
- `_mm_stream_si128`
- `vst1q_s64`
- `_mm_clflush`
- `_mm_malloc`
- `_mm_free`
- `free`
- `_mm_crc32_u8`
- `_mm_crc32_u16`
- `_mm_crc32_u32`
- `_mm_crc32_u64`

### `support/viterbi-spiral/viterbi-spiral.cpp`

- `ViterbiSpiral::ViterbiSpiral`
- `ViterbiSpiral::~ViterbiSpiral`
- `ViterbiSpiral::parity`
- `ViterbiSpiral::deconvolve`
- `ViterbiSpiral::calculate_BER`

### `support/viterbi-spiral/viterbi-spiral.h`

- `ViterbiSpiral`
- `~ViterbiSpiral`
- `deconvolve`
- `calculate_BER`
- `parity`

### `support/viterbi-spiral/viterbi_16way.h`

- `BFLY`

### `support/viterbi-spiral/viterbi_8way.h`

- `BFLY`

### `support/viterbi-spiral/viterbi_scalar.h`

- `limit_min_max`
- `BFLY`

### `support/wav_writer.cpp`

- `WavWriter::init`
- `WavWriter::close`
- `WavWriter::write`

### `support/wav_writer.h`

- `WavWriter`
- `~WavWriter`
- `init`
- `write`
- `close`

### `update/appversion.h`

- `AppVersion`
- `verRe`
- `qCritical`
- `exit`
- `operator==`
- `toUInt64`

### `update/updatechecker.cpp`

- `UpdateChecker::~UpdateChecker`
- `UpdateChecker::check`

### `update/updatechecker.h`

- `UpdateChecker`
- `~UpdateChecker`
- `check`
- `version`
- `isPreRelease`
- `releaseNotes`
- `finished`
- `onFileDownloaded`
- `parseResponse`

### `update/updatedialog.cpp`

- `UpdateDialog::UpdateDialog`

### `update/updatedialog.h`

- `UpdateDialog`
- `~UpdateDialog`

