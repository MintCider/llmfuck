mod config;
mod context;
mod credentials;
mod model;
mod provider;
mod pty;
mod redact;
mod risk;
mod shell;
mod ui;

use std::{
    env,
    io::{self, Write},
    path::PathBuf,
    sync::mpsc,
    thread,
    time::{Duration, Instant},
};

use anyhow::{Context, Result, bail};
use clap::{Args, Parser, Subcommand};
use config::{PrivacyMode, ProviderConfig};

#[derive(Parser)]
#[command(name = "fuck", version, about = "LLM-powered shell command correction")]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand)]
enum Command {
    /// Suggest a correction (normally called by the shell integration)
    #[command(hide = true)]
    Suggest(SuggestArgs),
    /// Run the interactive setup wizard
    Config,
    /// Install or remove ordinary shell integration
    Init {
        #[arg(value_enum)]
        shell: Option<shell::Shell>,
        #[arg(long)]
        reverse: bool,
        #[arg(long, hide = true)]
        profile: Option<PathBuf>,
    },
    /// Manage LLM providers
    Provider {
        #[command(subcommand)]
        command: ProviderCommand,
    },
    /// Set the context privacy mode
    Privacy {
        #[command(subcommand)]
        command: PrivacyCommand,
    },
    /// Preview context without contacting a provider
    Context(ContextArgs),
    /// Show configuration and capture status
    Status,
    /// Diagnose the current installation
    Doctor,
    /// Explain manual PTY setup
    Pty,
    /// Start a manually configured PTY shell
    Shell {
        #[arg(last = true, required = true)]
        command: Vec<String>,
    },
    /// Print shell integration code
    #[command(hide = true)]
    ShellHook {
        #[arg(value_enum)]
        shell: shell::Shell,
    },
    /// Mark a command boundary for the PTY recorder
    #[command(hide = true)]
    PtyMark {
        #[arg(long)]
        command: String,
        #[arg(long)]
        exit_code: Option<i32>,
    },
    /// Generate commands from an explicit natural-language intent
    #[command(external_subcommand)]
    Prompt(Vec<String>),
}

#[derive(Args)]
struct SuggestArgs {
    #[arg(long)]
    shell: String,
    #[arg(long)]
    exit_code: Option<i32>,
    #[arg(long)]
    succeeded: Option<bool>,
    #[arg(long)]
    history: Option<String>,
    #[arg(long)]
    command: Option<String>,
    #[arg(long, conflicts_with_all = ["command", "history", "terminal_output"])]
    intent: Option<String>,
    #[arg(long)]
    cwd: PathBuf,
    #[arg(long)]
    terminal_output: Option<String>,
}

#[derive(Args)]
struct ContextArgs {
    #[arg(long)]
    command: Option<String>,
    #[arg(long, default_value = "unknown")]
    shell: String,
    #[arg(long)]
    exit_code: Option<i32>,
    #[arg(long)]
    cwd: Option<PathBuf>,
}

#[derive(Subcommand)]
enum ProviderCommand {
    Add {
        name: String,
        #[arg(long)]
        endpoint: Option<String>,
        #[arg(long)]
        model: Option<String>,
        #[arg(long)]
        reasoning_effort: Option<String>,
        #[arg(long)]
        api_key_env: Option<String>,
        #[arg(long)]
        no_api_key: bool,
        /// Store the prompted API key in plaintext in config.toml
        #[arg(long, conflicts_with_all = ["api_key_env", "no_api_key"])]
        plaintext_api_key: bool,
    },
    /// Update provider options without replacing its credential
    Set {
        name: Option<String>,
        #[arg(long, conflicts_with = "clear_reasoning_effort")]
        reasoning_effort: Option<String>,
        #[arg(long)]
        clear_reasoning_effort: bool,
    },
    List,
    Use {
        name: String,
    },
    Remove {
        name: String,
    },
    Test {
        name: Option<String>,
    },
    /// Measure end-to-end candidate latency with a fixed private prompt
    Latency {
        names: Vec<String>,
        #[arg(
            long,
            default_value_t = 1,
            value_parser = clap::value_parser!(u8).range(1..=10)
        )]
        runs: u8,
    },
}

