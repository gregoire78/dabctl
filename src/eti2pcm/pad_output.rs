/// PAD JSON output on file descriptor 3 (compatible with dablin-gregoire format).
/// Also handles slideshow file saving and base64 JSON output.
use crate::eti2pcm::mot_manager::MotFile;
use base64::Engine;
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
    /// Create a new PadOutput that writes to fd 3.
    /// Returns a writer that silently drops data if fd 3 is not open.
    pub fn new(slide_dir: Option<PathBuf>, slide_base64: bool) -> Self {
        // Check if fd 3 is available before taking ownership
        let writer = if nix_fcntl_check(3) {
            Some(unsafe { std::fs::File::from_raw_fd(3) })
        } else {
            None
        };

        // Create slide directory if specified
        if let Some(ref dir) = slide_dir {
            if let Err(e) = std::fs::create_dir_all(dir) {
                tracing::warn!("Cannot create slide directory {:?}: {}", dir, e);
            }
        }

        PadOutput { writer, slide_dir, slide_base64 }
    }

    /// Write ensemble info as JSON
    pub fn write_ensemble(&mut self, label: &str, short_label: &str, eid: u16) {
        self.write_json(&format!(
            "{{\"ensemble\":{{\"label\":\"{}\",\"shortLabel\":\"{}\",\"eid\":\"0x{:04X}\"}}}}",
            escape_json(label),
            escape_json(short_label),
            eid
        ));
    }

    /// Write service info as JSON
    pub fn write_service(&mut self, label: &str, short_label: &str, sid: u16) {
        self.write_json(&format!(
            "{{\"service\":{{\"label\":\"{}\",\"shortLabel\":\"{}\",\"sid\":\"0x{:04X}\"}}}}",
            escape_json(label),
            escape_json(short_label),
            sid
        ));
    }

    /// Write dynamic label (DLS) as JSON
    pub fn write_dl(&mut self, text: &str) {
        self.write_json(&format!(
            "{{\"dl\":\"{}\"}}",
            escape_json(text)
        ));
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
        let b64 = base64::engine::general_purpose::STANDARD.encode(&file.data);
        let content_name = escape_json(&file.content_name);
        let mime = file.mime_type();
        self.write_json(&format!(
            "{{\"slide\":{{\"contentName\":\"{}\",\"contentType\":\"{}\",\"data\":\"{}\"}}}}",
            content_name, mime, b64
        ));
    }

    fn write_json(&mut self, json: &str) {
        if let Some(ref mut writer) = self.writer {
            let _ = writeln!(writer, "{}", json);
            let _ = writer.flush();
        }
    }
}

/// Check if a file descriptor is valid using fcntl
fn nix_fcntl_check(fd: i32) -> bool {
    unsafe { libc::fcntl(fd, libc::F_GETFD) != -1 }
}

/// Escape special JSON characters
fn escape_json(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    for ch in s.chars() {
        match ch {
            '"' => result.push_str("\\\""),
            '\\' => result.push_str("\\\\"),
            '\n' => result.push_str("\\n"),
            '\r' => result.push_str("\\r"),
            '\t' => result.push_str("\\t"),
            c if (c as u32) < 0x20 => {
                result.push_str(&format!("\\u{:04x}", c as u32));
            }
            c => result.push(c),
        }
    }
    result
}

/// Sanitize a filename: remove path components and unsafe characters.
fn sanitize_filename(name: &str) -> String {
    let base = Path::new(name)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("slide.bin");

    base.chars()
        .map(|c| if c.is_alphanumeric() || c == '.' || c == '-' || c == '_' { c } else { '_' })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_escape_json_simple() {
        assert_eq!(escape_json("hello"), "hello");
    }

    #[test]
    fn test_escape_json_quotes() {
        assert_eq!(escape_json("say \"hi\""), "say \\\"hi\\\"");
    }

    #[test]
    fn test_escape_json_backslash() {
        assert_eq!(escape_json("a\\b"), "a\\\\b");
    }

    #[test]
    fn test_escape_json_control_chars() {
        assert_eq!(escape_json("a\nb"), "a\\nb");
        assert_eq!(escape_json("a\tb"), "a\\tb");
    }

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
