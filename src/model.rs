use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize)]
pub struct SuggestionContext {
    pub command: String,
    pub exit_code: Option<i32>,
    pub succeeded: Option<bool>,
    pub shell: String,
    pub os: String,
    pub cwd: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub terminal_output: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub executable_candidates: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub path_candidates: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub git: Option<GitContext>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub project_commands: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct GitContext {
    pub branch: Option<String>,
    pub upstream: Option<String>,
    pub ahead: Option<u32>,
    pub behind: Option<u32>,
    pub staged: u32,
    pub modified: u32,
    pub untracked: u32,
    pub conflicted: u32,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Risk {
    Low,
    Medium,
    #[serde(other)]
    #[default]
    High,
}

impl Risk {
    pub fn max(self, other: Self) -> Self {
        use Risk::*;
        match (self, other) {
            (High, _) | (_, High) => High,
            (Medium, _) | (_, Medium) => Medium,
            _ => Low,
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Candidate {
    pub command: String,
    #[serde(default)]
    pub effect: String,
    #[serde(default)]
    pub risk: Risk,
    #[serde(default)]
    pub risk_reason: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct CandidateResponse {
    pub candidates: Vec<Candidate>,
}
