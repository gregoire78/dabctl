mod faad_decoder;
#[cfg(feature = "fdk-aac")]
mod fdk_aac;
mod mp4processor;

use anyhow::Result;

use crate::cli::AacDecoderKind;

pub use faad_decoder::FaadDecoder;
pub use mp4processor::{Mp4Processor, DEFAULT_DAB_PLUS_BITRATE};

pub trait AacDecoder: Send {
    fn decode_access_unit(&mut self, params: &StreamParameters, data: &[u8]) -> Result<Vec<i16>>;
}

#[derive(Debug, Clone, Copy, Default)]
pub struct StreamParameters {
    pub dac_rate: u8,
    pub sbr_flag: u8,
    pub ps_flag: u8,
    pub aac_channel_mode: u8,
    pub mpeg_surround: u8,
    pub core_ch_config: u8,
    pub core_sr_index: u8,
    pub extension_sr_index: u8,
}

pub struct DecoderFactory;

impl DecoderFactory {
    pub fn create(kind: AacDecoderKind) -> Box<dyn AacDecoder> {
        match kind {
            AacDecoderKind::Faad2 => Box::new(FaadDecoder::default()),
            AacDecoderKind::FdkAac => {
                #[cfg(feature = "fdk-aac")]
                {
                    Box::new(fdk_aac::FdkAacDecoder::default())
                }
                #[cfg(not(feature = "fdk-aac"))]
                {
                    Box::new(FaadDecoder::default())
                }
            }
        }
    }
}
