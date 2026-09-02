//! Metadata emitter: writes JSONL events to file descriptor 3 (FD 3).
//!
//! Output contract:
//!   - FD 3 MUST be opened by the caller (shell redirect: `3>meta.json`)
//!   - Each event is one JSON line terminated by `\n`
//!   - NEVER writes to stdout or stderr

use serde_json::json;
#[cfg(not(target_arch = "wasm32"))]
use std::os::unix::io::FromRawFd;

use crate::dablin::utils::jsonl::write_jsonl;

/// Metadata emitter backed by FD 3.
pub struct MetadataEmitter {
    writer: std::fs::File,
}

/// Audio metadata payload for DAB+ decoded stream.
pub struct AudioMeta<'a> {
    pub codec: &'a str,
    pub channels: u8,
    pub mode: &'a str,
    pub sample_rate: u32,
    pub bitrate: Option<u32>,
    pub sbr: bool,
    pub ps: bool,
}

impl MetadataEmitter {
    /// Open FD 3 for writing. Returns `Err` if FD 3 is not available.
    ///
    /// # Safety
    /// FD 3 must be opened by the shell before dabctl starts (e.g. `3>meta.json`).
    #[cfg(not(target_arch = "wasm32"))]
    pub fn open() -> anyhow::Result<Self> {
        // Guard against invalid/closed FD 3 before taking ownership.
        let fd = 3;
        let rc = unsafe { libc::fcntl(fd, libc::F_GETFD) };
        if rc == -1 {
            let err = std::io::Error::last_os_error();
            if err.raw_os_error() == Some(libc::EBADF) {
                anyhow::bail!("fd3 metadata emitter is not available: fd 3 is not open");
            }
            return Err(err.into());
        }

        // SAFETY: FD 3 is expected to be opened by the shell before dabctl starts.
        // We take ownership only after validating the descriptor is still valid.
        let writer = unsafe { std::fs::File::from_raw_fd(fd) };
        Ok(Self { writer })
    }

    /// WebAssembly targets do not expose POSIX file descriptors.
    #[cfg(target_arch = "wasm32")]
    pub fn open() -> anyhow::Result<Self> {
        anyhow::bail!("fd3 metadata emitter is not available on wasm32")
    }

    /// Serialize a JSON value and emit it as one line.
    fn emit(&mut self, value: serde_json::Value) {
        write_jsonl(&mut self.writer, value);
    }

    /// Emit ensemble information.
    pub fn emit_ensemble(&mut self, eid: u16, label: Option<&str>, short_label: Option<&str>) {
        let mut ensemble = json!({"eid": format!("{:#06x}", eid)});
        if let Some(l) = label {
            ensemble["label"] = json!(l);
        }
        if let Some(s) = short_label {
            ensemble["shortLabel"] = json!(s);
        }
        let v = json!({"ensemble": ensemble});
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

    /// Emit sub-channel information discovered from FIG.
    pub fn emit_subchannel(&mut self, scid: u8, protection: Option<&str>, dabplus: bool) {
        let mut subchannel = json!({
            "id": scid,
            "dabplus": dabplus,
        });
        if let Some(p) = protection {
            subchannel["protection"] = json!(p);
        }
        self.emit(json!({"subchannel": subchannel}));
    }

    /// Emit decoded DAB+ audio format information.
    pub fn emit_audio(&mut self, audio_meta: AudioMeta<'_>) {
        let mut audio = json!({
            "codec": audio_meta.codec,
            "channels": audio_meta.channels,
            "mode": audio_meta.mode,
            "sampleRate": audio_meta.sample_rate,
            "sbr": audio_meta.sbr,
            "ps": audio_meta.ps,
        });
        if let Some(kbps) = audio_meta.bitrate {
            audio["bitrate"] = json!(kbps);
        }
        self.emit(json!({
            "audio": audio
        }));
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

    #[test]
    fn test_metadata_fd3_missing_is_graceful() {
        let _ = unsafe { libc::close(3) };
        assert!(super::MetadataEmitter::open().is_err());
    }
}
