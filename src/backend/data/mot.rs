#[derive(Debug, Clone, Eq, PartialEq)]
pub struct Slide {
    pub content_name: String,
    pub content_type: String,
    pub data: Vec<u8>,
}

impl Slide {
    pub fn new(
        content_name: impl Into<String>,
        content_type: impl Into<String>,
        data: Vec<u8>,
    ) -> Self {
        Self {
            content_name: content_name.into(),
            content_type: content_type.into(),
            data,
        }
    }
}