#[derive(Subcommand)]
enum PrivacyCommand {
    Set {
        #[arg(value_enum)]
        mode: PrivacyMode,
    },
    Show,
}

fn main() {
    if let Err(error) = run() {
        eprintln!("error: {error:#}");
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        None => bail!("shell integration is not active; run `fuck config` or `fuck init`"),
        Some(Command::Suggest(args)) => suggest(args),
        Some(Command::Config) => configure(),
        Some(Command::Init {
            shell: requested,
            reverse,
            profile,
        }) => {
            let selected = shell::ensure_supported(requested)?;
            if let Some(path) = profile.as_deref() {
                shell::validate_profile(path)?;
            }
            let path = shell::install(selected, reverse, profile.as_deref())?;
            println!(
                "{} shell integration in {}",
                if reverse { "Removed" } else { "Installed" },
                path.display()
            );
            Ok(())
        }
        Some(Command::Provider { command }) => provider_command(command),
        Some(Command::Privacy { command }) => privacy_command(command),
        Some(Command::Context(args)) => preview_context(args),
        Some(Command::Status) => status(),
        Some(Command::Doctor) => doctor(),
        Some(Command::Pty) => {
            println!("{}", shell::pty_help());
            Ok(())
        }
        Some(Command::Shell { command }) => pty_shell(&command),
        Some(Command::ShellHook { shell }) => {
            print!("{}", shell::hook(shell));
            Ok(())
        }
        Some(Command::PtyMark { command, exit_code }) => pty_mark(command, exit_code),
        Some(Command::Prompt(parts)) => suggest_intent(parts.join(" ")),
    }
}

fn suggest(args: SuggestArgs) -> Result<()> {
    let cfg = config::load()?;
    let (name, provider_cfg) = config::active_provider(&cfg)?;
    if let Some(intent) = args.intent {
        let (ctx, secrets) = context::collect_intent(intent, args.shell, args.cwd, cfg.privacy);
        return request_and_select(name, provider_cfg, ctx, secrets);
    }
    let command = args
        .command
        .or_else(|| {
            args.history
                .as_deref()
                .and_then(|v| shell::previous_from_history(v).ok())
        })
        .context("no previous command available")?;
    let terminal_output = args.terminal_output.or_else(|| {
        env::var_os("LLMFUCK_PTY_SOCKET").and_then(|path| {
            pty::get(std::path::Path::new(&path), command.clone())
                .ok()
                .flatten()
        })
    });
    let (ctx, secrets) = context::collect(
        command,
        args.exit_code,
        args.succeeded,
        args.shell,
        args.cwd,
        cfg.privacy,
        terminal_output,
    );
    request_and_select(name, provider_cfg, ctx, secrets)
}

fn suggest_intent(intent: String) -> Result<()> {
    let intent = intent.trim();
    if intent.is_empty() {
        bail!("prompt must not be empty");
    }
    let cfg = config::load()?;
    let (name, provider_cfg) = config::active_provider(&cfg)?;
    let shell = shell::detect()
        .map(|value| value.name().to_string())
        .unwrap_or_else(|| "unknown".to_string());
    let (ctx, secrets) =
        context::collect_intent(intent.to_string(), shell, env::current_dir()?, cfg.privacy);
    request_and_select(name, provider_cfg, ctx, secrets)
}

fn request_and_select(
    name: &str,
    provider_cfg: &ProviderConfig,
    ctx: model::SuggestionContext,
    secrets: redact::SecretMap,
) -> Result<()> {
    let key = provider_key(name, provider_cfg)?;
    let mut candidates = {
        let _status = ui::RequestStatus::new(&provider_cfg.model);
        provider::suggest(provider_cfg, key.as_deref(), &ctx)?
    };
    candidates.retain_mut(|candidate| {
        let Ok(command) = redact::restore_command(&candidate.command, &secrets) else {
            return false;
        };
        candidate.command = command;
        true
    });
    if candidates.is_empty() {
        bail!("the provider returned no safe command candidates");
    }
    if let Some(selected) = ui::select(&candidates)? {
        println!("{}", selected.command);
    }
    Ok(())
}

