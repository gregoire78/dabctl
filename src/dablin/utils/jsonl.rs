use serde_json::Value;
use std::io::{BufWriter, Write};
use tracing::warn;

pub(crate) fn write_jsonl(writer: &mut BufWriter<std::fs::File>, value: Value) {
    match serde_json::to_string(&value) {
        Ok(line) => {
            if let Err(e) = writeln!(writer, "{}", line) {
                warn!("metadata file write error: {}", e);
            }
        }
        Err(e) => warn!("metadata JSON serialize error: {}", e),
    }
}

#[cfg(test)]
mod tests {
    use super::write_jsonl;
    use serde_json::json;
    use std::fs;
    use std::io::{BufWriter, Write};
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_file_path(prefix: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        std::env::temp_dir().join(format!(
            "dabctl-{}-{}-{}.tmp",
            prefix,
            std::process::id(),
            nanos
        ))
    }

    #[test]
    fn writes_one_json_line() {
        let path = temp_file_path("jsonl");
        let file = std::fs::File::create(&path).expect("create temp file");
        let mut writer = BufWriter::new(file);

        write_jsonl(&mut writer, json!({"bitrate": 88}));
        writer.flush().expect("flush writer");

        let content = fs::read_to_string(&path).expect("read temp file");
        assert_eq!(content, "{\"bitrate\":88}\n");

        let _ = fs::remove_file(path);
    }
}
