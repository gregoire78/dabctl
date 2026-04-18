use anyhow::Result;
use tracing::info;

use crate::backend::{
    audio::{DecoderFactory, Mp4Processor},
    data::mot::Slide,
    deconvolver::BackendDeconvolver,
};
use crate::cli::AacDecoderKind;
use crate::decoder::fib_decoder::AudioServiceInfo;

const MSC_BITS_PER_SYMBOL: usize = 3_072;
const CIF_BITS: usize = 18 * MSC_BITS_PER_SYMBOL;
const CU_BITS: usize = 64;
const INTERLEAVE_MAP: [usize; 16] = [0, 8, 4, 12, 2, 10, 6, 14, 1, 9, 5, 13, 3, 11, 7, 15];

// Literal receive-path split after FIC handling, mirroring DABstar's MSC branch.
pub struct MscHandler {
    decoder_kind: AacDecoderKind,
    mp4_processor: Mp4Processor,
    deconvolver: Option<BackendDeconvolver>,
    selected_service: Option<AudioServiceInfo>,
    cif_vector: Vec<i16>,
    interleave_data: Vec<Vec<i16>>,
    temp_x: Vec<i16>,
    interleaver_index: usize,
    interleaver_fill: usize,
}

impl MscHandler {
    pub fn new(bit_rate: u16, decoder_kind: AacDecoderKind) -> Self {
        Self {
            decoder_kind,
            mp4_processor: Mp4Processor::new(bit_rate, DecoderFactory::create(decoder_kind)),
            deconvolver: None,
            selected_service: None,
            cif_vector: vec![0; CIF_BITS],
            interleave_data: Vec::new(),
            temp_x: Vec::new(),
            interleaver_index: 0,
            interleaver_fill: 0,
        }
    }

    pub fn configure_service(&mut self, info: AudioServiceInfo) -> Result<()> {
        let needs_reconfigure = self.selected_service.as_ref().is_none_or(|current| {
            current.subch_id != info.subch_id
                || current.start_addr != info.start_addr
                || current.cu_size != info.cu_size
                || current.bit_rate != info.bit_rate
                || current.short_form != info.short_form
                || current.prot_level != info.prot_level
        });

        if needs_reconfigure {
            self.deconvolver = Some(BackendDeconvolver::new(
                info.bit_rate,
                info.short_form,
                info.prot_level,
            )?);
            self.mp4_processor =
                Mp4Processor::new(info.bit_rate, DecoderFactory::create(self.decoder_kind));
            self.cif_vector.fill(0);
            let fragment_size = info.cu_size * CU_BITS;
            self.interleave_data = vec![vec![0; fragment_size]; 16];
            self.temp_x = vec![0; fragment_size];
            self.interleaver_index = 0;
            self.interleaver_fill = 0;
            info!(
                sid = format_args!("0x{:04X}", info.sid),
                label = %info.label,
                subch_id = info.subch_id,
                cu_start = info.start_addr,
                cu_size = info.cu_size,
                bit_rate = info.bit_rate,
                prot_level = info.prot_level,
                short_form = info.short_form,
                "MSC service route configured"
            );
            self.selected_service = Some(info);
        }

        Ok(())
    }

