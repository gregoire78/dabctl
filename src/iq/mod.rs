#[path = "rawfile_handler.rs"]
pub mod replay;
#[path = "rtlsdr_handler.rs"]
pub mod rtlsdr;
pub mod rtlsdr_port;

use anyhow::Result;

pub trait IqSource {
    fn read_chunk(&mut self, buffer: &mut [u8]) -> Result<usize>;
}