fn configure() -> Result<()> {
    eprintln!(
        "llmfuck sends command context to the configured LLM provider. Commands, paths, and terminal output may contain confidential data. Redaction reduces risk but cannot guarantee that every secret is detected.\n"
    );
    if !confirm("Continue with configuration?", false)? {
        return Ok(());
    }
    let name = prompt("Provider name", "default")?;
    let endpoint = prompt(
        "Chat Completions endpoint",
        "https://api.openai.com/v1/chat/completions",
    )?;
    let model = prompt("Model", "")?;
    if model.is_empty() {
        bail!("model is required");
    }
    let reasoning_effort = optional_setting(prompt(
        "Reasoning effort (leave empty for provider default)",
        "",
    )?)?;
    let secret =
        rpassword::prompt_password("API key (leave empty for a local unauthenticated endpoint): ")?;
    let mut cfg = config::load()?;
    let Some(stored_key) = store_api_key(&name, secret, false)? else {
        eprintln!("Configuration cancelled; the API key was not saved.");
        return Ok(());
    };
    let superseded_credential =
        superseded_credential(&cfg, &name, stored_key.credential.as_deref());
    cfg.providers.insert(
        name.clone(),
        ProviderConfig {
            endpoint,
            model,
            reasoning_effort,
            credential: stored_key.credential,
            api_key_env: None,
            api_key: stored_key.api_key,
        },
    );
    cfg.default_provider = Some(name);
    cfg.privacy = PrivacyMode::Smart;
    config::save(&cfg)?;
    if let Some(reference) = superseded_credential {
        let _ = credentials::delete(&reference);
    }
    if let Some(detected) = shell::detect()
        && confirm(
            &format!("Install ordinary {} integration?", detected.name()),
            true,
        )?
    {
        let path = shell::install(detected, false, None)?;
        println!("Installed shell integration in {}", path.display());
    }
    println!(
        "Configuration saved to {}",
        config::config_path()?.display()
    );
    println!(
        "Optional PTY capture exists but was not enabled. Run `fuck pty` to read the manual setup instructions."
    );
    Ok(())
}

fn provider_command(command: ProviderCommand) -> Result<()> {
    let mut cfg = config::load()?;
    match command {
        ProviderCommand::Add {
            name,
            endpoint,
            model,
            reasoning_effort,
            api_key_env,
            no_api_key,
            plaintext_api_key,
        } => {
            let endpoint = match endpoint {
                Some(endpoint) => endpoint,
                None => prompt(
                    "Chat Completions endpoint",
                    "https://api.openai.com/v1/chat/completions",
                )?,
            };
            let model = match model {
                Some(model) => model,
                None => prompt("Model", "")?,
            };
            if model.is_empty() {
                bail!("model is required");
            }
            let reasoning_effort = reasoning_effort
                .map(optional_setting)
                .transpose()?
                .flatten();
            let stored_key = if no_api_key || api_key_env.is_some() {
                StoredApiKey::default()
            } else {
                let secret = rpassword::prompt_password("API key: ")?;
                let Some(stored_key) = store_api_key(&name, secret, plaintext_api_key)? else {
                    eprintln!("Provider was not added; the API key was not saved.");
                    return Ok(());
                };
                stored_key
            };
            let superseded_credential =
                superseded_credential(&cfg, &name, stored_key.credential.as_deref());
            cfg.providers.insert(
                name.clone(),
                ProviderConfig {
                    endpoint,
                    model,
                    reasoning_effort,
                    credential: stored_key.credential,
                    api_key_env,
                    api_key: stored_key.api_key,
                },
            );
            if cfg.default_provider.is_none() {
                cfg.default_provider = Some(name);
            }
            config::save(&cfg)?;
            if let Some(reference) = superseded_credential {
                let _ = credentials::delete(&reference);
            }
        }
        ProviderCommand::Set {
            name,
            reasoning_effort,
            clear_reasoning_effort,
        } => {
            if reasoning_effort.is_none() && !clear_reasoning_effort {
                bail!("specify `--reasoning-effort VALUE` or `--clear-reasoning-effort`");
            }
            let name = name
                .or(cfg.default_provider.clone())
                .context("no provider selected")?;
            let provider = cfg
                .providers
                .get_mut(&name)
                .with_context(|| format!("provider `{name}` does not exist"))?;
            provider.reasoning_effort = if clear_reasoning_effort {
                None
            } else {
                reasoning_effort
                    .map(optional_setting)
                    .transpose()?
                    .flatten()
            };
            config::save(&cfg)?;
        }
        ProviderCommand::List => {
            for (name, item) in &cfg.providers {
                println!(
                    "{}{}\t{}\t{}\treasoning={}",
                    if cfg.default_provider.as_deref() == Some(name) {
                        "* "
                    } else {
                        "  "
                    },
                    name,
                    item.model,
                    item.endpoint,
                    item.reasoning_effort
                        .as_deref()
                        .unwrap_or("provider-default")
                );
            }
        }
        ProviderCommand::Use { name } => {
            if !cfg.providers.contains_key(&name) {
                bail!("provider `{name}` does not exist");
            }
            cfg.default_provider = Some(name);
            config::save(&cfg)?;
        }
        ProviderCommand::Remove { name } => {
            let removed_credential = cfg
                .providers
                .remove(&name)
                .and_then(|provider| provider.credential);
            if cfg.default_provider.as_deref() == Some(&name) {
                cfg.default_provider = None;
            }
            config::save(&cfg)?;
            if let Some(reference) = removed_credential {
                let _ = credentials::delete(&reference);
            }
        }
        ProviderCommand::Test { name } => {
            let name = name
                .or(cfg.default_provider.clone())
                .context("no provider selected")?;
            let item = cfg
                .providers
                .get(&name)
                .context("provider does not exist")?;
            let _ = provider_key(&name, item)?;
            println!(
                "Provider `{name}` configuration and credential are readable. A network request was not sent."
            );
        }
        ProviderCommand::Latency { names, runs } => provider_latency(&cfg, names, runs)?,
    }
    Ok(())
}

