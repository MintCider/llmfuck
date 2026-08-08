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
