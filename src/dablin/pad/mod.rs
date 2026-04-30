use std::collections::BTreeMap;

use encoding_rs::WINDOWS_1252;

use crate::dablin::utils::ebu_latin::ebu_latin_bytes_to_utf8_string;

#[derive(Debug, Clone)]
pub struct PadSlide {
    pub content_name: String,
    pub content_type: String,
    pub data: Vec<u8>,
}

#[derive(Debug, Default)]
pub struct PadEvents {
    pub dynamic_label: Option<String>,
    pub slide: Option<PadSlide>,
}

#[derive(Debug, Clone, Copy)]
struct XpadCi {
    len: usize,
    r#type: u8,
}

#[derive(Debug, Clone)]
struct DlSegment {
    prefix0: u8,
    prefix1: u8,
    chars: Vec<u8>,
}

#[derive(Debug, Default)]
struct DlDecoder {
    segs: BTreeMap<u8, DlSegment>,
    last_toggle: Option<bool>,
}

impl DlDecoder {
    fn reset(&mut self) {
        self.segs.clear();
        self.last_toggle = None;
    }

    fn feed(&mut self, start: bool, data: &[u8]) -> Option<String> {
        if start {
            self.reset();
        }
        if data.len() < 4 {
            return None;
        }

        let command = (data[0] & 0x10) != 0;
        if command {
            // remove label command
            if (data[0] & 0x0f) == 0x01 {
                self.reset();
                return Some(String::new());
            }
            // Ignore DL+ command payload in this minimal decoder.
            return None;
        }

        let field_len = ((data[0] & 0x0f) as usize) + 1;
        let needed = 2 + field_len;
        if data.len() < needed {
            return None;
        }

        let seg = DlSegment {
            prefix0: data[0],
            prefix1: data[1],
            chars: data[2..2 + field_len].to_vec(),
        };

        let toggle = (seg.prefix0 & 0x80) != 0;
        if let Some(last_toggle) = self.last_toggle {
            if toggle != last_toggle {
                self.segs.clear();
            }
        }
        self.last_toggle = Some(toggle);

        let seg_num = if (seg.prefix0 & 0x40) != 0 {
            0
        } else {
            (seg.prefix1 >> 4) & 0x07
        };

        self.segs.entry(seg_num).or_insert(seg);

        let mut complete_chars = Vec::new();
        let mut found_last = false;
        for i in 0..8u8 {
            let seg_i = self.segs.get(&i)?;
            complete_chars.extend_from_slice(&seg_i.chars);
            if (seg_i.prefix0 & 0x20) != 0 {
                found_last = true;
                break;
            }
        }
        if !found_last {
            return None;
        }

        let charset = self
            .segs
            .get(&0)
            .map(|s| s.prefix1 >> 4)
            .unwrap_or(0);

        Some(convert_text_charset(&complete_chars, charset))
    }
}

#[derive(Debug, Default)]
struct MotDecoderSimple {
    dg_buf: Vec<u8>,
    dgli_len: usize,
}

impl MotDecoderSimple {
    fn feed(
        &mut self,
        start: bool,
        data: &[u8],
        dgli_len: usize,
        slide_counter: &mut u64,
    ) -> Option<PadSlide> {
        if start {
            self.dg_buf.clear();
            self.dgli_len = dgli_len;
        }
        self.dg_buf.extend_from_slice(data);

        if self.dgli_len == 0 || self.dg_buf.len() < self.dgli_len {
            return None;
        }

        let dg = self.dg_buf[..self.dgli_len].to_vec();
        self.dg_buf.drain(..self.dgli_len);

        if let Some((mime, ext, img)) = extract_image_from_mot_group(&dg) {
            *slide_counter += 1;
            return Some(PadSlide {
                content_name: format!("slide-{:06}.{}", *slide_counter, ext),
                content_type: mime.to_string(),
                data: img,
            });
        }

        None
    }
}

pub struct PadDecoder {
    dl: DlDecoder,
    mot: MotDecoderSimple,
    last_xpad_ci: Option<XpadCi>,
    dgli_len: usize,
    slide_counter: u64,
}

impl PadDecoder {
    pub fn new() -> Self {
        Self {
            dl: DlDecoder::default(),
            mot: MotDecoderSimple::default(),
            last_xpad_ci: None,
            dgli_len: 0,
            slide_counter: 0,
        }
    }

