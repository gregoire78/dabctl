use anyhow::Result;
use rustfft::num_complex::Complex32;
use rustfft::{Fft, FftPlanner};
use std::sync::Arc;
use crate::eti_handling::cif_interleaver::CifInterleaver;
use crate::eti_handling::eti_generator::extract_msc_payload_from_normalized_frame;

pub use crate::ofdm::sync_processor::{apply_cfo_correction, iq_bytes_to_complex};

pub const DAB_MODE_I_FFT_LEN: usize = 2048;
pub const DAB_MODE_I_CP_LEN: usize = 504;
pub const DAB_MODE_I_SYMBOLS_PER_FRAME: usize = 76;
pub const DAB_MODE_I_ACTIVE_CARRIERS: usize = 1536;
pub const DAB_FIC_SYMBOL_COUNT: usize = 3;
pub const DAB_MSC_SYMBOL_COUNT: usize = 72;
const DAB_EXPERIMENTAL_FIC_SEGMENT_BITS: usize = 256;
pub const DAB_FIB_BITS: usize = 256;
pub const DAB_FIB_BYTES: usize = 32;
const IQ_BYTES_PER_SAMPLE: usize = 2;
const DEFAULT_SYNC_THRESHOLD: f32 = 0.72;
const MAX_PENDING_SYMBOLS: usize = 6;
pub const ETI_FRAME_BYTES: usize = 6144;
pub const ETI_FIC_BYTES: usize = 96; // 3 FIBs × 32 octets en mode I
pub const ETI_SYNC_EVEN: [u8; 4] = [0x00, 0x49, 0xC5, 0xF8]; // ERR=0 + FSYNC trames paires
pub const ETI_SYNC_ODD: [u8; 4] = [0x00, 0xB6, 0x3A, 0x07]; // ERR=0 + FSYNC trames impaires

const DAB_PHASE_REFERENCE_H: [[u8; 32]; 4] = [
    [0, 2, 0, 0, 0, 0, 1, 1, 2, 0, 0, 0, 2, 2, 1, 1, 0, 2, 0, 0, 0, 0, 1, 1, 2, 0, 0, 0, 2, 2, 1, 1],
    [0, 3, 2, 3, 0, 1, 3, 0, 2, 1, 2, 3, 2, 3, 3, 0, 0, 3, 2, 3, 0, 1, 3, 0, 2, 1, 2, 3, 2, 3, 3, 0],
    [0, 0, 0, 2, 0, 2, 1, 3, 2, 2, 0, 2, 2, 0, 1, 3, 0, 0, 0, 2, 0, 2, 1, 3, 2, 2, 0, 2, 2, 0, 1, 3],
    [0, 1, 2, 1, 0, 3, 3, 2, 2, 3, 2, 1, 2, 1, 3, 2, 0, 1, 2, 1, 0, 3, 3, 2, 2, 3, 2, 1, 2, 1, 3, 2],
];

const DAB_MODE_I_PHASE_REFERENCE_POSITIVE: [(usize, u8); 24] = [
    (0, 3), (3, 1), (2, 1), (1, 1), (0, 2), (3, 2),
    (2, 1), (1, 0), (0, 2), (3, 2), (2, 3), (1, 3),
    (0, 0), (3, 2), (2, 1), (1, 3), (0, 3), (3, 3),
    (2, 3), (1, 0), (0, 3), (3, 0), (2, 1), (1, 1),
];

const DAB_MODE_I_PHASE_REFERENCE_NEGATIVE: [(usize, u8); 24] = [
    (0, 1), (1, 2), (2, 0), (3, 1), (0, 3), (1, 2),
    (2, 2), (3, 3), (0, 2), (1, 1), (2, 2), (3, 3),
    (0, 1), (1, 2), (2, 3), (3, 3), (0, 2), (1, 2),
    (2, 2), (3, 1), (0, 1), (1, 3), (2, 1), (3, 2),
];

#[derive(Debug, Clone)]
pub struct DabOfdmSymbol {
    pub start_sample: usize,
    pub cfo_phase_per_sample: f32,
    pub iq_fft_only: Vec<u8>,
}

#[derive(Debug, Clone)]
pub struct DabFrameCandidate {
    pub start_sample: usize,
    pub symbol_count: usize,
    pub symbols: Vec<DabOfdmSymbol>,
}

#[derive(Debug, Clone)]
pub struct DabFrequencySymbol {
    pub start_sample: usize,
    pub carriers: Vec<Complex32>,
}

