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
    if reverse && !path.exists() {
        return Ok(path);
    }
    let old = fs::read_to_string(&path).unwrap_or_default();
    let stripped = remove_block(&old);
    let new = if reverse {
        stripped
    } else {
        let line = match shell {
            Shell::Pwsh => "Invoke-Expression (& fuck shell-hook pwsh)",
            _ => &format!("eval \"$(command fuck shell-hook {})\"", shell.name()),
        };
        let separator = if stripped.is_empty() || stripped.ends_with('\n') {
            ""
        } else {
            "\n"
        };
        format!("{stripped}{separator}{START}\n{line}\n{END}\n")
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
    let prompt_hook = if shell == "zsh" {
        r#"
_llmfuck_precmd() {
  local _lf_status=$?
  LLMFUCK_LAST_EXIT=$_lf_status
  if [ -n "${LLMFUCK_PTY_SOCKET-}" ]; then
    command fuck pty-mark --exit-code "$_lf_status" --command "$(builtin fc -ln -1)" >/dev/null 2>&1
  fi
  return $_lf_status
}
autoload -Uz add-zsh-hook
add-zsh-hook -d precmd _llmfuck_precmd 2>/dev/null
add-zsh-hook precmd _llmfuck_precmd"#
    } else {
        r#"
_llmfuck_precmd() {
  local _lf_status=$?
  LLMFUCK_LAST_EXIT=$_lf_status
  if [ -n "${LLMFUCK_PTY_SOCKET-}" ]; then
    command fuck pty-mark --exit-code "$_lf_status" --command "$(builtin fc -ln -1)" >/dev/null 2>&1
  fi
  return $_lf_status
}
case ";${PROMPT_COMMAND[*]-};" in
  *';_llmfuck_precmd;'*) ;;
  *)
    case "$(declare -p PROMPT_COMMAND 2>/dev/null)" in
      'declare -a'*) PROMPT_COMMAND=(_llmfuck_precmd "${PROMPT_COMMAND[@]}") ;;
      *) PROMPT_COMMAND="_llmfuck_precmd${PROMPT_COMMAND:+;$PROMPT_COMMAND}" ;;
    esac
    ;;
esac"#
    };
    format!(
        r#"fuck() {{
  local _lf_status=${{LLMFUCK_LAST_EXIT:-$?}}
  if [ "$#" -gt 0 ]; then
    command fuck "$@"
    return $?
  fi
  local _lf_history
  _lf_history="$({history})"
  local _lf_cmd
  _lf_cmd="$(command fuck suggest --shell {shell} --exit-code "$_lf_status" --history "$_lf_history" --cwd "$PWD")" || return $?
  [ -n "$_lf_cmd" ] && eval "$_lf_cmd"
}}
{prompt_hook}"#
    )
}

fn pwsh_hook() -> String {
    r#"$script:LLMFuckExecutable = (Get-Command fuck -CommandType Application | Select-Object -First 1).Source
function global:fuck {
  $lfSucceeded = $?
  $lfExitCode = $global:LASTEXITCODE
  if ($args.Count -gt 0) {
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
    fn hooks_only_suggest_without_arguments() {
        for shell in [Shell::Bash, Shell::Zsh] {
            let generated = hook(shell);
            assert!(generated.contains("if [ \"$#\" -gt 0 ]; then"));
            assert!(!generated.contains("config|init|provider"));
        }

        let generated = hook(Shell::Pwsh);
        assert!(generated.contains("if ($args.Count -gt 0)"));
        assert!(!generated.contains("$args[0] -in"));
    }

    #[test]
    fn block_removal_preserves_user_text() {
        assert_eq!(
            remove_block("a\n# >>> llmfuck initialize >>>\nx\n# <<< llmfuck initialize <<<\nb\n"),
            "a\nb\n"
        );
    }

    #[test]
    fn install_and_reverse_touch_only_the_managed_block() {
        let dir = tempfile::tempdir().unwrap();
        let profile = dir.path().join("rc");
        fs::write(&profile, "user setting\n").unwrap();
        install(Shell::Bash, false, Some(&profile)).unwrap();
        let installed = fs::read_to_string(&profile).unwrap();
        assert!(installed.contains(START));
        assert!(installed.contains("user setting"));
        install(Shell::Bash, true, Some(&profile)).unwrap();
        assert_eq!(fs::read_to_string(profile).unwrap(), "user setting\n");
    }
}
