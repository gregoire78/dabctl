use anyhow::{Context, Result};
use std::io::{BufWriter, Seek, SeekFrom, Write};
use std::path::Path;

pub(crate) struct WavWriter {
    file: BufWriter<std::fs::File>,
    data_len: u32,
    pcm_bytes: Vec<u8>,
}

impl WavWriter {
    pub(crate) fn create(path: &Path) -> Result<Self> {
        let file = std::fs::File::create(path)
            .with_context(|| format!("cannot create WAV file: {}", path.display()))?;
        let mut file = BufWriter::new(file);
        write_wav_header(&mut file, 0)?;
        Ok(Self {
            file,
            data_len: 0,
            pcm_bytes: Vec::new(),
        })
    }

    pub(crate) fn write_pcm(&mut self, pcm: &[i16]) -> Result<()> {
        self.pcm_bytes.clear();
        self.pcm_bytes.reserve(pcm.len().saturating_mul(2));
        for sample in pcm {
            self.pcm_bytes.extend_from_slice(&sample.to_le_bytes());
        }

        self.file.write_all(&self.pcm_bytes)?;
        let written = u32::try_from(self.pcm_bytes.len()).unwrap_or(u32::MAX);
        self.data_len = self.data_len.saturating_add(written);
        Ok(())
    }

    pub(crate) fn finalize(&mut self) -> Result<()> {
        self.file.seek(SeekFrom::Start(0))?;
        write_wav_header(&mut self.file, self.data_len)?;
        self.file.flush()?;
        Ok(())
    }
}

fn write_wav_header<W: Write>(file: &mut W, data_len: u32) -> Result<()> {
    let riff_size = 36u32.saturating_add(data_len);
    let byte_rate = 48_000u32 * 2u32 * 2u32;
    let block_align = 2u16 * 2u16;

    file.write_all(b"RIFF")?;
    file.write_all(&riff_size.to_le_bytes())?;
    file.write_all(b"WAVE")?;
    file.write_all(b"fmt ")?;
    file.write_all(&16u32.to_le_bytes())?;
    file.write_all(&1u16.to_le_bytes())?;
    file.write_all(&2u16.to_le_bytes())?;
    file.write_all(&48_000u32.to_le_bytes())?;
    file.write_all(&byte_rate.to_le_bytes())?;
    file.write_all(&block_align.to_le_bytes())?;
    file.write_all(&16u16.to_le_bytes())?;
    file.write_all(b"data")?;
    file.write_all(&data_len.to_le_bytes())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::WavWriter;
    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_file_path(prefix: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        std::env::temp_dir().join(format!(
            "dabctl-{}-{}-{}.wav",
            prefix,
            std::process::id(),
            nanos
        ))
    }

    fn le_u32(bytes: &[u8], start: usize) -> u32 {
        u32::from_le_bytes([
            bytes[start],
            bytes[start + 1],
            bytes[start + 2],
            bytes[start + 3],
        ])
    }

    #[test]
    fn writes_empty_wav_header_on_finalize() {
        let path = temp_file_path("wav-empty");
        let mut wav = WavWriter::create(&path).expect("create wav");
        wav.finalize().expect("finalize wav");

        let bytes = fs::read(&path).expect("read wav");
        assert_eq!(bytes.len(), 44);
        assert_eq!(&bytes[0..4], b"RIFF");
        assert_eq!(le_u32(&bytes, 4), 36);
        assert_eq!(&bytes[8..12], b"WAVE");
        assert_eq!(&bytes[12..16], b"fmt ");
        assert_eq!(&bytes[36..40], b"data");
        assert_eq!(le_u32(&bytes, 40), 0);

        let _ = fs::remove_file(path);
    }

    #[test]
    fn writes_pcm_and_updates_sizes() {
        let path = temp_file_path("wav-pcm");
        let mut wav = WavWriter::create(&path).expect("create wav");
        let pcm = [1i16, -1i16, 32767i16, -32768i16];

        wav.write_pcm(&pcm).expect("write pcm");
        wav.finalize().expect("finalize wav");

        let bytes = fs::read(&path).expect("read wav");
        assert_eq!(bytes.len(), 44 + 8);
        assert_eq!(le_u32(&bytes, 4), 36 + 8);
        assert_eq!(le_u32(&bytes, 40), 8);

        let expected_payload = [1u8, 0, 255, 255, 255, 127, 0, 128];
        assert_eq!(&bytes[44..52], &expected_payload);

        let _ = fs::remove_file(path);
    }
}
