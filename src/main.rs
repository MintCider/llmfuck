mod config;
mod context;
mod credentials;
mod model;
mod provider;
mod redact;
mod risk;
mod shell;
mod ui;

use std::{
    env,
    io::{self, Write},
    path::PathBuf,
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
        api_key_env: Option<String>,
        #[arg(long)]
        no_api_key: bool,
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
    }
}

fn suggest(args: SuggestArgs) -> Result<()> {
    let cfg = config::load()?;
    let (name, provider_cfg) = config::active_provider(&cfg)?;
    let command = args
        .command
        .or_else(|| {
            args.history
                .as_deref()
                .and_then(|v| shell::previous_from_history(v).ok())
        })
        .context("no previous command available")?;
    let ctx = context::collect(
        command,
        args.exit_code,
        args.succeeded,
        args.shell,
        args.cwd,
        cfg.privacy,
        args.terminal_output,
    );
    let key = provider_key(name, provider_cfg)?;
    let candidates = provider::suggest(provider_cfg, key.as_deref(), &ctx)?;
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
    let secret =
        rpassword::prompt_password("API key (leave empty for a local unauthenticated endpoint): ")?;
    let mut cfg = config::load()?;
    let credential = if secret.is_empty() {
        None
    } else {
        let reference = format!("provider:{name}");
        credentials::store(&reference, &secret)?;
        Some(reference)
    };
    cfg.providers.insert(
        name.clone(),
        ProviderConfig {
            endpoint,
            model,
            credential,
            api_key_env: None,
        },
    );
    cfg.default_provider = Some(name);
    cfg.privacy = PrivacyMode::Smart;
    config::save(&cfg)?;
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
            api_key_env,
            no_api_key,
        } => {
            let endpoint = endpoint.unwrap_or(prompt(
                "Chat Completions endpoint",
                "https://api.openai.com/v1/chat/completions",
            )?);
            let model = model.unwrap_or(prompt("Model", "")?);
            if model.is_empty() {
                bail!("model is required");
            }
            let credential = if no_api_key || api_key_env.is_some() {
                None
            } else {
                let secret = rpassword::prompt_password("API key: ")?;
                if secret.is_empty() {
                    None
                } else {
                    let reference = format!("provider:{name}");
                    credentials::store(&reference, &secret)?;
                    Some(reference)
                }
            };
            cfg.providers.insert(
                name.clone(),
                ProviderConfig {
                    endpoint,
                    model,
                    credential,
                    api_key_env,
                },
            );
            if cfg.default_provider.is_none() {
                cfg.default_provider = Some(name);
            }
            config::save(&cfg)?;
        }
        ProviderCommand::List => {
            for (name, item) in &cfg.providers {
                println!(
                    "{}{}\t{}\t{}",
                    if cfg.default_provider.as_deref() == Some(name) {
                        "* "
                    } else {
                        "  "
                    },
                    name,
                    item.model,
                    item.endpoint
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
            if let Some(item) = cfg.providers.remove(&name)
                && let Some(reference) = item.credential
            {
                let _ = credentials::delete(&reference);
            }
            if cfg.default_provider.as_deref() == Some(&name) {
                cfg.default_provider = None;
            }
            config::save(&cfg)?;
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
    }
    Ok(())
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
    let ctx = context::collect(
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
    let _ = name;
    Ok(None)
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
    let _ = command;
    bail!("PTY forwarding is not available in this build yet; ordinary mode remains active")
}
