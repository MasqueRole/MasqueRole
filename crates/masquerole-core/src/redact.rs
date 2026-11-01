//! Keeping API keys out of logs.
//!
//! Code that handles a key calls [`register_secret`]; anything that writes text a person
//! might read later (the log file, for example) passes it through [`redact`] first.

use std::sync::Mutex;

/// What a secret is replaced with.
pub const REDACTED: &str = "[redacted]";

/// Secrets shorter than this are not tracked: replacing them would mangle ordinary text.
const MIN_SECRET_LEN: usize = 4;

static SECRETS: Mutex<Vec<String>> = Mutex::new(Vec::new());

/// Remember `secret` so [`redact`] removes it from text. Short or empty values are ignored.
pub fn register_secret(secret: &str) {
    let secret = secret.trim();
    if secret.len() < MIN_SECRET_LEN {
        return;
    }
    // A poisoned lock only means another thread panicked; the list itself is still sound.
    let mut secrets = SECRETS.lock().unwrap_or_else(|e| e.into_inner());
    if !secrets.iter().any(|s| s == secret) {
        secrets.push(secret.to_string());
    }
}

/// `text` with every registered secret, and every `Bearer` token, replaced by
/// [`REDACTED`].
pub fn redact(text: &str) -> String {
    let mut out = text.to_string();
    {
        let secrets = SECRETS.lock().unwrap_or_else(|e| e.into_inner());
        for secret in secrets.iter() {
            out = out.replace(secret.as_str(), REDACTED);
        }
    }
    redact_bearer(&out)
}

/// Replace the token after each `Bearer ` (any case) with [`REDACTED`].
fn redact_bearer(text: &str) -> String {
    const MARKER: &str = "bearer ";
    let lower = text.to_ascii_lowercase();
    let mut out = String::with_capacity(text.len());
    let mut rest = 0;
    while let Some(found) = lower[rest..].find(MARKER) {
        let start = rest + found + MARKER.len();
        let end = text[start..]
            .find(|c: char| c.is_whitespace() || matches!(c, '"' | '\'' | ',' | ';' | ')'))
            .map_or(text.len(), |n| start + n);
        out.push_str(&text[rest..start]);
        out.push_str(REDACTED);
        rest = end;
    }
    out.push_str(&text[rest..]);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registered_secrets_are_removed_everywhere() {
        register_secret("  sk-test-redact-1  ");
        let text = "key sk-test-redact-1 and again sk-test-redact-1.";
        assert_eq!(redact(text), "key [redacted] and again [redacted].");
    }

    #[test]
    fn short_secrets_are_ignored() {
        register_secret("ab");
        register_secret("");
        assert_eq!(redact("a b ab"), "a b ab");
    }

    #[test]
    fn bearer_tokens_are_removed_even_when_unregistered() {
        assert_eq!(
            redact("authorization: Bearer abc.def-123 next"),
            "authorization: Bearer [redacted] next"
        );
        assert_eq!(
            redact(r#"{"h":"BEARER tok"}"#),
            r#"{"h":"BEARER [redacted]"}"#
        );
    }

    #[test]
    fn text_without_secrets_is_unchanged() {
        let text = "Plain ünïcode text with bearer-like words, but no token.";
        assert_eq!(redact(text), text);
    }

    #[test]
    fn redacting_twice_changes_nothing() {
        let once = redact("Bearer abc");
        assert_eq!(redact(&once), once);
    }
}
