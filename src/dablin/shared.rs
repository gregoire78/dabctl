use base64::Engine;

use crate::cli::DateTimeFormat;
use crate::dablin::dabplus::SuperframeFormat;
use crate::dablin::fic::{FicDecoder, ProtectionProfile};

pub type DateTimeMode<'a> = (bool, bool, Option<&'a str>);

pub fn datetime_mode_from_option(fmt: Option<&DateTimeFormat>) -> Option<DateTimeMode<'_>> {
    fmt.map(|fmt| {
        let custom_datetime_format = match fmt {
            DateTimeFormat::Custom(pattern) => Some(pattern.as_str()),
            _ => None,
        };
        let use_iso8601_time = matches!(fmt, DateTimeFormat::Iso8601 | DateTimeFormat::TimeIso8601);
        let use_time_only = matches!(fmt, DateTimeFormat::TimeHuman | DateTimeFormat::TimeIso8601);
        (use_iso8601_time, use_time_only, custom_datetime_format)
    })
}

pub fn protection_label(p: &ProtectionProfile) -> String {
    match p {
        ProtectionProfile::EepA(level) => format!("EEP-{}A", level),
        ProtectionProfile::EepB(level) => format!("EEP-{}B", level),
        ProtectionProfile::Uep(index) => format!("UEP-{}", index),
    }
}

pub fn audio_codec_label(fmt: &SuperframeFormat) -> &'static str {
    match (fmt.sbr_flag, fmt.ps_flag) {
        (false, _) => "AAC-LC",
        (true, false) => "HE-AAC",
        (true, true) => "HE-AAC v2",
    }
}

pub fn audio_mode_label(fmt: &SuperframeFormat) -> &'static str {
    if fmt.core_ch_config() == 2 {
        "stereo"
    } else {
        "mono"
    }
}

pub fn current_subchannel_protection(fic: &FicDecoder, scid: u8) -> Option<String> {
    fic.subchannel_org(scid)
        .map(|s| protection_label(&s.protection))
}

pub fn hash_bytes(data: &[u8]) -> u64 {
    use std::hash::{Hash, Hasher};

    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    data.hash(&mut hasher);
    hasher.finish()
}

pub fn encode_slide_base64(data: &[u8], do_base64: bool) -> String {
    if do_base64 {
        base64::engine::general_purpose::STANDARD.encode(data)
    } else {
        String::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn protection_label_formats_eep_a() {
        let label = protection_label(&ProtectionProfile::EepA(3));
        assert_eq!(label, "EEP-3A");
    }

    #[test]
    fn current_subchannel_protection_none_when_unknown() {
        let fic = FicDecoder::new();
        assert_eq!(current_subchannel_protection(&fic, 3), None);
    }

    #[test]
    fn audio_codec_label_detects_he_aac_and_v2() {
        let v1 = SuperframeFormat {
            dac_rate: true,
            sbr_flag: true,
            aac_channel_mode: true,
            ps_flag: false,
            mpeg_surround_config: 0,
        };
        let v2 = SuperframeFormat {
            dac_rate: true,
            sbr_flag: true,
            aac_channel_mode: false,
            ps_flag: true,
            mpeg_surround_config: 0,
        };
        assert_eq!(audio_codec_label(&v1), "HE-AAC");
        assert_eq!(audio_codec_label(&v2), "HE-AAC v2");
        assert_eq!(audio_mode_label(&v1), "stereo");
        assert_eq!(audio_mode_label(&v2), "stereo");
    }

    #[test]
    fn hash_bytes_same_input_gives_same_hash() {
        let a = hash_bytes(b"hello");
        let b = hash_bytes(b"hello");
        assert_eq!(a, b);
    }

    #[test]
    fn hash_bytes_different_input_gives_different_hash() {
        let a = hash_bytes(b"slide1");
        let b = hash_bytes(b"slide2");
        assert_ne!(a, b);
    }

    #[test]
    fn encode_slide_base64_disabled_returns_empty() {
        let result = encode_slide_base64(b"some data", false);
        assert_eq!(result, "");
    }

    #[test]
    fn encode_slide_base64_enabled_returns_base64() {
        let result = encode_slide_base64(b"hello", true);
        assert_eq!(result, "aGVsbG8=");
    }

    #[test]
    fn datetime_mode_none_when_unset() {
        assert_eq!(datetime_mode_from_option(None), None);
    }

    #[test]
    fn datetime_mode_from_custom_pattern() {
        let fmt = DateTimeFormat::Custom("%H:%M".to_string());
        let mode = datetime_mode_from_option(Some(&fmt));
        assert_eq!(mode, Some((false, false, Some("%H:%M"))));
    }
}
