use std::{
    collections::HashSet,
    env, fs,
    io::Read,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant},
};

use serde_json::Value;

use crate::{
    config::PrivacyMode,
    model::{GitContext, SuggestionContext},
    redact,
};

pub fn collect(
    command: String,
    exit_code: Option<i32>,
    succeeded: Option<bool>,
    shell: String,
    cwd: PathBuf,
    privacy: PrivacyMode,
    terminal_output: Option<String>,
) -> (SuggestionContext, redact::SecretMap) {
    let (command, secrets) = redact::redact_command(&command);
    let mut context = SuggestionContext {
        command,
        intent: None,
        exit_code,
        succeeded,
        shell,
        os: env::consts::OS.to_string(),
        cwd: private_cwd(&cwd),
        terminal_output: None,
        executable_candidates: Vec::new(),
        path_candidates: Vec::new(),
        git: None,
        project_commands: Vec::new(),
    };
    if matches!(privacy, PrivacyMode::Minimal) {
        return (context, secrets);
    }

    context.terminal_output = terminal_output.map(|v| redact::redact(&limit_tail(&v, 16_384)));
    context.executable_candidates = executable_candidates(&context.command);
    context.path_candidates = path_candidates(&cwd, &context.command);
    context.git = git_context(&cwd);
    context.project_commands = project_commands(&cwd);
    (context, secrets)
}

pub fn collect_intent(
    intent: String,
    shell: String,
    cwd: PathBuf,
    privacy: PrivacyMode,
) -> (SuggestionContext, redact::SecretMap) {
    let (intent, secrets) = redact::redact_command(&intent);
    let mut context = SuggestionContext {
        command: String::new(),
        intent: Some(intent),
        exit_code: None,
        succeeded: None,
        shell,
        os: env::consts::OS.to_string(),
        cwd: private_cwd(&cwd),
        terminal_output: None,
        executable_candidates: Vec::new(),
        path_candidates: Vec::new(),
        git: None,
        project_commands: Vec::new(),
    };
    if !matches!(privacy, PrivacyMode::Minimal) {
        context.git = git_context(&cwd);
        context.project_commands = project_commands(&cwd);
    }
    (context, secrets)
}

fn private_cwd(cwd: &Path) -> String {
    if let Some(home) = directories::BaseDirs::new().map(|d| d.home_dir().to_path_buf())
        && let Ok(relative) = cwd.strip_prefix(home)
    {
        return format!("<HOME>/{}", relative.display());
    }
    cwd.file_name()
        .map(|v| format!("<CWD>/{}", v.to_string_lossy()))
        .unwrap_or_else(|| "<CWD>".to_string())
}

fn limit_tail(value: &str, max: usize) -> String {
    if value.len() <= max {
        return value.to_string();
    }
    let mut start = value.len() - max;
    while !value.is_char_boundary(start) {
        start += 1;
    }
    format!("<TRUNCATED>\n{}", &value[start..])
}

fn executable_candidates(command: &str) -> Vec<String> {
    let typed = command.split_whitespace().next().unwrap_or_default();
    if typed.is_empty() || typed.contains(['/', '\\']) {
        return Vec::new();
    }
    let path_dirs: Vec<_> = env::split_paths(&env::var_os("PATH").unwrap_or_default()).collect();
    if path_dirs.iter().any(|dir| executable_exists(dir, typed)) {
        return Vec::new();
    }
    let mut names = HashSet::new();
    for dir in path_dirs {
        let Ok(entries) = fs::read_dir(dir) else {
            continue;
        };
        for entry in entries.flatten().take(2_000) {
            let mut name = entry.file_name().to_string_lossy().into_owned();
            if cfg!(windows) {
                for suffix in [".exe", ".cmd", ".bat", ".ps1"] {
                    if name.to_ascii_lowercase().ends_with(suffix) {
                        name.truncate(name.len() - suffix.len());
                        break;
                    }
                }
            }
            if distance(typed, &name) <= 2 {
                names.insert(name);
            }
        }
    }
    let mut values: Vec<_> = names.into_iter().collect();
    values.sort_by_key(|v| distance(typed, v));
    values.truncate(8);
    values
}

fn executable_exists(dir: &Path, name: &str) -> bool {
    if dir.join(name).is_file() {
        return true;
    }
    cfg!(windows)
        && [".exe", ".cmd", ".bat", ".ps1"]
            .iter()
            .any(|suffix| dir.join(format!("{name}{suffix}")).is_file())
}

fn path_candidates(cwd: &Path, command: &str) -> Vec<String> {
    let program = command
        .split_whitespace()
        .next()
        .unwrap_or_default()
        .to_ascii_lowercase();
    let needle = command
        .split_whitespace()
        .last()
        .unwrap_or_default()
        .trim_matches(['\'', '"']);
    if needle.is_empty() || needle.starts_with('-') {
        return Vec::new();
    }
    let path_commands = [
        "cd",
        "ls",
        "cat",
        "less",
        "rm",
        "mv",
        "cp",
        "open",
        "code",
        "xdg-open",
        "set-location",
        "get-content",
        "remove-item",
        "copy-item",
        "move-item",
    ];
    if !needle.contains(['/', '\\']) && !path_commands.contains(&program.as_str()) {
        return Vec::new();
    }
    let base = Path::new(needle)
        .file_name()
        .and_then(|v| v.to_str())
        .unwrap_or(needle);
    let Ok(entries) = fs::read_dir(cwd) else {
        return Vec::new();
    };
    let mut values: Vec<_> = entries
        .flatten()
        .filter_map(|e| e.file_name().into_string().ok())
        .filter(|name| distance(base, name) <= 3 || name.starts_with(base))
        .take(20)
        .collect();
    values.sort_by_key(|v| distance(base, v));
    values.truncate(8);
    values
}

