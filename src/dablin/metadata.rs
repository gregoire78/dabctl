//! Metadata emitter: writes JSONL events to file descriptor 3 (FD 3).
//!
//! Output contract:
//!   - FD 3 MUST be opened by the caller (shell redirect: `3>meta.json`)
//!   - Each event is one JSON line terminated by `\n`
//!   - NEVER writes to stdout or stderr

use serde_json::json;
use std::os::unix::io::FromRawFd;

use crate::dablin::utils::jsonl::write_jsonl;

/// Metadata emitter backed by FD 3.
pub struct MetadataEmitter {
    writer: std::fs::File,
}

impl MetadataEmitter {
    /// Open FD 3 for writing. Returns `Err` if FD 3 is not available.
    ///
    /// # Safety
    /// FD 3 must be opened by the shell before dabctl starts (e.g. `3>meta.json`).
    pub fn open() -> anyhow::Result<Self> {
        // SAFETY: FD 3 is expected to be opened by the shell before dabctl starts.
        // We don't own the fd's lifetime – we just wrap it.
        let writer = unsafe { std::fs::File::from_raw_fd(3) };
        Ok(Self { writer })
    }

    /// Serialize a JSON value and emit it as one line.
    fn emit(&mut self, value: serde_json::Value) {
        write_jsonl(&mut self.writer, value);
    }

    /// Emit ensemble information.
    pub fn emit_ensemble(&mut self, eid: u16, label: Option<&str>) {
        let v = match label {
            Some(l) => json!({"ensemble": {"eid": format!("{:#06x}", eid), "label": l}}),
            None => json!({"ensemble": {"eid": format!("{:#06x}", eid)}}),
        };
        self.emit(v);
    }

    /// Emit service information.
    pub fn emit_service(&mut self, sid: u32, label: Option<&str>) {
        let v = match label {
            Some(l) => json!({"service": {"sid": format!("{:#06x}", sid), "label": l}}),
            None => json!({"service": {"sid": format!("{:#06x}", sid)}}),
        };
        self.emit(v);
    }

    /// Emit a dynamic label (DL) string.
    pub fn emit_dynamic_label(&mut self, text: &str) {
        self.emit(json!({"dl": text}));
    }

    /// Emit audio bitrate information.
    pub fn emit_bitrate(&mut self, kbps: u32) {
        self.emit(json!({"bitrate": kbps}));
    }

    /// Emit DAB date/time information derived from FIG 0/9 + FIG 0/10.
    pub fn emit_time(&mut self, utc: &str, local: &str, lto: &str) {
        self.emit(json!({
            "time": {
                "utc": utc,
                "local": local,
                "lto": lto,
            }
        }));
    }

    /// Emit MOT slide metadata.
    pub fn emit_slide(&mut self, name: &str, content_type: &str, data_base64: &str) {
        self.emit(json!({
            "slide": {
                "contentName": name,
                "contentType": content_type,
                "data": data_base64,
            }
        }));
    }
}

#[cfg(test)]
mod tests {
    /// Vérifie que serde_json échappe correctement les caractères spéciaux.
    #[test]
    fn test_serde_json_escapes_quotes() {
        let v = serde_json::json!({"dl": r#"say "hi""#});
        let s = serde_json::to_string(&v).unwrap();
        assert!(s.contains(r#"say \"hi\""#));
    }

    #[test]
    fn test_serde_json_escapes_backslash() {
        let v = serde_json::json!({"dl": "a\\b"});
        let s = serde_json::to_string(&v).unwrap();
        assert!(s.contains(r#"a\\b"#));
    }

    #[test]
    fn test_serde_json_escapes_newline() {
        let v = serde_json::json!({"dl": "a\nb"});
        let s = serde_json::to_string(&v).unwrap();
        assert!(s.contains(r#"a\nb"#));
    }

    #[test]
    fn test_serde_json_unicode_passthrough() {
        let v = serde_json::json!({"dl": "café"});
        let s = serde_json::to_string(&v).unwrap();
        assert!(s.contains("café"));
    }

    #[test]
    fn test_serde_json_control_char() {
        let v = serde_json::json!({"dl": "\x01"});
        let s = serde_json::to_string(&v).unwrap();
        assert!(s.contains("\\u0001"));
    }

    #[test]
    fn test_emit_time_shape() {
        let v = serde_json::json!({
            "time": {
                "utc": "2023-02-25, Sat - 12:34:45.321",
                "local": "2023-02-25, Sat - 13:34:45",
                "lto": "+01:00"
            }
        });
        let s = serde_json::to_string(&v).unwrap();
        assert!(s.contains("\"time\""));
        assert!(s.contains("\"utc\""));
        assert!(s.contains("\"local\""));
        assert!(s.contains("\"lto\""));
    }
}
