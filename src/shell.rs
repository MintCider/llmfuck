use std::{
    env, fs,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, bail};

const START: &str = "# >>> llmfuck initialize >>>";
const END: &str = "# <<< llmfuck initialize <<<";

#[derive(Debug, Clone, Copy, clap::ValueEnum)]
pub enum Shell {
    Bash,
    Zsh,
    Pwsh,
}

impl Shell {
    pub fn name(self) -> &'static str {
        match self {
            Self::Bash => "bash",
            Self::Zsh => "zsh",
            Self::Pwsh => "pwsh",
        }
    }
}

pub fn detect() -> Option<Shell> {
    if cfg!(windows) {
        return Some(Shell::Pwsh);
    }
    let shell = env::var("SHELL").ok()?;
    if shell.ends_with("zsh") {
        Some(Shell::Zsh)
    } else if shell.ends_with("bash") {
        Some(Shell::Bash)
    } else {
        None
    }
}

pub fn profile(shell: Shell) -> Result<PathBuf> {
    let home = directories::BaseDirs::new()
        .context("cannot determine home directory")?
        .home_dir()
        .to_path_buf();
    Ok(match shell {
        Shell::Bash => home.join(".bashrc"),
        Shell::Zsh => home.join(".zshrc"),
        Shell::Pwsh if cfg!(windows) => home
            .join("Documents")
            .join("PowerShell")
            .join("Microsoft.PowerShell_profile.ps1"),
        Shell::Pwsh => home.join(".config/powershell/Microsoft.PowerShell_profile.ps1"),
    })
}

pub fn install(shell: Shell, reverse: bool, explicit_profile: Option<&Path>) -> Result<PathBuf> {
    let path = explicit_profile
        .map(Path::to_path_buf)
        .unwrap_or(profile(shell)?);
    let old = fs::read_to_string(&path).unwrap_or_default();
    let stripped = remove_block(&old);
    let new = if reverse {
        stripped
    } else {
        let line = match shell {
            Shell::Pwsh => "Invoke-Expression (& fuck shell-hook pwsh)",
            _ => &format!("eval \"$(command fuck shell-hook {})\"", shell.name()),
        };
        format!(
            "{}{}\n{}\n{}\n",
            stripped.trim_end(),
            if stripped.trim().is_empty() {
                ""
            } else {
                "\n\n"
            },
            START,
            line.to_string() + "\n" + END
        )
    };
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    if path.exists() {
        fs::copy(&path, path.with_extension("llmfuck.bak"))?;
    }
    let tmp = path.with_extension("llmfuck.tmp");
    fs::write(&tmp, new)?;
    fs::rename(tmp, &path)?;
    Ok(path)
}

fn remove_block(input: &str) -> String {
    let Some(start) = input.find(START) else {
        return input.to_string();
    };
    let Some(relative_end) = input[start..].find(END) else {
        return input.to_string();
    };
    let end = start + relative_end + END.len();
    let end = end + usize::from(input.as_bytes().get(end) == Some(&b'\n'));
    format!("{}{}", &input[..start], &input[end..])
}

pub fn hook(shell: Shell) -> String {
    match shell {
        Shell::Bash => posix_hook("bash", "builtin fc -ln -10"),
        Shell::Zsh => posix_hook("zsh", "builtin fc -ln -10"),
        Shell::Pwsh => pwsh_hook(),
    }
}

fn posix_hook(shell: &str, history: &str) -> String {
    format!(
        r#"fuck() {{
  local _lf_status=$?
  case "${{1-}}" in
    config|init|provider|privacy|context|status|doctor|pty|shell|shell-hook) command fuck "$@"; return $? ;;
  esac
  local _lf_history
  _lf_history="$({history})"
  local _lf_cmd
  _lf_cmd="$(command fuck suggest --shell {shell} --exit-code "$_lf_status" --history "$_lf_history" --cwd "$PWD")" || return $?
  [ -n "$_lf_cmd" ] && eval "$_lf_cmd"
}}"#
    )
}

fn pwsh_hook() -> String {
    r#"$script:LLMFuckExecutable = (Get-Command fuck -CommandType Application | Select-Object -First 1).Source
function global:fuck {
  $lfSucceeded = $?
  $lfExitCode = $global:LASTEXITCODE
  if ($args.Count -gt 0 -and $args[0] -in @('config','init','provider','privacy','context','status','doctor','pty','shell','shell-hook')) {
    & $script:LLMFuckExecutable @args
    return
  }
  $lfHistory = (Get-History -Count 10 | ForEach-Object CommandLine) -join "`n"
  $lfCommand = & $script:LLMFuckExecutable suggest --shell pwsh --exit-code $lfExitCode --succeeded $lfSucceeded --history $lfHistory --cwd $PWD.Path
  if ($LASTEXITCODE -eq 0 -and $lfCommand) { Invoke-Expression ($lfCommand -join [Environment]::NewLine) }
}"#.to_string()
}

pub fn previous_from_history(history: &str) -> Result<String> {
    history
        .lines()
        .rev()
        .map(str::trim)
        .find(|line| !line.is_empty() && !line.starts_with("fuck") && !line.starts_with("llmfuck"))
        .map(str::to_string)
        .context("could not find a previous command")
}

pub fn pty_help() -> &'static str {
    "PTY mode captures bounded terminal output in memory and is never enabled by the configuration wizard.\n\nConfigure your terminal profile manually to start one of:\n  fuck shell -- zsh -l\n  fuck shell -- bash -l\n  fuck shell -- pwsh\n\nKeep the ordinary shell integration installed. See: https://github.com/llmfuck/llmfuck/blob/main/docs/pty.md"
}

pub fn ensure_supported(shell: Option<Shell>) -> Result<Shell> {
    shell
        .or_else(detect)
        .ok_or_else(|| anyhow::anyhow!("cannot detect shell; specify bash, zsh, or pwsh"))
}

pub fn validate_profile(path: &Path) -> Result<()> {
    if path.is_dir() {
        bail!("profile path is a directory: {}", path.display());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn history_skips_fuck() {
        assert_eq!(
            previous_from_history("echo ok\ngit chekout main\nfuck").unwrap(),
            "git chekout main"
        );
    }
    #[test]
    fn block_removal_preserves_user_text() {
        assert_eq!(
            remove_block("a\n# >>> llmfuck initialize >>>\nx\n# <<< llmfuck initialize <<<\nb\n"),
            "a\nb\n"
        );
    }
}
