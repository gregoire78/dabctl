pub(crate) fn sanitize_for_path(label: &str) -> String {
    let mut out = String::with_capacity(label.len());
    for c in label.chars() {
        if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
            out.push(c);
        } else if c.is_whitespace() {
            out.push('_');
        }
    }
    let trimmed = out.trim_matches('_');
    if trimmed.is_empty() {
        "no-label".to_string()
    } else {
        trimmed.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::sanitize_for_path;

    #[test]
    fn keeps_safe_ascii_chars() {
        assert_eq!(sanitize_for_path("NRJ-Paris_1"), "NRJ-Paris_1");
    }

    #[test]
    fn converts_whitespace_to_underscores() {
        assert_eq!(sanitize_for_path("France Inter Live"), "France_Inter_Live");
    }

    #[test]
    fn trims_leading_and_trailing_underscores() {
        assert_eq!(sanitize_for_path("  NRJ  "), "NRJ");
    }

    #[test]
    fn falls_back_when_no_supported_chars() {
        assert_eq!(sanitize_for_path("***"), "no-label");
        assert_eq!(sanitize_for_path("   "), "no-label");
    }
}
