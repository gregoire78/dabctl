/// PAD JSON output on file descriptor 3 (compatible with dablin-gregoire format).
/// Also handles slideshow file saving and base64 JSON output.
use crate::audio::mot_manager::MotFile;
use base64::Engine;
use serde::Serialize;
use serde_json;
use std::io::Write;
use std::os::unix::io::FromRawFd;
use std::path::{Path, PathBuf};

/// Writer for DAB metadata JSON on fd 3 and slideshow output
pub struct PadOutput {
    writer: Option<std::fs::File>,
    slide_dir: Option<PathBuf>,
    slide_base64: bool,
}

impl PadOutput {
    /// Create a new PadOutput.
    ///
    /// Metadata JSON is written to fd 3, if it is open.
    /// Logs a warning if fd 3 is not available.
    pub fn new(slide_dir: Option<PathBuf>, slide_base64: bool) -> Self {
        let writer = if nix_fcntl_check(3) {
            Some(unsafe { std::fs::File::from_raw_fd(3) })
        } else {
            tracing::warn!("fd 3 is not open — metadata JSON output will be discarded. Use 3>file or 3>&1 to capture it.");
            None
        };

        // Create slide directory if specified
        if let Some(ref dir) = slide_dir {
            if let Err(e) = std::fs::create_dir_all(dir) {
                tracing::warn!("Cannot create slide directory {:?}: {}", dir, e);
            }
        }

        PadOutput {
            writer,
            slide_dir,
            slide_base64,
        }
    }

    /// Write ensemble info as JSON
    pub fn write_ensemble(&mut self, label: &str, short_label: &str, eid: u16) {
        #[derive(Serialize)]
        struct Ensemble<'a> {
            label: &'a str,
            #[serde(rename = "shortLabel")]
            short_label: &'a str,
            eid: String,
        }
        #[derive(Serialize)]
        struct Wrapper<'a> {
            ensemble: Ensemble<'a>,
        }
        let data = Wrapper {
            ensemble: Ensemble {
                label,
                short_label,
                eid: format!("0x{:04X}", eid),
            },
        };
        self.write_json_struct(&data);
    }

    /// Write service info as JSON
    pub fn write_service(&mut self, label: &str, short_label: &str, sid: u16) {
        #[derive(Serialize)]
        struct Service<'a> {
            label: &'a str,
            #[serde(rename = "shortLabel")]
            short_label: &'a str,
            sid: String,
        }
        #[derive(Serialize)]
        struct Wrapper<'a> {
            service: Service<'a>,
        }
        let data = Wrapper {
            service: Service {
                label,
                short_label,
                sid: format!("0x{:04X}", sid),
            },
        };
        self.write_json_struct(&data);
    }

    /// Write dynamic label (DLS) as JSON
    pub fn write_dl(&mut self, text: &str) {
        #[derive(Serialize)]
        struct Dl<'a> {
            dl: &'a str,
        }
        let data = Dl { dl: text };
        self.write_json_struct(&data);
    }

    /// Handle a completed slideshow image.
    /// - If slide_dir is set, saves the image to disk.
    /// - If slide_base64 is set, writes JSON with base64-encoded image to fd 3.
    pub fn write_slide(&mut self, file: &MotFile) {
        // Save to disk if slide_dir is configured
        if let Some(ref dir) = self.slide_dir {
            self.save_slide_to_dir(dir.clone(), file);
        }

        // Output as base64 JSON on fd 3 if enabled
        if self.slide_base64 {
            self.write_slide_base64(file);
        }
    }

    fn save_slide_to_dir(&self, dir: PathBuf, file: &MotFile) {
        let filename = if file.content_name.is_empty() {
            format!("slide.{}", file.extension())
        } else {
            sanitize_filename(&file.content_name)
        };

        let path = dir.join(&filename);
        match std::fs::write(&path, &file.data) {
            Ok(_) => {
                tracing::info!("Slide saved: {}", path.display());
            }
            Err(e) => {
                tracing::warn!("Failed to save slide to {}: {}", path.display(), e);
            }
        }
    }

    fn write_slide_base64(&mut self, file: &MotFile) {
        #[derive(Serialize)]
        struct Slide<'a> {
            #[serde(rename = "contentName")]
            content_name: &'a str,
            #[serde(rename = "contentType")]
            content_type: &'a str,
            data: String,
        }
        #[derive(Serialize)]
        struct Wrapper<'a> {
            slide: Slide<'a>,
        }
        let b64 = base64::engine::general_purpose::STANDARD.encode(&file.data);
        let slide = Slide {
            content_name: &file.content_name,
            content_type: file.mime_type(),
            data: b64,
        };
        let data = Wrapper { slide };
        self.write_json_struct(&data);
    }

    fn write_json_struct<T: serde::Serialize>(&mut self, value: &T) {
        if let Some(ref mut writer) = self.writer {
            match serde_json::to_string(value) {
                Ok(json) => {
                    if let Err(e) = writeln!(writer, "{}", json) {
                        tracing::warn!("Metadata write to fd 3 failed: {e}");
                    }
                    if let Err(e) = writer.flush() {
                        tracing::warn!("Metadata flush on fd 3 failed: {e}");
                    }
                }
                Err(e) => {
                    tracing::warn!("Metadata JSON serialization failed: {e}");
                }
            }
        }
    }
}

/// Check if a file descriptor is valid using fcntl
fn nix_fcntl_check(fd: i32) -> bool {
    unsafe { libc::fcntl(fd, libc::F_GETFD) != -1 }
}

/// Sanitize a filename: remove path components and unsafe characters.
fn sanitize_filename(name: &str) -> String {
    let base = Path::new(name)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("slide.bin");

    base.chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '.' || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sanitize_filename_basic() {
        assert_eq!(sanitize_filename("logo.jpg"), "logo.jpg");
        assert_eq!(sanitize_filename("my slide.png"), "my_slide.png");
    }

    #[test]
    fn test_sanitize_filename_path_traversal() {
        assert_eq!(sanitize_filename("../../etc/passwd"), "passwd");
        assert_eq!(sanitize_filename("/tmp/evil.jpg"), "evil.jpg");
    }

    #[test]
    fn test_sanitize_filename_special_chars() {
        assert_eq!(sanitize_filename("file<>:\"|?.jpg"), "file______.jpg");
    }
}
