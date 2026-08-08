use anyhow::{Result, bail};
use regex::{Captures, Regex};
use std::sync::LazyLock;

static ASSIGNMENT: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)(password|passwd|token|api[_-]?key|secret)(\s*[:=]\s*)([^\s,;]+)")
        .expect("valid regex")
});
static AUTH: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"(?i)(authorization\s*:\s*(?:bearer|basic)\s+)([^\s'\"]+)"#).expect("valid regex")
});
static URL_AUTH: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"([a-zA-Z][a-zA-Z0-9+.-]*://[^\s:/@]+:)([^\s@]+)(@)").expect("valid regex")
});
static KNOWN_KEY: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\b(?:sk-[A-Za-z0-9_-]{16,}|gh[pousr]_[A-Za-z0-9_]{20,}|AKIA[0-9A-Z]{16})\b")
        .expect("valid regex")
});
static PRIVATE_KEY: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?s)-----BEGIN [A-Z0-9 ]*PRIVATE KEY-----.*?-----END [A-Z0-9 ]*PRIVATE KEY-----")
        .expect("valid regex")
});

#[derive(Debug, Default)]
pub struct SecretMap(Vec<(String, String)>);

pub fn redact_command(input: &str) -> (String, SecretMap) {
    let mut secrets = SecretMap::default();
    let text = replace_capture(input, &ASSIGNMENT, 3, &mut secrets);
    let text = replace_capture(&text, &AUTH, 2, &mut secrets);
    let text = replace_capture(&text, &URL_AUTH, 2, &mut secrets);
    let text = replace_capture(&text, &KNOWN_KEY, 0, &mut secrets);
    (text, secrets)
}

pub fn restore_command(command: &str, secrets: &SecretMap) -> Result<String> {
    let mut restored = command.to_string();
    for (placeholder, secret) in &secrets.0 {
        match restored.matches(placeholder).count() {
            0 => {}
            1 => restored = restored.replace(placeholder, secret),
            _ => bail!("candidate repeats a sensitive command placeholder"),
        }
    }
    if restored.contains("<LLMFUCK_COMMAND_SECRET_") {
        bail!("candidate contains an unknown sensitive command placeholder");
    }
    Ok(restored)
}

fn replace_capture(input: &str, pattern: &Regex, group: usize, secrets: &mut SecretMap) -> String {
    pattern
        .replace_all(input, |captures: &Captures<'_>| {
            let whole = captures.get(0).expect("complete match");
            let secret = captures.get(group).expect("secret capture");
            let placeholder = format!("<LLMFUCK_COMMAND_SECRET_{}>", secrets.0.len() + 1);
            secrets
                .0
                .push((placeholder.clone(), secret.as_str().to_string()));
            let start = secret.start() - whole.start();
            let end = secret.end() - whole.start();
            format!(
                "{}{}{}",
                &whole.as_str()[..start],
                placeholder,
                &whole.as_str()[end..]
            )
        })
        .into_owned()
}

pub fn redact(input: &str) -> String {
    let text = ASSIGNMENT.replace_all(input, "$1$2<REDACTED:SECRET>");
    let text = AUTH.replace_all(&text, "$1<REDACTED:TOKEN>");
    let text = URL_AUTH.replace_all(&text, "$1<REDACTED:PASSWORD>$3");
    let text = KNOWN_KEY.replace_all(&text, "<REDACTED:KEY>");
    PRIVATE_KEY
        .replace_all(&text, "<REDACTED:PRIVATE_KEY>")
        .into_owned()
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

    #[test]
    fn restores_only_known_command_placeholders() {
        let original = "curl -H 'Authorization: Bearer secret-value' example.com";
        let (redacted, secrets) = redact_command(original);
        assert!(!redacted.contains("secret-value"));
        assert_eq!(restore_command(&redacted, &secrets).unwrap(), original);
        assert!(restore_command("echo <LLMFUCK_COMMAND_SECRET_99>", &secrets).is_err());
    }

    #[test]
    fn rejects_repeated_command_placeholder() {
        let (_, secrets) = redact_command("token=secret-value");
        assert!(
            restore_command(
                "echo <LLMFUCK_COMMAND_SECRET_1> <LLMFUCK_COMMAND_SECRET_1>",
                &secrets
            )
            .is_err()
        );
    }

    #[test]
    fn removes_private_keys_from_output() {
        let value = "-----BEGIN PRIVATE KEY-----\nsecret\n-----END PRIVATE KEY-----";
        assert_eq!(redact(value), "<REDACTED:PRIVATE_KEY>");
    }
}
