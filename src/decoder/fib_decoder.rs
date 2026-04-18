// Literal placeholder for DABstar's FibDecoder / FIG service database handling.
#[derive(Default)]
pub struct FibDecoder {
    services_seen: usize,
    figs_seen: usize,
    ensemble_label: Option<String>,
    service_labels: Vec<String>,
}

impl FibDecoder {
    pub fn process_fib(&mut self, fib: &[u8; 32]) {
        let mut processed = 0usize;
        while processed < 30 {
            let fig_header = fib[processed];
            if fig_header == 0xFF {
                break;
            }

            let fig_type = fig_header >> 5;
            let fig_length = (fig_header & 0x1F) as usize;
            let start = processed + 1;
            let end = start.saturating_add(fig_length).min(30);
            let payload = &fib[start..end];
            self.figs_seen += 1;

            match fig_type {
                0 => {
                    self.services_seen += 1;
                }
                1 => {
                    let printable = payload
                        .iter()
                        .copied()
                        .filter(|byte| byte.is_ascii_graphic() || *byte == b' ')
                        .map(char::from)
                        .collect::<String>()
                        .trim()
                        .to_string();
                    if !printable.is_empty() {
                        if self.ensemble_label.is_none() {
                            self.ensemble_label = Some(printable);
                        } else {
                            self.service_labels.push(printable);
                        }
                    }
                }
                _ => {}
            }

            processed = end;
        }
    }

    pub fn service_count(&self) -> usize {
        self.services_seen.max(self.service_labels.len())
    }

    pub fn ensemble_label(&self) -> Option<&str> {
        self.ensemble_label.as_deref()
    }

    pub fn first_service_label(&self) -> Option<&str> {
        self.service_labels.first().map(String::as_str)
    }
}
