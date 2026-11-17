pub mod keychain;

/// Remove anything that looks like a credential from a message before it is
/// logged, stored (e.g. `last_error`) or shown in the UI.
pub fn redact(message: &str, secrets: &[&str]) -> String {
    let mut out = message.to_string();
    for s in secrets.iter().filter(|s| s.len() >= 8) {
        out = out.replace(*s, "[REDACTED]");
    }
    // Generic patterns: common key prefixes followed by a long token.
    let prefixes = ["sk-ant-", "sk-proj-", "sk-or-", "sk-admin-", "sk-", "gsk_", "AIza", "Bearer "];
    let mut result = String::with_capacity(out.len());
    let mut rest = out.as_str();
    let mut prev_word_char = false;
    'outer: while !rest.is_empty() {
        for p in prefixes {
            if !prev_word_char && rest.starts_with(p) {
                let tail = &rest[p.len()..];
                let tok_len = tail
                    .char_indices()
                    .find(|(_, c)| !(c.is_ascii_alphanumeric() || *c == '-' || *c == '_' || *c == '*'))
                    .map(|(i, _)| i)
                    .unwrap_or(tail.len());
                if tok_len >= 12 {
                    result.push_str(p.trim_end());
                    result.push_str(if p == "Bearer " { " [REDACTED]" } else { "[REDACTED]" });
                    rest = &tail[tok_len..];
                    continue 'outer;
                }
            }
        }
        let ch = rest.chars().next().unwrap();
        prev_word_char = ch.is_alphanumeric() || ch == '-' || ch == '_';
        result.push(ch);
        rest = &rest[ch.len_utf8()..];
    }
    // Keep messages short; provider HTML error pages can be huge.
    if result.chars().count() > 400 {
        result = result.chars().take(400).collect::<String>() + "…";
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redacts_known_secret_and_patterns() {
        let msg = "bad key sk-ant-api03-abcdefghijklmnopqrstuvwxyz and AIzaSyA1234567890abcdefgh and my-secret-value";
        let r = redact(msg, &["my-secret-value"]);
        assert!(!r.contains("abcdefghijklmnop"));
        assert!(!r.contains("SyA1234567890"));
        assert!(!r.contains("my-secret-value"));
        assert!(r.contains("bad key"));
    }

    #[test]
    fn leaves_short_tokens() {
        assert_eq!(redact("task-123 ok", &[]), "task-123 ok");
        let w = "risk-assessment-procedure-document";
        assert_eq!(redact(w, &[]), w);
    }

    #[test]
    fn truncates() {
        let long = "x".repeat(1000);
        assert!(redact(&long, &[]).chars().count() <= 401);
    }
}
