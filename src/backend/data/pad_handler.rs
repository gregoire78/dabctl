use anyhow::Result;

use super::mot::Slide;

// ETSI EN 300 401 §7.4 / §8: PAD and X-PAD are carried alongside audio frames.
#[derive(Default)]
pub struct PadHandler {
    last_dynamic_label: Option<String>,
    last_slide: Option<Slide>,
}

impl PadHandler {
    pub fn process_pad(&mut self, payload: &[u8]) -> Result<()> {
        if let Ok(text) = std::str::from_utf8(payload) {
            let trimmed = text.trim_matches(char::from(0)).trim();
            if !trimmed.is_empty() {
                self.last_dynamic_label = Some(trimmed.to_string());
            }
        }
        Ok(())
    }

    pub fn accept_slide(&mut self, slide: Slide) {
        self.last_slide = Some(slide);
    }

    pub fn has_slide(&self) -> bool {
        self.last_slide.is_some()
    }

    pub fn last_dynamic_label(&self) -> Option<&str> {
        self.last_dynamic_label.as_deref()
    }
}
