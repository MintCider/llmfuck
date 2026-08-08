use regex::Regex;
use std::sync::LazyLock;

static ASSIGNMENT: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)(password|passwd|token|api[_-]?key|secret)(\s*[:=]\s*)([^\s,;]+)")
        .expect("valid regex")
});
static AUTH: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)(authorization\s*:\s*(?:bearer|basic)\s+)([^\s]+)").expect("valid regex")
});
static URL_AUTH: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"([a-zA-Z][a-zA-Z0-9+.-]*://[^\s:/@]+:)([^\s@]+)(@)").expect("valid regex")
});
static KNOWN_KEY: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\b(?:sk-[A-Za-z0-9_-]{16,}|gh[pousr]_[A-Za-z0-9_]{20,}|AKIA[0-9A-Z]{16})\b")
        .expect("valid regex")
});

pub fn redact(input: &str) -> String {
    let text = ASSIGNMENT.replace_all(input, "$1$2<REDACTED:SECRET>");
    let text = AUTH.replace_all(&text, "$1<REDACTED:TOKEN>");
    let text = URL_AUTH.replace_all(&text, "$1<REDACTED:PASSWORD>$3");
    KNOWN_KEY.replace_all(&text, "<REDACTED:KEY>").into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn removes_common_secrets() {
        assert_eq!(redact("token=abc123"), "token=<REDACTED:SECRET>");
        assert!(!redact("Authorization: Bearer sk-abcdefghijklmnop").contains("sk-"));
        assert!(!redact("https://me:hunter2@example.com").contains("hunter2"));
    }
}
