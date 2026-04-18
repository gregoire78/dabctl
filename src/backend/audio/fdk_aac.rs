#![cfg(feature = "fdk-aac")]

use anyhow::Result;

use super::{AacDecoder, StreamParameters};

// Optional backend placeholder. The default verified path uses faad2.
#[derive(Default)]
pub struct FdkAacDecoder;

impl AacDecoder for FdkAacDecoder {
    fn decode_access_unit(&mut self, _params: &StreamParameters, _data: &[u8]) -> Result<Vec<i16>> {
        Ok(Vec::new())
    }
}