struct LatencyResult {
    name: String,
    model: String,
    timings: Vec<Duration>,
    errors: Vec<String>,
}

fn provider_latency(cfg: &config::Config, names: Vec<String>, runs: u8) -> Result<()> {
    let names = if names.is_empty() {
        cfg.providers.keys().cloned().collect::<Vec<_>>()
    } else {
        let mut unique = Vec::new();
        for name in names {
            if !cfg.providers.contains_key(&name) {
                bail!("provider `{name}` does not exist");
            }
            if !unique.contains(&name) {
                unique.push(name);
            }
        }
        unique
    };
    if names.is_empty() {
        bail!("no providers are configured");
    }

    let context = latency_context();
    let (sender, receiver) = mpsc::channel();
    for name in &names {
        let provider = cfg
            .providers
            .get(name)
            .context("provider disappeared from configuration")?
            .clone();
        let key = provider_key(name, &provider);
        let name = name.clone();
        let context = context.clone();
        let sender = sender.clone();
        thread::spawn(move || {
            let mut result = LatencyResult {
                name,
                model: provider.model.clone(),
                timings: Vec::new(),
                errors: Vec::new(),
            };
            match key {
                Ok(key) => {
                    for _ in 0..runs {
                        let started = Instant::now();
                        match provider::suggest(&provider, key.as_deref(), &context) {
                            Ok(_) => result.timings.push(started.elapsed()),
                            Err(error) => result.errors.push(format!("{error:#}")),
                        }
                    }
                }
                Err(error) => result.errors.push(format!("credential error: {error:#}")),
            }
            let _ = sender.send(result);
        });
    }
    drop(sender);

    eprintln!(
        "Testing {} provider(s) in parallel with {} request(s) each…",
        names.len(),
        runs
    );
    for _ in 0..names.len() {
        let result = receiver.recv().context("latency worker stopped")?;
        print_latency_result(&result, runs);
    }
    Ok(())
}

fn latency_context() -> model::SuggestionContext {
    model::SuggestionContext {
        command: String::new(),
        intent: Some("Print the current working directory.".into()),
        exit_code: None,
        succeeded: None,
        shell: shell::detect()
            .map(|value| value.name().to_string())
            .unwrap_or_else(|| "unknown".into()),
        os: env::consts::OS.into(),
        cwd: "<BENCHMARK>".into(),
        terminal_output: None,
        executable_candidates: Vec::new(),
        path_candidates: Vec::new(),
        git: None,
        project_commands: Vec::new(),
    }
}

