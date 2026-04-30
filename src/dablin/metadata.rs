//! Metadata emitter: writes JSONL events to file descriptor 3 (FD 3).
//!
//! Output contract:
//!   - FD 3 MUST be opened by the caller (shell redirect: `3>meta.json`)
//!   - Each event is one JSON line terminated by `\n`
//!   - NEVER writes to stdout or stderr

use std::io::Write;
use std::os::unix::io::FromRawFd;

/// Metadata emitter backed by FD 3.
#[allow(dead_code)]
pub struct MetadataEmitter {
    writer: std::fs::File,
}

#[allow(dead_code)]
impl MetadataEmitter {
    /// Open FD 3 for writing. Panics (propagated as Err) if FD 3 is not open.
    ///
    /// # Safety
    /// The caller must ensure FD 3 is valid and writable before calling this.
    pub fn open() -> anyhow::Result<Self> {
        // SAFETY: FD 3 is expected to be opened by the shell before dabctl starts.
        // We don't own the fd's lifetime – we just wrap it.
        let writer = unsafe { std::fs::File::from_raw_fd(3) };
        Ok(Self { writer })
    }

    /// Emit a raw JSON string as one line.
    fn emit(&mut self, json: &str) {
        if let Err(e) = writeln!(self.writer, "{}", json) {
            tracing::warn!("FD3 write error: {}", e);
        }
    }

    /// Emit ensemble information.
    pub fn emit_ensemble(&mut self, eid: u16, label: Option<&str>) {
        let label_str = label.map(|l| format!(r#","label":"{}""#, escape_json(l))).unwrap_or_default();
        self.emit(&format!(r#"{{"ensemble":{{"eid":"{:#06x}"{}}}}}"#, eid, label_str));
    }

    /// Emit service information.
    pub fn emit_service(&mut self, sid: u32, label: Option<&str>) {
        let label_str = label.map(|l| format!(r#","label":"{}""#, escape_json(l))).unwrap_or_default();
        self.emit(&format!(r#"{{"service":{{"sid":"{:#06x}"{}}}}}"#, sid, label_str));
    }

    /// Emit a dynamic label (DL) string.
    pub fn emit_dynamic_label(&mut self, text: &str) {
        self.emit(&format!(r#"{{"dl":"{}"}}"#, escape_json(text)));
    }

    /// Emit audio bitrate information.
    pub fn emit_bitrate(&mut self, kbps: u32) {
        self.emit(&format!(r#"{{"bitrate":{}}}"#, kbps));
    }

    /// Emit MOT slide metadata.
    pub fn emit_slide(&mut self, name: &str, content_type: &str, data_base64: &str) {
        self.emit(&format!(
            r#"{{"slide":{{"contentName":"{}","contentType":"{}","data":"{}"}}}}"#,
            escape_json(name),
            escape_json(content_type),
            data_base64,
        ));
    }
}

/// Minimal JSON string escaping (backslash, double-quote, control chars).
fn escape_json(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => {
                out.push_str(&format!("\\u{:04x}", c as u32));
            }
            c => out.push(c),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::escape_json;

    #[test]
    fn test_escape_json_plain() {
        assert_eq!(escape_json("hello"), "hello");
    }

    #[test]
    fn test_escape_json_quote() {
        assert_eq!(escape_json(r#"say "hi""#), r#"say \"hi\""#);
    }

    #[test]
    fn test_escape_json_backslash() {
        assert_eq!(escape_json("a\\b"), "a\\\\b");
    }

    #[test]
    fn test_escape_json_newline() {
        assert_eq!(escape_json("a\nb"), "a\\nb");
    }

    #[test]
    fn test_escape_json_tab() {
        assert_eq!(escape_json("a\tb"), "a\\tb");
    }

    #[test]
    fn test_escape_json_control_char() {
        // ASCII 0x01 = SOH
        assert_eq!(escape_json("\x01"), "\\u0001");
    }

    #[test]
    fn test_escape_json_unicode_ok() {
        // Unicode above 0x1F is passed through unchanged
        assert_eq!(escape_json("café"), "café");
        assert_eq!(escape_json("日本語"), "日本語");
    }
}