    pub fn process_block(&mut self, soft_bits: &[i16], block_nr: usize) -> Result<Vec<i16>> {
        {
            static DIAG_MSC_INTAKE: std::sync::atomic::AtomicUsize =
                std::sync::atomic::AtomicUsize::new(0);
            let n = DIAG_MSC_INTAKE.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            if n < 12 {
                tracing::debug!(
                    n,
                    block_nr,
                    soft_bits_len = soft_bits.len(),
                    expected_len = MSC_BITS_PER_SYMBOL,
                    "MSC block intake"
                );
            }
        }

        if block_nr < 4 || soft_bits.len() != MSC_BITS_PER_SYMBOL {
            return Ok(Vec::new());
        }

        let cur_block_idx = (block_nr - 4) % 18;
        let dst_start = cur_block_idx * MSC_BITS_PER_SYMBOL;
        self.cif_vector[dst_start..dst_start + MSC_BITS_PER_SYMBOL].copy_from_slice(soft_bits);

        if cur_block_idx < 17 {
            return Ok(Vec::new());
        }

        let (service, deconvolver) = match (&self.selected_service, &self.deconvolver) {
            (Some(service), Some(deconvolver)) => (service, deconvolver),
            _ => return Ok(Vec::new()),
        };

        let fragment_start = service.start_addr * CU_BITS;
        let fragment_end = fragment_start + service.cu_size * CU_BITS;
        if fragment_end > self.cif_vector.len() {
            return Ok(Vec::new());
        }

        let fragment = &self.cif_vector[fragment_start..fragment_end];
        if self.temp_x.len() != fragment.len() || self.interleave_data.len() != 16 {
            self.interleave_data = vec![vec![0; fragment.len()]; 16];
            self.temp_x = vec![0; fragment.len()];
            self.interleaver_index = 0;
            self.interleaver_fill = 0;
        }

        for (idx, soft) in fragment.iter().copied().enumerate() {
            let src_slot = (self.interleaver_index + INTERLEAVE_MAP[idx & 0x0F]) & 0x0F;
            self.temp_x[idx] = self.interleave_data[src_slot][idx];
            self.interleave_data[self.interleaver_index][idx] = soft;
        }

        // DABstar advances the interleaver index first and only starts
        // decoding after the full 16-CIF history has been filled.
        self.interleaver_index = (self.interleaver_index + 1) & 0x0F;
        if self.interleaver_fill <= 15 {
            if self.interleaver_fill < 4 {
                tracing::debug!(
                    block_nr,
                    cur_block_idx,
                    interleaver_fill = self.interleaver_fill,
                    fragment_len = fragment.len(),
                    "MSC deinterleaver warmup"
                );
            }
            self.interleaver_fill += 1;
            return Ok(Vec::new());
        }

        // DIAG: compare raw fragment vs time-deinterleaved output
        {
            static DIAG_MSC: std::sync::atomic::AtomicUsize =
                std::sync::atomic::AtomicUsize::new(0);
            let n = DIAG_MSC.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            if n < 5 {
                let frag_abs_mean =
                    fragment.iter().map(|x| (*x as f32).abs()).sum::<f32>() / fragment.len() as f32;
                let deint_abs_mean = self.temp_x.iter().map(|x| (*x as f32).abs()).sum::<f32>()
                    / self.temp_x.len() as f32;
                // Correlation: how similar are adjacent soft bits (smoothness)?
                let frag_corr: f32 = fragment
                    .windows(2)
                    .map(|w| (w[0] as f32) * (w[1] as f32))
                    .sum::<f32>()
                    / (fragment.len() - 1) as f32;
                let deint_corr: f32 = self
                    .temp_x
                    .windows(2)
                    .map(|w| (w[0] as f32) * (w[1] as f32))
                    .sum::<f32>()
                    / (self.temp_x.len() - 1) as f32;
                tracing::debug!(
                    n,
                    frag_abs_mean,
                    deint_abs_mean,
                    frag_corr,
                    deint_corr,
                    interleaver_index = self.interleaver_index,
                    interleaver_fill = self.interleaver_fill,
                    fragment_len = fragment.len(),
                    first_frag_16 = ?&fragment[..16.min(fragment.len())],
                    first_deint_16 = ?&self.temp_x[..16.min(self.temp_x.len())],
                    "MSC time deinterleaver diagnostic"
                );
            }
        }

        let decoded_bits = deconvolver.deconvolve(&self.temp_x)?;
        self.mp4_processor.add_to_frame(&decoded_bits)
    }

    pub fn last_dynamic_label(&self) -> Option<&str> {
        self.mp4_processor.last_dynamic_label()
    }

    pub fn take_last_slide(&mut self) -> Option<Slide> {
        self.mp4_processor.take_last_slide()
    }
}
