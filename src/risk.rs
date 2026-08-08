use regex::Regex;
use std::sync::LazyLock;

use crate::model::{Candidate, Risk};

static HIGH: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)(^|[|;&]\s*|\s)(sudo\s+|rm\s+.*(?:-[^\s]*r|--recursive)|mkfs(?:\.|\s)|dd\s+.*\bof=|git\s+(?:reset\s+--hard|clean\s+-|push\s+.*--force)|curl\b.*\|\s*(?:sh|bash|zsh)|wget\b.*\|\s*(?:sh|bash|zsh)|remove-item\b.*(?:-recurse|-force)|invoke-expression\b|\biex\b)")
        .expect("valid risk regex")
});

pub fn enforce(candidate: &mut Candidate) {
    let local = if HIGH.is_match(&candidate.command) || has_overwrite_redirect(&candidate.command) {
        Risk::High
    } else {
        Risk::Low
    };
    candidate.risk = candidate.risk.clone().max(local);
    if candidate.command.contains(['\n', '\r', '\0']) || candidate.command.len() > 8_192 {
        candidate.risk = Risk::High;
        candidate
            .risk_reason
            .get_or_insert_with(|| "The command contains unusual control data".into());
    }
    candidate.effect = candidate.effect.trim().trim_end_matches('.').to_string();
    if candidate.effect.is_empty() {
        candidate.effect = "Run this command".to_string();
    }
}

fn has_overwrite_redirect(command: &str) -> bool {
    let bytes = command.as_bytes();
    bytes.iter().enumerate().any(|(index, byte)| {
        *byte == b'>'
            && bytes.get(index.wrapping_sub(1)) != Some(&b'>')
            && bytes.get(index + 1) != Some(&b'>')
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn local_detection_only_upgrades_risk() {
        let mut candidate = Candidate {
            command: "rm -rf build".into(),
            effect: "Delete the build directory.".into(),
            risk: Risk::Low,
            risk_reason: None,
        };
        enforce(&mut candidate);
        assert_eq!(candidate.risk, Risk::High);
        assert_eq!(candidate.effect, "Delete the build directory");
    }

    #[test]
    fn unknown_or_missing_model_risk_defaults_high() {
        let unknown: Candidate = serde_json::from_str(
            r#"{"command":"echo ok","effect":"Print ok","risk":"unexpected"}"#,
        )
        .unwrap();
        let missing: Candidate =
            serde_json::from_str(r#"{"command":"echo ok","effect":"Print ok"}"#).unwrap();
        assert_eq!(unknown.risk, Risk::High);
        assert_eq!(missing.risk, Risk::High);
    }
}
