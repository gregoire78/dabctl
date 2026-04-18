use std::fs::File;
use std::io::Write;
use std::mem::ManuallyDrop;
use std::os::fd::{AsRawFd, FromRawFd};
use std::path::Path;

use anyhow::Result;
use base64::Engine;
use serde::Serialize;

#[derive(Default)]
pub struct MetadataWriter {
    file: Option<File>,
}

impl MetadataWriter {
    pub fn from_fd3() -> Result<Self> {
        // The contract requires explicit handling of file descriptor 3.
        if unsafe { libc::fcntl(3, libc::F_GETFD) } == -1 {
            return Ok(Self { file: None });
        }

        let fd3 = unsafe { ManuallyDrop::new(File::from_raw_fd(3)) };
        let dup_fd = unsafe { libc::dup(fd3.as_raw_fd()) };
        if dup_fd < 0 {
            return Ok(Self { file: None });
        }

        let file = unsafe { File::from_raw_fd(dup_fd) };
        Ok(Self { file: Some(file) })
    }

    #[allow(dead_code)]
    pub fn write_ensemble(&mut self, eid: u32, label: &str) -> Result<()> {
        self.write_json(&serde_json::json!({
            "ensemble": {
                "eid": format!("0x{eid:04X}"),
                "label": label,
            }
        }))
    }

    pub fn write_service(&mut self, sid: u32, label: &str) -> Result<()> {
        self.write_json(&serde_json::json!({
            "service": {
                "sid": format!("0x{sid:04X}"),
                "label": label,
            }
        }))
    }

    pub fn write_dynamic_label(&mut self, text: &str) -> Result<()> {
        self.write_json(&serde_json::json!({ "dl": text }))
    }

    #[allow(dead_code)]
    pub fn write_slide(
        &mut self,
        content_name: &str,
        content_type: &str,
        payload: &[u8],
        include_base64: bool,
    ) -> Result<()> {
        if include_base64 {
            let encoded = base64::engine::general_purpose::STANDARD.encode(payload);
            self.write_json(&serde_json::json!({
                "slide": {
                    "contentName": content_name,
                    "contentType": content_type,
                    "data": encoded,
                }
            }))
        } else {
            self.write_json(&serde_json::json!({
                "slide": {
                    "contentName": content_name,
                    "contentType": content_type,
                }
            }))
        }
    }

    pub fn save_slide_to_dir(&mut self, dir: &Path, name: &str, payload: &[u8]) -> Result<()> {
        let path = dir.join(name);
        std::fs::write(path, payload)?;
        Ok(())
    }

    fn write_json<T: Serialize>(&mut self, event: &T) -> Result<()> {
        if let Some(file) = self.file.as_mut() {
            serde_json::to_writer(&mut *file, event)?;
            file.write_all(b"\n")?;
            file.flush()?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::MetadataWriter;

    #[test]
    fn no_fd3_is_a_valid_noop_sink() {
        let mut writer = MetadataWriter::default();
        writer
            .write_dynamic_label("hello world")
            .expect("no-op metadata sink should succeed");
    }
}