    pub fn process_au(&mut self, au: &[u8], mot_app_type: Option<u8>) -> PadEvents {
        let mut events = PadEvents::default();
        let Some((xpad, fpad, exact_xpad_len)) = extract_pad_from_au(au) else {
            return events;
        };

        // Undo reversed byte order (matches dablin PADDecoder::Process)
        let mut xpad_rev = xpad;
        xpad_rev.reverse();

        let fpad_type = fpad[0] >> 6;
        let xpad_ind = (fpad[0] & 0x30) >> 4;
        let ci_flag = (fpad[1] & 0x02) != 0;

        let ci_lens = [4usize, 6, 8, 12, 16, 24, 32, 48];
        let mut cis: Vec<XpadCi> = Vec::new();
        let mut cis_len = 0usize;

        if fpad_type == 0 {
            if ci_flag {
                match xpad_ind {
                    0b01 => {
                        if xpad_rev.is_empty() {
                            return events;
                        }
                        let t = xpad_rev[0] & 0x1f;
                        if t != 0 {
                            cis_len = 1;
                            cis.push(XpadCi { len: 3, r#type: t });
                        }
                    }
                    0b10 => {
                        for i in 0..4usize {
                            if xpad_rev.len() < i + 1 {
                                return events;
                            }
                            let raw = xpad_rev[i];
                            cis_len += 1;
                            let t = raw & 0x1f;
                            if t == 0 {
                                break;
                            }
                            cis.push(XpadCi {
                                len: ci_lens[(raw >> 5) as usize],
                                r#type: t,
                            });
                        }
                    }
                    _ => {}
                }
            } else if matches!(xpad_ind, 0b01 | 0b10) {
                if let Some(prev) = self.last_xpad_ci {
                    cis.push(prev);
                    cis_len = 0;
                }
            }
        }

        if cis.is_empty() {
            return events;
        }

        let announced_xpad_len = cis_len + cis.iter().map(|ci| ci.len).sum::<usize>();
        if announced_xpad_len > xpad_rev.len() {
            return events;
        }

        let mut usable_xpad_len = xpad_rev.len();
        if exact_xpad_len && announced_xpad_len < usable_xpad_len {
            // Generous behavior: ignore trailing pad bytes.
            usable_xpad_len = announced_xpad_len;
        }

        let mot_type = mot_app_type.unwrap_or(12);
        let mut offset = cis_len;
        let mut continued_ci: Option<XpadCi> = None;

        for ci in cis {
            if offset + ci.len > usable_xpad_len {
                break;
            }
            let sub = &xpad_rev[offset..offset + ci.len];
            match ci.r#type {
                1 => {
                    if sub.len() >= 2 {
                        self.dgli_len = ((sub[0] as usize) << 8) | (sub[1] as usize);
                    }
                    continued_ci = Some(XpadCi { len: ci.len, r#type: 1 });
                }
                2 | 3 => {
                    if let Some(label) = self.dl.feed(ci.r#type == 2, sub) {
                        events.dynamic_label = Some(label);
                    }
                    continued_ci = Some(XpadCi { len: ci.len, r#type: 3 });
                }
                t if t == mot_type || t == mot_type.saturating_add(1) => {
                    if let Some(slide) =
                        self.mot
                            .feed(t == mot_type, sub, self.dgli_len, &mut self.slide_counter)
                    {
                        events.slide = Some(slide);
                    }
                    continued_ci = Some(XpadCi {
                        len: ci.len,
                        r#type: mot_type.saturating_add(1),
                    });
                }
                _ => {
                    continued_ci = None;
                }
            }
            offset += ci.len;
        }

        self.last_xpad_ci = continued_ci;
        events
    }
}

fn extract_pad_from_au(au: &[u8]) -> Option<(Vec<u8>, [u8; 2], bool)> {
    if au.len() < 3 {
        return None;
    }

    for i in 0..(au.len() - 2) {
        if (au[i] >> 5) != 4 {
            continue;
        }

        let mut pad_start = i + 2;
        let mut pad_len = au[i + 1] as usize;
        if pad_len == 255 {
            if i + 2 >= au.len() {
                continue;
            }
            pad_len += au[i + 2] as usize;
            pad_start += 1;
        }

        if pad_len < 2 || au.len() < pad_start + pad_len {
            continue;
        }

        let xpad_len = pad_len - 2;
        let xpad = au[pad_start..pad_start + xpad_len].to_vec();
        let fpad = [au[pad_start + xpad_len], au[pad_start + xpad_len + 1]];
        return Some((xpad, fpad, true));
    }

    None
}

fn convert_text_charset(raw: &[u8], charset: u8) -> String {
    if charset == 0 {
        let s = ebu_latin_bytes_to_utf8_string(raw);
        return s.trim_matches(char::from(0)).trim().to_string();
    }

    if let Ok(s) = std::str::from_utf8(raw) {
        return s.trim_matches(char::from(0)).trim().to_string();
    }

    let (cow, _, _) = WINDOWS_1252.decode(raw);
    cow.trim_matches(char::from(0)).trim().to_string()
}

fn extract_image_from_mot_group(dg: &[u8]) -> Option<(&'static str, &'static str, Vec<u8>)> {
    // JPEG
    if let Some(start) = dg.windows(2).position(|w| w == [0xff, 0xd8]) {
        if let Some(end_rel) = dg[start + 2..].windows(2).position(|w| w == [0xff, 0xd9]) {
            let end = start + 2 + end_rel + 2;
            return Some(("image/jpeg", "jpg", dg[start..end].to_vec()));
        }
    }

    // PNG
    let sig = [0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a];
    if let Some(start) = dg.windows(sig.len()).position(|w| w == sig) {
        let iend = [b'I', b'E', b'N', b'D'];
        if let Some(pos) = dg[start + sig.len()..].windows(4).position(|w| w == iend) {
            let end = start + sig.len() + pos + 8;
            if end <= dg.len() {
                return Some(("image/png", "png", dg[start..end].to_vec()));
            }
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_pad_from_au_none_on_non_dse() {
        let au = [0x00, 0x00, 0x00];
        assert!(extract_pad_from_au(&au).is_none());
    }

    #[test]
    fn test_extract_pad_from_au_basic() {
        let au = [0x80, 0x04, 0x11, 0x22, 0xaa, 0xbb];
        let out = extract_pad_from_au(&au).unwrap();
        assert_eq!(out.0, vec![0x11, 0x22]);
        assert_eq!(out.1, [0xaa, 0xbb]);
    }

    #[test]
    fn test_dl_reassembly_simple() {
        let mut dl = DlDecoder::default();
        // prefix0: first+last, field_len nibble=1 => 2 chars
        let data = [0x61, 0x00, b'O', b'K'];
        let txt = dl.feed(true, &data).unwrap();
        assert_eq!(txt, "OK");
    }
}