fn print_latency_result(result: &LatencyResult, runs: u8) {
    if result.timings.is_empty() {
        let error = result
            .errors
            .first()
            .map(|value| one_line(value, 180))
            .unwrap_or_else(|| "unknown error".into());
        println!("{}	{}	error: {}", result.name, result.model, error);
        return;
    }

    let mut millis: Vec<u128> = result.timings.iter().map(Duration::as_millis).collect();
    millis.sort_unstable();
    let median = if millis.len().is_multiple_of(2) {
        (millis[millis.len() / 2 - 1] + millis[millis.len() / 2]) / 2
    } else {
        millis[millis.len() / 2]
    };
    let minimum = millis[0];
    if runs == 1 {
        println!("{}\t{}\t{} ms", result.name, result.model, median);
    } else {
        println!(
            "{}\t{}\tmedian={} ms\tmin={} ms\tsuccess={}/{}",
            result.name,
            result.model,
            median,
            minimum,
            result.timings.len(),
            runs
        );
    }
    if let Some(error) = result.errors.first() {
        println!("  error: {}", one_line(error, 180));
    }
}

fn one_line(value: &str, max_chars: usize) -> String {
    let normalized = value.split_whitespace().collect::<Vec<_>>().join(" ");
    normalized.chars().take(max_chars).collect()
}

fn privacy_command(command: PrivacyCommand) -> Result<()> {
    let mut cfg = config::load()?;
    match command {
        PrivacyCommand::Set { mode } => {
            cfg.privacy = mode;
            config::save(&cfg)?;
        }
        PrivacyCommand::Show => println!("{:?}", cfg.privacy),
    }
    Ok(())
}

fn preview_context(args: ContextArgs) -> Result<()> {
    let cfg = config::load()?;
    let command = args.command.unwrap_or_default();
    let (ctx, _) = context::collect(
        command,
        args.exit_code,
        None,
        args.shell,
        args.cwd.unwrap_or(env::current_dir()?),
        cfg.privacy,
        None,
    );
    println!("{}", provider::preview_request(&ctx)?);
    Ok(())
}

fn status() -> Result<()> {
    let cfg = config::load()?;
    println!("Config: {}", config::config_path()?.display());
    println!("Privacy: {:?}", cfg.privacy);
    println!(
        "Provider: {}",
        cfg.default_provider.as_deref().unwrap_or("not configured")
    );
    println!(
        "Capture: {}",
        if env::var_os("LLMFUCK_PTY_SESSION").is_some() {
            "PTY"
        } else {
            "ordinary"
        }
    );
    Ok(())
}

fn doctor() -> Result<()> {
    let cfg = config::load()?;
    let (name, item) = config::active_provider(&cfg)?;
    let _ = provider_key(name, item)?;
    println!("Configuration: OK");
    println!("Provider credential: OK");
    if let Some(shell) = shell::detect() {
        println!("Detected shell: {}", shell.name());
    }
    Ok(())
}

fn provider_key(name: &str, provider: &ProviderConfig) -> Result<Option<String>> {
    if let Some(env_name) = &provider.api_key_env {
        return env::var(env_name)
            .map(Some)
            .with_context(|| format!("environment variable `{env_name}` is not set"));
    }
    if let Some(reference) = &provider.credential {
        return credentials::load(reference).map(Some);
    }
    if let Some(api_key) = &provider.api_key {
        return Ok(Some(api_key.clone()));
    }
    let _ = name;
    Ok(None)
}

fn optional_setting(value: String) -> Result<Option<String>> {
    let value = value.trim();
    if value.is_empty() {
        return Ok(None);
    }
    if value.chars().any(char::is_control) || value.len() > 64 {
        bail!("provider setting must be 1 to 64 printable characters");
    }
    Ok(Some(value.to_string()))
}

#[derive(Default)]
struct StoredApiKey {
    credential: Option<String>,
    api_key: Option<String>,
}

