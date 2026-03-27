// ==============================================================================
// lib.rs - ETI-RTL-SDR Rust Library
// ==============================================================================
// Architecture refactorisée avec separation des responsabilités et traits idiomatiques

// Core modules
pub mod callbacks;
pub mod errors;
pub mod types;

// Processing modules
pub mod support;
pub mod rtlsdr_sys;
pub mod ofdm;
pub mod eti_handling;
pub mod iq;

// Pipeline and CLI
pub mod eti_pipeline;
pub mod cli;

// Re-export from old ofdm_processor for backward compatibility
pub use ofdm::ofdm_processor;

pub use support::percentile;
pub use ofdm::ofdm_processor as pipeline;
pub use eti_handling::viterbi_handler as viterbi;

// Re-export pipeline items for tests
pub use pipeline::{
    DabPipeline, PipelineMode, SyncCandidate, PipelineReport,
    DabOfdmSymbol, DabFrameCandidate,
    DabFrequencySymbol, DabFrequencyFrameCandidate,
    DabMappedSymbol, DabMappedFrameCandidate,
    DabNormalizedFrame,
    FicCandidate, FicBitstreamCandidate, FicDeinterleavedCandidate,
    FicSegmentCandidate, FicBlockCandidate,
    FibCandidate, FigCandidate, FigDetails, FigType0Details,
    SignallingSnapshot, Type0ExtensionSummary, MultiplexState,
    SubChannelProtection, DabSubChannel, DabServiceComponent, DabService,
    Fig0Decoded, DabEnsembleInfo,
    EtiFrameBuilder,
    FrameAligner, FrameBuilder, FrequencyFrameTransformer, 
    CarrierMapper, FrameNormalizer, FicDemapper, FicPreDecoder,
    FibExtractor, SignallingDecoder, OfdmSyncDetector,
    dab_mode_i_phase_reference_mapped,
    iq_bytes_to_complex, qpsk_hard_demapp, crc16_matches, crc16_ccitt_false,
    crc16_ccitt_bytes, bits_to_u16, bits_to_bytes, decode_fig0,
    DAB_MODE_I_FFT_LEN, DAB_MODE_I_CP_LEN, DAB_MODE_I_SYMBOLS_PER_FRAME,
    DAB_MODE_I_ACTIVE_CARRIERS, DAB_FIC_SYMBOL_COUNT, DAB_MSC_SYMBOL_COUNT,
    DAB_FIB_BITS, DAB_FIB_BYTES, ETI_FRAME_BYTES, ETI_FIC_BYTES,
    ETI_SYNC_EVEN, ETI_SYNC_ODD,
};