#[derive(Debug, Clone)]
pub struct DabFrequencyFrameCandidate {
    pub start_sample: usize,
    pub symbol_count: usize,
    pub symbols: Vec<DabFrequencySymbol>,
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct DabMappedSymbol {
    pub start_sample: usize,
    pub carriers: Vec<Complex32>,
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct DabMappedFrameCandidate {
    pub start_sample: usize,
    pub symbol_count: usize,
    pub symbols: Vec<DabMappedSymbol>,
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct DabNormalizedFrame {
    pub start_sample: usize,
    pub phase_reference: DabMappedSymbol,
    pub fic_symbols: Vec<DabMappedSymbol>,
    pub msc_symbols: Vec<DabMappedSymbol>,
    /// Per-carrier channel gain weights (normalized to mean=1, capped at 2).
    /// Used to weight FIC soft bits: weak carriers contribute less to Viterbi.
    pub channel_gains: Vec<f32>,
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct FicCandidate {
    pub frame_start_sample: usize,
    pub symbol_count: usize,
    pub carriers_per_symbol: usize,
}

#[derive(Debug, Clone)]
pub struct FicBitstreamCandidate {
    pub frame_start_sample: usize,
    pub bit_count: usize,
    pub bits: Vec<u8>,
    pub soft_bits: Vec<i16>,
}

#[derive(Debug, Clone)]
pub struct FicDeinterleavedCandidate {
    pub frame_start_sample: usize,
    pub bits: Vec<u8>,
    pub soft_bits: Vec<i16>,
}

#[derive(Debug, Clone)]
pub struct FicSegmentCandidate {
    pub frame_start_sample: usize,
    pub segment_index: usize,
    pub bit_count: usize,
    pub bits: Vec<u8>,
}

#[derive(Debug, Clone)]
pub struct FicBlockCandidate {
    pub frame_start_sample: usize,
    pub block_index: usize,
    pub bit_count: usize,
    pub crc_ok: bool,
    pub bits: Vec<u8>,
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct FibCandidate {
    pub frame_start_sample: usize,
    pub block_index: usize,
    pub crc_ok: bool,
    pub bytes: Vec<u8>,
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct FigCandidate {
    pub frame_start_sample: usize,
    pub block_index: usize,
    pub fig_type: u8,
    pub extension: Option<u8>,
    pub payload_len: usize,
    pub payload: Vec<u8>,
    pub details: FigDetails,
}

#[derive(Debug, Clone)]
pub enum FigDetails {
    Type0(FigType0Details),
    Raw,
}

#[derive(Debug, Clone)]
pub struct FigType0Details {
    pub cn: bool,
    pub oe: bool,
    pub pd: bool,
    pub extension: u8,
    pub body: Vec<u8>,
}

/// Informations d'ensemble issues de FIG 0/0 (ETSI EN 300 401 §6.2)
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DabEnsembleInfo {
    pub eid: u16,
    pub change_flags: u8,
    pub al: bool,
    pub cif_count_high: u8,
    pub cif_count_low: u8,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SubChannelProtection {
    Short { table_switch: bool, table_index: u8 },
    Long { protection_level: u8, protection_type: bool, size_cu: u16 },
}

/// Descripteur de sous-canal issu de FIG 0/1
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DabSubChannel {
    pub id: u8,
    pub start_address: u16,
    pub protection: SubChannelProtection,
}

/// Composant de service issu de FIG 0/2
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DabServiceComponent {
    pub tm_id: u8,
    pub type_id: u8,
    pub sub_ch_id: u8,
    pub primary: bool,
    pub ca: bool,
}

/// Service issu de FIG 0/2 (ETSI EN 300 401 §8.1.14.2)
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DabService {
    pub sid: u32,
    pub ca_id: u8,
    pub components: Vec<DabServiceComponent>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Fig0Decoded {
    EnsembleInfo(DabEnsembleInfo),
    SubChannels(Vec<DabSubChannel>),
    Services(Vec<DabService>),
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SignallingSnapshot {
    pub fig_count: usize,
    pub type1_count: usize,
    pub type0_extensions: Vec<Type0ExtensionSummary>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Type0ExtensionSummary {
    pub extension: u8,
    pub count: usize,
    pub cn: bool,
    pub oe: bool,
    pub pd: bool,
    pub last_body: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MultiplexState {
    pub updates: usize,
    pub total_fig_count: usize,
    pub total_type1_count: usize,
    pub last_frame_start_sample: Option<usize>,
    pub type0_extensions: Vec<Type0ExtensionSummary>,
    pub ensemble_info: Option<DabEnsembleInfo>,
    pub sub_channels: Vec<DabSubChannel>,
    pub services: Vec<DabService>,
}

#[derive(Debug, Clone, Copy)]
pub struct SyncCandidate {
    pub sample_offset: usize,
    pub metric: f32,
    pub cfo_phase_per_sample: f32,
}

#[derive(Debug, Clone)]
pub struct PipelineReport {
    pub sync_candidate: Option<SyncCandidate>,
    pub sync_cfo_phase_per_sample: Option<f32>,
    pub prs_eq_mse: Option<f32>,
    pub prs_eq_phase_rms_rad: Option<f32>,
    pub prs_channel_gain_avg: Option<f32>,
    pub prs_channel_gain_spread_db: Option<f32>,
    pub inspected_samples: usize,
    pub aligned_symbols: usize,
    pub last_symbol_start: Option<usize>,
    pub completed_frames: usize,
    pub last_frame_start: Option<usize>,
    pub frequency_frames: usize,
    pub last_frequency_frame_start: Option<usize>,
    pub mapped_frames: usize,
    pub last_mapped_frame_start: Option<usize>,
    pub normalized_frames: usize,
    pub fic_candidates: usize,
    pub fic_bitstreams: usize,
    pub last_fic_bit_count: Option<usize>,
    pub fic_deinterleaved: usize,
    pub fic_segments: usize,
    pub fic_blocks: usize,
    pub fic_crc_ok: usize,
    pub fib_candidates: usize,
    pub fig_candidates: usize,
    pub fig_type0: usize,
    pub fig_type1: usize,
    pub fig_type0_unique_extensions: usize,
    pub last_fig0_extension: Option<u8>,
    pub multiplex_updates: usize,
    pub eti_frames_built: usize,
    pub eti_frames_emitted: usize,
    pub eti_fic_cache_valid: bool,
}

#[derive(Debug, Clone, Copy)]
pub enum PipelineMode {
    RawIq,
    /// Émet des trames ETI-NI de 6144 octets (expérimental)
    Eti,
}

pub struct DabPipeline {
    mode: PipelineMode,
    sync_detector: OfdmSyncDetector,
    frame_aligner: FrameAligner,
    frame_builder: FrameBuilder,
    frequency_transformer: FrequencyFrameTransformer,
    carrier_mapper: CarrierMapper,
    frame_normalizer: FrameNormalizer,
    fic_demapper: FicDemapper,
    fic_predecoder: FicPreDecoder,
    fib_extractor: FibExtractor,
    signalling_decoder: SignallingDecoder,
    eti_builder: EtiFrameBuilder,
    eti_frames_built: usize,
    eti_frames_emitted: usize,
    eti_fic_cache: Vec<u8>,
    eti_fic_cache_valid: bool,
    eti_msc_cache: Vec<u8>,
    eti_msc_cache_valid: bool,
    eti_cif_interleaver: CifInterleaver,
    last_report: PipelineReport,
    last_signalling: Option<SignallingSnapshot>,
    last_eti_frame: Vec<u8>,
    multiplex_state: MultiplexState,
}

impl DabPipeline {
    pub fn new(mode: PipelineMode) -> Self {
        Self {
            mode,
            sync_detector: OfdmSyncDetector::new(
                DAB_MODE_I_FFT_LEN,
                DAB_MODE_I_CP_LEN,
                DEFAULT_SYNC_THRESHOLD,
            ),
            frame_aligner: FrameAligner::new(DAB_MODE_I_FFT_LEN, DAB_MODE_I_CP_LEN),
            frame_builder: FrameBuilder::new(DAB_MODE_I_CP_LEN + DAB_MODE_I_FFT_LEN),
            frequency_transformer: FrequencyFrameTransformer::new(DAB_MODE_I_FFT_LEN),
            carrier_mapper: CarrierMapper::new(DAB_MODE_I_FFT_LEN, DAB_MODE_I_ACTIVE_CARRIERS),
            frame_normalizer: FrameNormalizer::new(),
            fic_demapper: FicDemapper::new(),
            fic_predecoder: FicPreDecoder::new(DAB_FIC_SYMBOL_COUNT, DAB_EXPERIMENTAL_FIC_SEGMENT_BITS),
            fib_extractor: FibExtractor::new(),
            signalling_decoder: SignallingDecoder::new(),
            eti_builder: EtiFrameBuilder::new(),
            eti_frames_built: 0,
            eti_frames_emitted: 0,
            eti_fic_cache: vec![0u8; ETI_FIC_BYTES],
            eti_fic_cache_valid: false,
            eti_msc_cache: Vec::new(),
            eti_msc_cache_valid: false,
            eti_cif_interleaver: CifInterleaver::new(),
            last_report: PipelineReport {
                sync_candidate: None,
                sync_cfo_phase_per_sample: None,
                prs_eq_mse: None,
                prs_eq_phase_rms_rad: None,
                prs_channel_gain_avg: None,
                prs_channel_gain_spread_db: None,
                inspected_samples: 0,
                aligned_symbols: 0,
                last_symbol_start: None,
                completed_frames: 0,
                last_frame_start: None,
                frequency_frames: 0,
                last_frequency_frame_start: None,
                mapped_frames: 0,
                last_mapped_frame_start: None,
                normalized_frames: 0,
                fic_candidates: 0,
                fic_bitstreams: 0,
                last_fic_bit_count: None,
                fic_deinterleaved: 0,
                fic_segments: 0,
                fic_blocks: 0,
                fic_crc_ok: 0,
                fib_candidates: 0,
                fig_candidates: 0,
                fig_type0: 0,
                fig_type1: 0,
                fig_type0_unique_extensions: 0,
                last_fig0_extension: None,
                multiplex_updates: 0,
                eti_frames_built: 0,
                eti_frames_emitted: 0,
                eti_fic_cache_valid: false,
            },
            last_signalling: None,
            last_eti_frame: Vec::new(),
            multiplex_state: MultiplexState::new(),
        }
    }

    pub fn process_chunk(&mut self, iq_chunk: &[u8], out: &mut Vec<u8>) -> Result<()> {
        let mut report = self.sync_detector.inspect(iq_chunk);
        let symbols = self
            .frame_aligner
            .push_chunk(iq_chunk, report.sync_candidate);
        let frames = self.frame_builder.push_symbols(symbols.clone());
        let frequency_frames = self.frequency_transformer.transform_frames(&frames);
        let mapped_frames = self.carrier_mapper.map_frames(&frequency_frames);
        let normalized_frames = self.frame_normalizer.normalize_frames(&mapped_frames);
        let prs_quality = mapped_frames
            .last()
            .and_then(|frame| self.frame_normalizer.analyze_phase_reference(frame));
        let fic_candidates = self.frame_normalizer.extract_fic_candidates(&normalized_frames);
        let fic_bitstreams = self.fic_demapper.demapp_candidates(&normalized_frames);
        let fic_deinterleaved = self.fic_predecoder.deinterleave_candidates(&fic_bitstreams);
        let fic_segments = self.fic_predecoder.segment_candidates(&fic_deinterleaved);
        let fic_blocks = self.fic_predecoder.build_blocks(&fic_segments);
        let fib_candidates = self.fib_extractor.extract_fibs(&fic_blocks);
        let fig_candidates = self.fib_extractor.extract_figs(&fib_candidates);
        let signalling = self.signalling_decoder.decode(&fig_candidates);
        if let Some(snapshot) = signalling.as_ref() {
            self.multiplex_state
                .update(snapshot, normalized_frames.last().map(|frame| frame.start_sample));
        }

        report.aligned_symbols = symbols.len();
        report.sync_cfo_phase_per_sample = report.sync_candidate.map(|sync| sync.cfo_phase_per_sample);
        report.prs_eq_mse = prs_quality.map(|quality| quality.eq_mse);
        report.prs_eq_phase_rms_rad = prs_quality.map(|quality| quality.eq_phase_rms_rad);
        report.prs_channel_gain_avg = prs_quality.map(|quality| quality.channel_gain_avg);
        report.prs_channel_gain_spread_db = prs_quality.map(|quality| quality.channel_gain_spread_db);
        report.last_symbol_start = symbols.last().map(|s| s.start_sample);
        report.completed_frames = frames.len();
        report.last_frame_start = frames.last().map(|f| f.start_sample);
        report.frequency_frames = frequency_frames.len();
        report.last_frequency_frame_start = frequency_frames.last().map(|f| f.start_sample);
        report.mapped_frames = mapped_frames.len();
        report.last_mapped_frame_start = mapped_frames.last().map(|f| f.start_sample);
        report.normalized_frames = normalized_frames.len();
        report.fic_candidates = fic_candidates.len();
        report.fic_bitstreams = fic_bitstreams.len();
        report.last_fic_bit_count = fic_bitstreams.last().map(|bits| bits.bit_count);
        report.fic_deinterleaved = fic_deinterleaved.len();
        report.fic_segments = fic_segments.len();
        report.fic_blocks = fic_blocks.len();
        report.fic_crc_ok = fic_blocks.iter().filter(|block| block.crc_ok).count();
        report.fib_candidates = fib_candidates.len();
        report.fig_candidates = fig_candidates.len();
        report.fig_type0 = fig_candidates.iter().filter(|fig| fig.fig_type == 0).count();
        report.fig_type1 = fig_candidates.iter().filter(|fig| fig.fig_type == 1).count();
        report.fig_type0_unique_extensions = signalling
            .as_ref()
            .map(|snapshot| snapshot.type0_extensions.len())
            .unwrap_or(0);
        report.last_fig0_extension = fig_candidates.iter().rev().find_map(|fig| match &fig.details {
            FigDetails::Type0(details) => Some(details.extension),
            FigDetails::Raw => None,
        });
        report.multiplex_updates = self.multiplex_state.updates;

        // Construire une trame ETI à partir des FIBs disponibles ce cycle
        if !fib_candidates.is_empty() {
            self.eti_fic_cache.clear();
            self.eti_fic_cache.reserve(ETI_FIC_BYTES);
            for fib in fib_candidates.iter().take(3) {
                self.eti_fic_cache.extend_from_slice(&fib.bytes);
            }
            self.eti_fic_cache.resize(ETI_FIC_BYTES, 0);
            self.eti_fic_cache_valid = true;
            self.eti_frames_built += 1;
        }

        if let Some(frame) = normalized_frames.last() {
            let raw_payload = extract_msc_payload_from_normalized_frame(frame);
            if let Some(interleaved) = self.eti_cif_interleaver.push_and_interleave(&raw_payload) {
                self.eti_msc_cache = interleaved;
                self.eti_msc_cache_valid = !self.eti_msc_cache.is_empty();
            } else {
                self.eti_msc_cache_valid = false;
            }
        }

        let fic_for_eti: Option<&[u8]> = if self.eti_fic_cache_valid {
            Some(self.eti_fic_cache.as_slice())
        } else {
            None
        };

        let msc_for_eti: Option<&[u8]> = if self.eti_msc_cache_valid {
            Some(self.eti_msc_cache.as_slice())
        } else {
            None
        };

        self.last_eti_frame = self
            .eti_builder
            .build_frame_with_msc(&self.multiplex_state, fic_for_eti, msc_for_eti);

        report.eti_frames_built = self.eti_frames_built;
        report.eti_frames_emitted = self.eti_frames_emitted;
        report.eti_fic_cache_valid = self.eti_fic_cache_valid;
        self.last_report = report;
        self.last_signalling = signalling;

        out.clear();
        match self.mode {
            PipelineMode::RawIq => out.extend_from_slice(iq_chunk),
            PipelineMode::Eti => {
                out.extend_from_slice(&self.last_eti_frame);
                self.eti_frames_emitted += 1;
                self.last_report.eti_frames_emitted = self.eti_frames_emitted;
            }
        }
        Ok(())
    }

    pub fn last_report(&self) -> &PipelineReport {
        &self.last_report
    }

    pub fn last_signalling(&self) -> Option<&SignallingSnapshot> {
        self.last_signalling.as_ref()
    }

    pub fn multiplex_state(&self) -> &MultiplexState {
        &self.multiplex_state
    }

}

pub struct FrequencyFrameTransformer {
    fft_len: usize,
    fft: Arc<dyn Fft<f32>>,
}

pub struct CarrierMapper {
    fft_len: usize,
    active_carriers: usize,
    half_active: usize,
}

pub struct FrameNormalizer {
    phase_reference: Vec<Complex32>,
}

#[derive(Debug, Clone, Copy)]
pub struct PhaseReferenceQuality {
    pub eq_mse: f32,
    pub eq_phase_rms_rad: f32,
    pub channel_gain_avg: f32,
    pub channel_gain_spread_db: f32,
}

fn phase_reference_quadrature(value: u8) -> Complex32 {
    match value & 0x03 {
        0 => Complex32::new(1.0, 0.0),
        1 => Complex32::new(0.0, 1.0),
        2 => Complex32::new(-1.0, 0.0),
        _ => Complex32::new(0.0, -1.0),
    }
}

fn append_phase_reference_block(target: &mut Vec<Complex32>, pattern: &[(usize, u8)]) {
    for (table_index, phase_offset) in pattern {
        for value in DAB_PHASE_REFERENCE_H[*table_index] {
            target.push(phase_reference_quadrature(value + *phase_offset));
        }
    }
}

pub fn dab_mode_i_phase_reference_mapped() -> Vec<Complex32> {
    let mut positive = Vec::with_capacity(DAB_MODE_I_ACTIVE_CARRIERS / 2);
    append_phase_reference_block(&mut positive, &DAB_MODE_I_PHASE_REFERENCE_POSITIVE);

    let mut negative = Vec::with_capacity(DAB_MODE_I_ACTIVE_CARRIERS / 2);
    append_phase_reference_block(&mut negative, &DAB_MODE_I_PHASE_REFERENCE_NEGATIVE);

    let mut mapped = Vec::with_capacity(DAB_MODE_I_ACTIVE_CARRIERS);
    mapped.extend_from_slice(&negative);
    mapped.extend_from_slice(&positive);
    mapped
}

fn estimate_channel_from_reference(
    observed_reference: &DabMappedSymbol,
    theoretical_reference: &[Complex32],
) -> Vec<Complex32> {
    observed_reference
        .carriers
        .iter()
        .zip(theoretical_reference.iter())
        .map(|(observed, theoretical)| {
            let theoretical_energy = theoretical.norm_sqr();
            if theoretical_energy <= 1e-9 {
                Complex32::new(1.0, 0.0)
            } else {
                *observed / *theoretical
            }
        })
        .collect()
}

fn equalize_symbol_from_channel(symbol: &DabMappedSymbol, channel: &[Complex32]) -> DabMappedSymbol {
    let carriers = symbol
        .carriers
        .iter()
        .enumerate()
        .map(|(index, carrier)| {
            let estimate = channel
                .get(index)
                .copied()
                .unwrap_or_else(|| Complex32::new(1.0, 0.0));
            let estimate_energy = estimate.norm_sqr();
            if estimate_energy <= 1e-9 {
                *carrier
            } else {
                *carrier / estimate
            }
        })
        .collect();

    DabMappedSymbol {
        start_sample: symbol.start_sample,
        carriers,
    }
}

impl Default for FrameNormalizer {
    fn default() -> Self {
        Self::new()
    }
}

impl FrameNormalizer {
    pub fn new() -> Self {
        Self {
            phase_reference: dab_mode_i_phase_reference_mapped(),
        }
    }

    pub fn normalize_frames(&self, frames: &[DabMappedFrameCandidate]) -> Vec<DabNormalizedFrame> {
        frames
            .iter()
            .filter_map(|frame| self.normalize_frame(frame))
            .collect()
    }

    pub fn normalize_frame(&self, frame: &DabMappedFrameCandidate) -> Option<DabNormalizedFrame> {
        if frame.symbols.len() != DAB_MODE_I_SYMBOLS_PER_FRAME {
            return None;
        }

        let reference_raw = &frame.symbols[0];
        let channel = estimate_channel_from_reference(reference_raw, &self.phase_reference);
        let phase_reference = equalize_symbol_from_channel(reference_raw, &channel);
        let fic_symbols = frame.symbols[1..1 + DAB_FIC_SYMBOL_COUNT]
            .iter()
            .map(|symbol| equalize_symbol_from_channel(symbol, &channel))
            .collect();
        let msc_symbols = frame.symbols[1 + DAB_FIC_SYMBOL_COUNT..].to_vec();

        if msc_symbols.len() != DAB_MSC_SYMBOL_COUNT {
            return None;
        }

        // Compute per-carrier gain weights for LLR scaling in the FIC demapper.
        // Normalise to mean gain = 1 so the overall Viterbi scale is unchanged,
        // but weak carriers (deep fades) get < 1 and strong ones get capped at 2.
        let raw_gains: Vec<f32> = channel.iter().map(|h| h.norm()).collect();
        let gain_sum: f32 = raw_gains.iter().sum();
        let gain_avg = if raw_gains.is_empty() { 1.0_f32 } else { gain_sum / raw_gains.len() as f32 };
        let inv_avg = if gain_avg > 1e-12 { 1.0 / gain_avg } else { 1.0 };
        let channel_gains: Vec<f32> = raw_gains.iter().map(|&g| (g * inv_avg).min(2.0)).collect();

        Some(DabNormalizedFrame {
            start_sample: frame.start_sample,
            phase_reference,
            fic_symbols,
            msc_symbols,
            channel_gains,
        })
    }

    pub fn analyze_phase_reference(&self, frame: &DabMappedFrameCandidate) -> Option<PhaseReferenceQuality> {
        let reference_raw = frame.symbols.first()?;
        if reference_raw.carriers.is_empty() {
            return None;
        }

        let channel = estimate_channel_from_reference(reference_raw, &self.phase_reference);
        let equalized_reference = equalize_symbol_from_channel(reference_raw, &channel);

        let mut count = 0usize;
        let mut mse_sum = 0.0f32;
        let mut phase_sq_sum = 0.0f32;

        for (equalized, theoretical) in equalized_reference
            .carriers
            .iter()
            .zip(self.phase_reference.iter())
        {
            let error = *equalized - *theoretical;
            mse_sum += error.norm_sqr();

            let phase_diff = (*equalized * theoretical.conj()).arg();
            phase_sq_sum += phase_diff * phase_diff;
            count += 1;
        }

        if count == 0 {
            return None;
        }

        let mut gain_sum = 0.0f32;
        let mut gain_min = f32::INFINITY;
        let mut gain_max = 0.0f32;
        let mut gain_count = 0usize;

        for estimate in channel.iter().take(count) {
            let magnitude = estimate.norm();
            if !magnitude.is_finite() || magnitude <= 1e-9 {
                continue;
            }
            gain_sum += magnitude;
            gain_min = gain_min.min(magnitude);
            gain_max = gain_max.max(magnitude);
            gain_count += 1;
        }

        if gain_count == 0 {
            return None;
        }

        let channel_gain_avg = gain_sum / gain_count as f32;
        let channel_gain_spread_db = if gain_min > 0.0 {
            20.0 * (gain_max / gain_min).log10()
        } else {
            0.0
        };

        Some(PhaseReferenceQuality {
            eq_mse: mse_sum / count as f32,
            eq_phase_rms_rad: (phase_sq_sum / count as f32).sqrt(),
            channel_gain_avg,
            channel_gain_spread_db,
        })
    }

    pub fn extract_fic_candidates(&self, frames: &[DabNormalizedFrame]) -> Vec<FicCandidate> {
        frames
            .iter()
            .map(|frame| FicCandidate {
                frame_start_sample: frame.start_sample,
                symbol_count: frame.fic_symbols.len(),
                carriers_per_symbol: frame
                    .fic_symbols
                    .first()
                    .map(|symbol| symbol.carriers.len())
                    .unwrap_or(0),
            })
            .collect()
    }
}

pub struct FicDemapper;

pub struct FicPreDecoder {
    pub(crate) fic_symbol_count: usize,
    pub(crate) segment_bits: usize,
}

    pub struct FibExtractor;

    pub struct SignallingDecoder;

impl Default for MultiplexState {
    fn default() -> Self {
        Self::new()
    }
}

impl MultiplexState {
    pub fn new() -> Self {
        Self {
            updates: 0,
            total_fig_count: 0,
            total_type1_count: 0,
            last_frame_start_sample: None,
            type0_extensions: Vec::new(),
            ensemble_info: None,
            sub_channels: Vec::new(),
            services: Vec::new(),
        }
    }

    pub fn update(&mut self, snapshot: &SignallingSnapshot, frame_start_sample: Option<usize>) {
        self.updates += 1;
        self.total_fig_count += snapshot.fig_count;
        self.total_type1_count += snapshot.type1_count;
        self.last_frame_start_sample = frame_start_sample;

        for entry in &snapshot.type0_extensions {
            match self
                .type0_extensions
                .iter_mut()
                .find(|existing| existing.extension == entry.extension)
            {
                Some(existing) => {
                    existing.count += entry.count;
                    existing.cn |= entry.cn;
                    existing.oe |= entry.oe;
                    existing.pd |= entry.pd;
                    existing.last_body = entry.last_body.clone();
                }
                None => self.type0_extensions.push(entry.clone()),
            }
        }

        self.type0_extensions
            .sort_by(|left, right| left.extension.cmp(&right.extension));

        // Décoder les extensions FIG 0 connues depuis les corps accumulés
        for entry in &snapshot.type0_extensions {
            match decode_fig0(entry.extension, entry.pd, &entry.last_body) {
                Fig0Decoded::EnsembleInfo(info) => {
                    self.ensemble_info = Some(info);
                }
                Fig0Decoded::SubChannels(channels) => {
                    for ch in channels {
                        match self.sub_channels.iter_mut().find(|c| c.id == ch.id) {
                            Some(existing) => *existing = ch,
                            None => self.sub_channels.push(ch),
                        }
                    }
                }
                Fig0Decoded::Services(svcs) => {
                    for svc in svcs {
                        match self.services.iter_mut().find(|s| s.sid == svc.sid) {
                            Some(existing) => *existing = svc,
                            None => self.services.push(svc),
                        }
                    }
                }
                Fig0Decoded::Unknown => {}
            }
        }
    }
}

impl CarrierMapper {
    pub fn new(fft_len: usize, active_carriers: usize) -> Self {
        Self {
            fft_len,
            active_carriers,
            half_active: active_carriers / 2,
        }
    }

    pub fn map_frames(&self, frames: &[DabFrequencyFrameCandidate]) -> Vec<DabMappedFrameCandidate> {
        frames
            .iter()
            .map(|frame| DabMappedFrameCandidate {
                start_sample: frame.start_sample,
                symbol_count: frame.symbol_count,
                symbols: frame
                    .symbols
                    .iter()
                    .map(|symbol| DabMappedSymbol {
                        start_sample: symbol.start_sample,
                        carriers: self.map_symbol(symbol),
                    })
                    .collect(),
            })
            .collect()
    }

    pub fn map_symbol(&self, symbol: &DabFrequencySymbol) -> Vec<Complex32> {
        let carriers = &symbol.carriers;
        debug_assert_eq!(carriers.len(), self.fft_len);

        let mut mapped = Vec::with_capacity(self.active_carriers);

        mapped.extend_from_slice(&carriers[self.fft_len - self.half_active..self.fft_len]);
        mapped.extend_from_slice(&carriers[1..=self.half_active]);

        mapped
    }
}

impl FrequencyFrameTransformer {
    pub fn new(fft_len: usize) -> Self {
        let mut planner = FftPlanner::<f32>::new();
        let fft = planner.plan_fft_forward(fft_len);
        Self { fft_len, fft }
    }

    pub fn transform_frames(
        &self,
        frames: &[DabFrameCandidate],
    ) -> Vec<DabFrequencyFrameCandidate> {
        frames
            .iter()
            .map(|frame| DabFrequencyFrameCandidate {
                start_sample: frame.start_sample,
                symbol_count: frame.symbol_count,
                symbols: frame
                    .symbols
                    .iter()
                    .map(|symbol| DabFrequencySymbol {
                        start_sample: symbol.start_sample,
                        carriers: self.transform_symbol(symbol),
                    })
                    .collect(),
            })
            .collect()
    }

    pub fn transform_symbol(&self, symbol: &DabOfdmSymbol) -> Vec<Complex32> {
        let mut buffer = iq_bytes_to_complex(&symbol.iq_fft_only);
        debug_assert_eq!(buffer.len(), self.fft_len);
        apply_cfo_correction(&mut buffer, symbol.cfo_phase_per_sample);
        self.fft.process(&mut buffer);
        buffer
    }
}

#[derive(Debug)]
pub struct FrameBuilder {
    symbol_len: usize,
    pending_symbols: Vec<DabOfdmSymbol>,
}

impl FrameBuilder {
    pub fn new(symbol_len: usize) -> Self {
        Self {
            symbol_len,
            pending_symbols: Vec::new(),
        }
    }

    pub fn push_symbols(&mut self, symbols: Vec<DabOfdmSymbol>) -> Vec<DabFrameCandidate> {
        self.pending_symbols.extend(symbols);

        let mut frames = Vec::new();

        loop {
            if self.pending_symbols.len() < DAB_MODE_I_SYMBOLS_PER_FRAME {
                break;
            }

            if !self.has_consecutive_prefix(DAB_MODE_I_SYMBOLS_PER_FRAME) {
                self.pending_symbols.drain(0..1);
                continue;
            }

            let frame_symbols: Vec<DabOfdmSymbol> = self
                .pending_symbols
                .drain(0..DAB_MODE_I_SYMBOLS_PER_FRAME)
                .collect();

            let start_sample = frame_symbols[0].start_sample;
            frames.push(DabFrameCandidate {
                start_sample,
                symbol_count: frame_symbols.len(),
                symbols: frame_symbols,
            });
        }

        frames
    }

    pub fn has_consecutive_prefix(&self, count: usize) -> bool {
        if self.pending_symbols.len() < count {
            return false;
        }

        for idx in 1..count {
            let prev = self.pending_symbols[idx - 1].start_sample;
            let current = self.pending_symbols[idx].start_sample;
            if current != prev + self.symbol_len {
                return false;
            }
        }

        true
    }
}

#[derive(Debug)]
pub struct FrameAligner {
    fft_len: usize,
    cp_len: usize,
    symbol_len: usize,
    pending: Vec<u8>,
    pending_start_sample: usize,
    ingested_samples: usize,
    next_symbol_sample: Option<usize>,
    current_cfo_phase_per_sample: f32,
}

impl FrameAligner {
    pub fn new(fft_len: usize, cp_len: usize) -> Self {
        Self {
            fft_len,
            cp_len,
            symbol_len: fft_len + cp_len,
            pending: Vec::new(),
            pending_start_sample: 0,
            ingested_samples: 0,
            next_symbol_sample: None,
            current_cfo_phase_per_sample: 0.0,
        }
    }

    pub fn push_chunk(
        &mut self,
        iq_chunk: &[u8],
        sync_candidate: Option<SyncCandidate>,
    ) -> Vec<DabOfdmSymbol> {
        let chunk_samples = iq_chunk.len() / IQ_BYTES_PER_SAMPLE;
        if chunk_samples == 0 {
            return Vec::new();
        }

        if self.pending.is_empty() {
            self.pending_start_sample = self.ingested_samples;
        }

        self.pending.extend_from_slice(iq_chunk);
        self.ingested_samples += chunk_samples;

        if self.next_symbol_sample.is_none() {
            if let Some(sync) = sync_candidate {
                self.next_symbol_sample = Some(sync.sample_offset);
                self.current_cfo_phase_per_sample = sync.cfo_phase_per_sample;
            }
        } else if let Some(sync) = sync_candidate {
            self.current_cfo_phase_per_sample = sync.cfo_phase_per_sample;
        }

        let mut symbols = Vec::new();

        while let Some(next_symbol) = self.next_symbol_sample {
            if next_symbol < self.pending_start_sample {
                self.next_symbol_sample = Some(next_symbol + self.symbol_len);
                continue;
            }

            let pending_samples = self.pending.len() / IQ_BYTES_PER_SAMPLE;
            let pending_end_sample = self.pending_start_sample + pending_samples;
            let symbol_end = next_symbol + self.symbol_len;

            if symbol_end > pending_end_sample {
                break;
            }

            let local_start = next_symbol - self.pending_start_sample;
            let byte_start = local_start * IQ_BYTES_PER_SAMPLE;
            let cp_bytes = self.cp_len * IQ_BYTES_PER_SAMPLE;
            let fft_bytes = self.fft_len * IQ_BYTES_PER_SAMPLE;
            let fft_start = byte_start + cp_bytes;

            let iq_fft_only = self.pending[fft_start..fft_start + fft_bytes].to_vec();
            symbols.push(DabOfdmSymbol {
                start_sample: next_symbol,
                cfo_phase_per_sample: self.current_cfo_phase_per_sample,
                iq_fft_only,
            });

            self.next_symbol_sample = Some(next_symbol + self.symbol_len);
        }

        self.trim_pending();
        symbols
    }

    pub fn trim_pending(&mut self) {
        let pending_samples = self.pending.len() / IQ_BYTES_PER_SAMPLE;
        if pending_samples == 0 {
            return;
        }

        let keep_samples = self.symbol_len * MAX_PENDING_SYMBOLS;
        if pending_samples <= keep_samples {
            return;
        }

        let drop_samples = pending_samples - keep_samples;
        let drop_bytes = drop_samples * IQ_BYTES_PER_SAMPLE;
        self.pending.drain(0..drop_bytes);
        self.pending_start_sample += drop_samples;
    }
}

#[derive(Debug)]
pub struct OfdmSyncDetector {
    pub(crate) fft_len: usize,
    pub(crate) cp_len: usize,
    pub(crate) symbol_len: usize,
    pub(crate) threshold: f32,
    pub(crate) tail: Vec<u8>,
    pub(crate) total_samples_seen: usize,
    pub(crate) predicted_next_symbol_sample: Option<usize>,
    pub(crate) cfo_phase_smoothed: f32,
    pub(crate) lock_misses: u32,
}

#[inline]
pub fn qpsk_hard_demapp(value: Complex32) -> (u8, u8) {
    let bit_i = if value.re >= 0.0 { 0 } else { 1 };
    let bit_q = if value.im >= 0.0 { 0 } else { 1 };
    (bit_i, bit_q)
}

pub fn crc16_matches(bits: &[u8]) -> bool {
    if bits.len() < 16 {
        return false;
    }

    let payload_bits = &bits[..bits.len() - 16];
    let expected_crc = bits_to_u16(&bits[bits.len() - 16..]);
    crc16_ccitt_false(payload_bits) == expected_crc
}

pub fn crc16_ccitt_false(bits: &[u8]) -> u16 {
    let mut crc = 0xFFFFu16;

    for bit in bits {
        let top = (crc & 0x8000) != 0;
        crc <<= 1;
        if (bit & 1) != 0 {
            crc |= 1;
        }
        if top {
            crc ^= 0x1021;
        }
    }

    crc
}

pub fn bits_to_u16(bits: &[u8]) -> u16 {
    bits.iter().fold(0u16, |acc, bit| (acc << 1) | ((bit & 1) as u16))
}

pub fn bits_to_bytes(bits: &[u8]) -> Vec<u8> {
    bits.chunks_exact(8)
        .map(|chunk| chunk.iter().fold(0u8, |acc, bit| (acc << 1) | (bit & 1)))
        .collect()
}

/// Décode l'extension d'une FIG de type 0 selon ETSI EN 300 401 §6.2
pub fn decode_fig0(extension: u8, pd: bool, body: &[u8]) -> Fig0Decoded {
    match extension {
        0 => decode_fig0_ensemble_info(body),
        1 => decode_fig0_sub_channels(body),
        2 => decode_fig0_services(pd, body),
        _ => Fig0Decoded::Unknown,
    }
}

/// FIG 0/2 – Service Organization (ETSI EN 300 401 §8.1.14.2)
fn decode_fig0_services(pd: bool, body: &[u8]) -> Fig0Decoded {
    let mut services = Vec::new();
    let mut offset = 0usize;

    while offset < body.len() {
        let (sid, id_len) = if pd {
            if offset + 4 > body.len() { break; }
            let s = ((body[offset] as u32) << 24)
                | ((body[offset + 1] as u32) << 16)
                | ((body[offset + 2] as u32) << 8)
                | body[offset + 3] as u32;
            (s, 4usize)
        } else {
            if offset + 2 > body.len() { break; }
            let s = ((body[offset] as u32) << 8) | body[offset + 1] as u32;
            (s, 2usize)
        };
        offset += id_len;

        if offset >= body.len() { break; }
        let header = body[offset];
        let ca_id = (header >> 5) & 0x07;
        let ncomp = (header & 0x0F) as usize;
        offset += 1;

        if offset + ncomp * 2 > body.len() { break; }

        let mut components = Vec::with_capacity(ncomp);
        for _ in 0..ncomp {
            let b0 = body[offset];
            let b1 = body[offset + 1];
            components.push(DabServiceComponent {
                tm_id: b0 >> 6,
                type_id: b0 & 0x3F,
                sub_ch_id: b1 >> 2,
                primary: (b1 & 0x02) != 0,
                ca: (b1 & 0x01) != 0,
            });
            offset += 2;
        }

        services.push(DabService { sid, ca_id, components });
    }

    Fig0Decoded::Services(services)
}

// ── Constructeur de trames ETI-NI ─────────────────────────────────────────────

pub struct EtiFrameBuilder {
    frame_counter: u8,
}

impl Default for EtiFrameBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl EtiFrameBuilder {
    pub fn new() -> Self {
        Self { frame_counter: 0 }
    }

    /// Produit une trame ETI-NI de 6144 octets.
    /// `fic_bytes`: jusqu'à 96 octets de données FIC (3 FIBs mode I), None → FIC zéro.
    pub fn build_frame(&mut self, multiplex: &MultiplexState, fic_bytes: Option<&[u8]>) -> Vec<u8> {
        self.build_frame_with_msc(multiplex, fic_bytes, None)
    }

    /// Variante avec charge MSC explicite (best-effort) pour rapprocher le flux d'eti-cmdline.
    pub fn build_frame_with_msc(
        &mut self,
        multiplex: &MultiplexState,
        fic_bytes: Option<&[u8]>,
        msc_bytes: Option<&[u8]>,
    ) -> Vec<u8> {
        // eti-cmdline pads remaining bytes with 0x55.
        let mut frame = vec![0x55u8; ETI_FRAME_BYTES];
        let nst = multiplex.sub_channels.len().min(64) as u8;

        // SYNC (4 octets)
        let sync = if self.frame_counter.is_multiple_of(2) { ETI_SYNC_EVEN } else { ETI_SYNC_ODD };
        frame[0..4].copy_from_slice(&sync);

        // FC (4 octets) – FL = (6144−8)/4 = 1534 = 0x5FE, invariant mode I
        let fl: u16 = ((ETI_FRAME_BYTES - 8) / 4) as u16;
        frame[4] = self.frame_counter;
        frame[5] = (1u8 << 7) | (nst & 0x7F); // FICF=1 | NST
        let fp = self.frame_counter % 8;
        let mid: u8 = 0b01; // mode I
        frame[6] = (fp << 5) | (mid << 3) | ((fl >> 8) as u8 & 0x07);
        frame[7] = (fl & 0xFF) as u8;

        // STC (4 octets × NST)
        let stc_start = 8usize;
        for (idx, ch) in multiplex.sub_channels.iter().take(64).enumerate() {
            let off = stc_start + idx * 4;
            let (tpl, stl) = sub_channel_tpl_stl(ch);
            frame[off]     = (ch.id << 2) | ((ch.start_address >> 8) as u8 & 0x03);
            frame[off + 1] = (ch.start_address & 0xFF) as u8;
            frame[off + 2] = (tpl << 2) | ((stl >> 8) as u8 & 0x03);
            frame[off + 3] = (stl & 0xFF) as u8;
        }

        // EOH (4 octets): MNF (2 zéros) + CRC-CCITT sur FC+STC+MNF
        let eoh_start = stc_start + (nst as usize) * 4;
        let eoh_crc = crc16_ccitt_bytes(&frame[4..eoh_start + 2]);
        frame[eoh_start + 2] = (eoh_crc >> 8) as u8;
        frame[eoh_start + 3] = (eoh_crc & 0xFF) as u8;

        // MST : FIC (96 octets) + MSC
        let mst_start = eoh_start + 4;
        let eof_start  = ETI_FRAME_BYTES - 8;
        if let Some(fic) = fic_bytes {
            let len = fic.len().min(ETI_FIC_BYTES);
            frame[mst_start..mst_start + len].copy_from_slice(&fic[..len]);
        }

        if nst > 0 {
            if let Some(msc) = msc_bytes {
            let msc_start = mst_start + ETI_FIC_BYTES;
            if msc_start < eof_start {
                let room = eof_start - msc_start;
                let len = msc.len().min(room);
                frame[msc_start..msc_start + len].copy_from_slice(&msc[..len]);
            }
            }
        }

        // EOF (4 octets): CRC sur MST, puis MNSC=0xFFFF
        let eof_crc = crc16_ccitt_bytes(&frame[mst_start..eof_start]);
        frame[eof_start]     = (eof_crc >> 8) as u8;
        frame[eof_start + 1] = (eof_crc & 0xFF) as u8;
        frame[eof_start + 2] = 0xFF;
        frame[eof_start + 3] = 0xFF;

        // TIST (4 octets): 0xFFFFFF00 = horodatage non disponible
        let tist_start = ETI_FRAME_BYTES - 4;
        frame[tist_start]     = 0xFF;
        frame[tist_start + 1] = 0xFF;
        frame[tist_start + 2] = 0xFF;
        frame[tist_start + 3] = 0x00;

        self.frame_counter = (self.frame_counter + 1) % 250;
        frame
    }
}

fn sub_channel_tpl_stl(ch: &DabSubChannel) -> (u8, u16) {
    match &ch.protection {
        SubChannelProtection::Short { table_index, .. } => {
            (*table_index & 0x3F, uep_short_stl(*table_index))
        }
        SubChannelProtection::Long { protection_level, size_cu, .. } => {
            (protection_level & 0x3F, size_cu & 0x3FF)
        }
    }
}

/// Taille en CUs (= STL) pour protection courte UEP (EN 300 401 Tableau 7, simplifié)
fn uep_short_stl(table_index: u8) -> u16 {
    const CU_TABLE: &[u16] = &[
        16, 21, 24, 29, 24, 29, 35, 42, 28, 35,
        42, 52, 32, 42, 48, 58, 40, 52, 58, 70,
        48, 58, 70, 84,
    ];
    CU_TABLE.get(table_index as usize).copied().unwrap_or(84)
}

/// CRC-16/CCITT-FALSE sur octets bruts (MSB en premier)
pub fn crc16_ccitt_bytes(data: &[u8]) -> u16 {
    let mut crc = 0xFFFFu16;
    for &byte in data {
        for shift in (0..8).rev() {
            let bit = (byte >> shift) & 1;
            let top = (crc & 0x8000) != 0;
            crc <<= 1;
            if bit != 0 { crc |= 1; }
            if top { crc ^= 0x1021; }
        }
    }
    crc
}

/// FIG 0/0 – Ensemble Information (§6.2.1)
/// Body = 4 bytes: EId[15:8] EId[7:0] | CF[1:0] AL CIF_H[4:0] | CIF_L[7:0]
fn decode_fig0_ensemble_info(body: &[u8]) -> Fig0Decoded {
    if body.len() < 4 {
        return Fig0Decoded::Unknown;
    }
    Fig0Decoded::EnsembleInfo(DabEnsembleInfo {
        eid: (body[0] as u16) << 8 | body[1] as u16,
        change_flags: (body[2] >> 6) & 0x03,
        al: (body[2] & 0x20) != 0,
        cif_count_high: body[2] & 0x1F,
        cif_count_low: body[3],
    })
}

/// FIG 0/1 – Sub-Channel Organization (§6.2.1)
/// Short form = 3 bytes/entrée, Long form = 4 bytes/entrée (bit S du 3e octet)
fn decode_fig0_sub_channels(body: &[u8]) -> Fig0Decoded {
    let mut sub_channels = Vec::new();
    let mut offset = 0usize;

    while offset + 3 <= body.len() {
        let byte0 = body[offset];
        let byte1 = body[offset + 1];
        let byte2 = body[offset + 2];
        let long_form = (byte2 & 0x80) != 0;

        if long_form {
            if offset + 4 > body.len() {
                break;
            }
            // Layout 32 bits: SubChId[5:0] SA[9:0] S=1 Option ProtLevel[1:0] ProtType SubChSize[9:0] spare
            let val = ((byte0 as u32) << 24)
                | ((byte1 as u32) << 16)
                | ((byte2 as u32) << 8)
                | body[offset + 3] as u32;
            sub_channels.push(DabSubChannel {
                id: ((val >> 26) & 0x3F) as u8,
                start_address: ((val >> 16) & 0x3FF) as u16,
                protection: SubChannelProtection::Long {
                    protection_level: ((val >> 12) & 0x3) as u8,
                    protection_type: (val >> 11) & 1 != 0,
                    size_cu: ((val >> 1) & 0x3FF) as u16,
                },
            });
            offset += 4;
        } else {
            // Layout 24 bits: SubChId[5:0] SA[9:0] S=0 TableSwitch TableIndex[5:0]
            sub_channels.push(DabSubChannel {
                id: byte0 >> 2,
                start_address: ((byte0 & 0x03) as u16) << 8 | byte1 as u16,
                protection: SubChannelProtection::Short {
                    table_switch: (byte2 & 0x40) != 0,
                    table_index: byte2 & 0x3F,
                },
            });
            offset += 3;
        }
    }

    Fig0Decoded::SubChannels(sub_channels)
}