fn store_api_key(
    provider_name: &str,
    secret: String,
    plaintext_requested: bool,
) -> Result<Option<StoredApiKey>> {
    if secret.is_empty() {
        return Ok(Some(StoredApiKey::default()));
    }
    if plaintext_requested {
        return plaintext_api_key(secret);
    }

    let reference = format!("provider:{provider_name}");
    match credentials::store(&reference, &secret) {
        Ok(()) => Ok(Some(StoredApiKey {
            credential: Some(reference),
            api_key: None,
        })),
        Err(error) => {
            eprintln!("The system credential store is unavailable: {error:#}");
            plaintext_api_key(secret)
        }
    }
}

fn plaintext_api_key(secret: String) -> Result<Option<StoredApiKey>> {
    let path = config::config_path()?;
    eprintln!(
        "The API key can be stored unencrypted in {} instead.",
        path.display()
    );
    #[cfg(unix)]
    eprintln!("The configuration file will be restricted to your user (mode 0600).");
    if !confirm("Save the API key in the config file?", false)? {
        return Ok(None);
    }
    Ok(Some(StoredApiKey {
        credential: None,
        api_key: Some(secret),
    }))
}

fn superseded_credential(
    config: &config::Config,
    provider_name: &str,
    new_reference: Option<&str>,
) -> Option<String> {
    let old_reference = config
        .providers
        .get(provider_name)
        .and_then(|provider| provider.credential.as_deref())?;
    if Some(old_reference) != new_reference {
        return Some(old_reference.to_string());
    }
    None
}

fn prompt(label: &str, default: &str) -> Result<String> {
    eprint!(
        "{label}{}: ",
        if default.is_empty() {
            String::new()
        } else {
            format!(" [{default}]")
        }
    );
    io::stderr().flush()?;
    let mut value = String::new();
    io::stdin().read_line(&mut value)?;
    let value = value.trim();
    Ok(if value.is_empty() {
        default.to_string()
    } else {
        value.to_string()
    })
}

fn confirm(label: &str, default: bool) -> Result<bool> {
    let answer = prompt(
        &format!("{label} {}", if default { "[Y/n]" } else { "[y/N]" }),
        "",
    )?;
    if answer.is_empty() {
        Ok(default)
    } else {
        Ok(matches!(answer.to_ascii_lowercase().as_str(), "y" | "yes"))
    }
}

fn pty_shell(command: &[String]) -> Result<()> {
    pty::run(command)
}

fn pty_mark(command: String, exit_code: Option<i32>) -> Result<()> {
    let socket = env::var_os("LLMFUCK_PTY_SOCKET").context("PTY recorder is not active")?;
    pty::mark(std::path::Path::new(&socket), command, exit_code)
}

#[cfg(test)]
mod cli_tests {
    use super::*;
    use clap::error::ErrorKind;

    #[test]
    fn parses_unknown_words_as_an_explicit_prompt() {
        let cli =
            Cli::try_parse_from(["fuck", "I", "want", "to", "pull", "upstream/master"]).unwrap();
        let Some(Command::Prompt(parts)) = cli.command else {
            panic!("expected an explicit prompt");
        };
        assert_eq!(parts.join(" "), "I want to pull upstream/master");
    }

    #[test]
    fn help_remains_cli_help() {
        for args in [["fuck", "help"], ["fuck", "--help"]] {
            let error = match Cli::try_parse_from(args) {
                Ok(_) => panic!("expected help output"),
                Err(error) => error,
            };
            assert_eq!(error.kind(), ErrorKind::DisplayHelp);
        }
    }

    #[test]
    fn parses_parallel_provider_latency_options() {
        let cli = Cli::try_parse_from([
            "fuck", "provider", "latency", "groq", "gemini", "--runs", "3",
        ])
        .unwrap();
        let Some(Command::Provider {
            command: ProviderCommand::Latency { names, runs },
        }) = cli.command
        else {
            panic!("expected provider latency command");
        };
        assert_eq!(names, ["groq", "gemini"]);
        assert_eq!(runs, 3);
    }

    #[test]
    fn latency_probe_contains_no_real_command_context() {
        let json = serde_json::to_value(latency_context()).unwrap();
        assert_eq!(json["intent"], "Print the current working directory.");
        assert_eq!(json["cwd"], "<BENCHMARK>");
        for field in ["command", "exit_code", "terminal_output", "git"] {
            assert!(json.get(field).is_none(), "unexpected field: {field}");
        }
    }
}
