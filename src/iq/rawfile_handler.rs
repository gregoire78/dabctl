use crate::iq::IqSource;
use anyhow::{Context, Result};
use std::fs::File;
use std::io::{BufReader, Read};
use std::path::{Path, PathBuf};

pub struct FileReplaySource {
    path: PathBuf,
    loop_forever: bool,
    reader: BufReader<File>,
}

impl FileReplaySource {
    pub fn new(path: impl AsRef<Path>, loop_forever: bool) -> Result<Self> {
        let path_buf = path.as_ref().to_path_buf();
        let file = File::open(&path_buf)
            .with_context(|| format!("Unable to open IQ file: {}", path_buf.display()))?;

        Ok(Self {
            path: path_buf,
            loop_forever,
            reader: BufReader::new(file),
        })
    }

    fn reopen(&mut self) -> Result<()> {
        let file = File::open(&self.path)
            .with_context(|| format!("Unable to reopen IQ file: {}", self.path.display()))?;
        self.reader = BufReader::new(file);
        Ok(())
    }
}

impl IqSource for FileReplaySource {
    fn read_chunk(&mut self, buffer: &mut [u8]) -> Result<usize> {
        let mut total = 0usize;

        while total < buffer.len() {
            let read = self.reader.read(&mut buffer[total..])?;
            if read == 0 {
                if !self.loop_forever {
                    return Ok(total);
                }
                self.reopen()?;
                continue;
            }
            total += read;
        }

        Ok(total)
    }
}
