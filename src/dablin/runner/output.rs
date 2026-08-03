use anyhow::{Context, Result};
use std::io::{self, Write};
use tracing::info;

pub(crate) enum WriteOutcome {
    Written,
    Closed,
}

fn write_bytes_or_exit(
    out: &mut impl Write,
    bytes: &[u8],
    error_context: &'static str,
) -> Result<WriteOutcome> {
    if let Err(e) = out.write_all(bytes) {
        if e.kind() == io::ErrorKind::BrokenPipe {
            info!("stdout closed, exiting");
            return Ok(WriteOutcome::Closed);
        }
        return Err(e).context(error_context);
    }
    Ok(WriteOutcome::Written)
}

pub(crate) fn write_pcm_or_exit(
    out: &mut impl Write,
    pcm: &[i16],
    scratch: &mut Vec<u8>,
) -> Result<WriteOutcome> {
    scratch.clear();
    scratch.reserve(std::mem::size_of_val(pcm));
    for &sample in pcm {
        scratch.extend_from_slice(&sample.to_le_bytes());
    }
    write_bytes_or_exit(out, scratch, "PCM write error")
}

pub(crate) fn write_adts_or_exit(out: &mut impl Write, adts: &[u8]) -> Result<WriteOutcome> {
    write_bytes_or_exit(out, adts, "ADTS write error")
}

pub(crate) fn write_latm_or_exit(out: &mut impl Write, latm: &[u8]) -> Result<WriteOutcome> {
    write_bytes_or_exit(out, latm, "LATM write error")
}