fn git_context(cwd: &Path) -> Option<GitContext> {
    let mut child = Command::new("git")
        .current_dir(cwd)
        .args([
            "--no-pager",
            "--no-optional-locks",
            "-c",
            "core.fsmonitor=false",
            "-c",
            "status.submoduleSummary=false",
            "status",
            "--porcelain=v2",
            "--branch",
            "-z",
            "--untracked-files=normal",
            "--ignore-submodules=all",
            "--no-ahead-behind",
        ])
        .env("GIT_TERMINAL_PROMPT", "0")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    let stdout = child.stdout.take()?;
    let reader = thread::spawn(move || read_bounded(stdout, 2 * 1024 * 1024));
    let deadline = Instant::now() + Duration::from_secs(2);
    let status = loop {
        if let Some(status) = child.try_wait().ok()? {
            break status;
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            let _ = reader.join();
            return None;
        }
        thread::sleep(Duration::from_millis(10));
    };
    let output = reader.join().ok()?;
    if !status.success() {
        return None;
    }
    parse_git_status(&String::from_utf8_lossy(&output))
}

fn read_bounded(mut reader: impl Read, limit: usize) -> Vec<u8> {
    let mut result = Vec::new();
    let mut chunk = [0u8; 8192];
    while let Ok(read) = reader.read(&mut chunk) {
        if read == 0 {
            break;
        }
        let remaining = limit.saturating_sub(result.len());
        result.extend_from_slice(&chunk[..read.min(remaining)]);
    }
    result
}

fn parse_git_status(raw: &str) -> Option<GitContext> {
    let mut git = GitContext::default();
    for line in raw.split('\0').filter(|v| !v.is_empty()) {
        if let Some(value) = line.strip_prefix("# branch.head ") {
            git.branch = Some(value.to_string());
        } else if let Some(value) = line.strip_prefix("# branch.upstream ") {
            git.upstream = Some(value.to_string());
        } else if let Some(value) = line.strip_prefix("# branch.ab ") {
            for part in value.split_whitespace() {
                if let Some(v) = part.strip_prefix('+') {
                    git.ahead = v.parse().ok();
                }
                if let Some(v) = part.strip_prefix('-') {
                    git.behind = v.parse().ok();
                }
            }
        } else if line.starts_with("? ") {
            git.untracked += 1;
        } else if line.starts_with("u ") {
            git.conflicted += 1;
        } else if line.starts_with("1 ") || line.starts_with("2 ") {
            let xy = line.as_bytes().get(2..4).unwrap_or_default();
            if xy.first().is_some_and(|v| *v != b'.') {
                git.staged += 1;
            }
            if xy.get(1).is_some_and(|v| *v != b'.') {
                git.modified += 1;
            }
        }
    }
    Some(git)
}

fn project_commands(cwd: &Path) -> Vec<String> {
    let mut values = Vec::new();
    if let Ok(raw) = fs::read_to_string(cwd.join("package.json"))
        && raw.len() <= 1_000_000
        && let Ok(json) = serde_json::from_str::<Value>(&raw)
        && let Some(scripts) = json.get("scripts").and_then(Value::as_object)
    {
        values.extend(
            scripts
                .keys()
                .take(30)
                .map(|v| format!("package script: {v}")),
        );
    }
    if let Ok(raw) = fs::read_to_string(cwd.join("Cargo.toml"))
        && raw.len() <= 1_000_000
        && let Ok(doc) = raw.parse::<toml::Value>()
        && let Some(features) = doc.get("features").and_then(|v| v.as_table())
    {
        values.extend(
            features
                .keys()
                .take(30)
                .map(|v| format!("cargo feature: {v}")),
        );
    }
    values.truncate(40);
    values
}

fn distance(a: &str, b: &str) -> usize {
    let bchars: Vec<char> = b.chars().collect();
    let mut row: Vec<usize> = (0..=bchars.len()).collect();
    for (i, ca) in a.chars().enumerate() {
        let mut next = vec![i + 1];
        for (j, cb) in bchars.iter().enumerate() {
            next.push(
                (row[j + 1] + 1)
                    .min(next[j] + 1)
                    .min(row[j] + usize::from(ca != *cb)),
            );
        }
        row = next;
    }
    row[bchars.len()]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn edit_distance_works() {
        assert_eq!(distance("chekout", "checkout"), 1);
    }

    #[test]
    fn parses_git_summary_without_paths() {
        let raw = concat!(
            "# branch.head main\0",
            "# branch.upstream origin/main\0",
            "1 M. N... file\0",
            "? secret.txt\0"
        );
        let git = parse_git_status(raw).unwrap();
        assert_eq!(git.branch.as_deref(), Some("main"));
        assert_eq!(git.staged, 1);
        assert_eq!(git.untracked, 1);
    }

    #[test]
    fn explicit_intent_does_not_include_a_previous_command() {
        let (context, _) = collect_intent(
            "list the current directory".into(),
            "zsh".into(),
            PathBuf::from("/tmp"),
            PrivacyMode::Minimal,
        );
        let json = serde_json::to_value(context).unwrap();
        assert_eq!(json["intent"], "list the current directory");
        assert!(json.get("command").is_none());
        assert!(json.get("exit_code").is_none());
        assert!(json.get("terminal_output").is_none());
    }
}
