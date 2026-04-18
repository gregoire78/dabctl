use anyhow::Result;

use crate::backend::audio::{AacDecoder, Mp4Processor};

// Literal receive-path split after FIC handling, mirroring DABstar's MSC branch.
pub struct MscHandler {
    mp4_processor: Mp4Processor,
}

impl MscHandler {
    pub fn new(bit_rate: u16, decoder: Box<dyn AacDecoder>) -> Self {
        Self {
            mp4_processor: Mp4Processor::new(bit_rate, decoder),
        }
    }

    pub fn process_block(&mut self, soft_bits: &[i8]) -> Result<Vec<i16>> {
        self.mp4_processor.add_to_frame(soft_bits)
    }

    pub fn last_dynamic_label(&self) -> Option<&str> {
        self.mp4_processor.last_dynamic_label()
    }
}
