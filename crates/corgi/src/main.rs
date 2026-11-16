//! corgi: a tmux-style terminal UI for herdr-core.

mod app;
mod chooser;
mod claude_tap;
mod clipboard;
mod commands;
mod config;
mod copy;
mod display;
mod doctor;
mod form;
mod host_notify;
mod input;
mod keyhints;
mod keylist;
mod keys;
mod layout;
mod logging;
mod machines;
mod menu;
mod migrate;
mod model;
mod nesting;
mod pane;
mod pane_title;
mod paths;
mod persist;
mod process;
mod sidebar;
mod status;
mod term;
mod theme;
mod toast;
mod tree;
mod ui;
mod usage;
mod usage_sources;
mod worktree;
mod worktrees;

use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(name = "corgi", version, about = "A tmux-style terminal UI for herdr-core")]
struct Cli {
    /// herdr-core session to use (sets HC_SESSION for the core connection).
    #[arg(long, env = "HC_SESSION", default_value = "default")]
    session: String,
    /// Connect to this core socket instead of spawning `hc connect`.
    #[arg(long, env = "HC_SOCKET")]
    socket: Option<PathBuf>,
    /// Config file (default: ~/.config/corgi/config.toml).
    #[arg(long, env = "CORGI_CONFIG")]
    config: Option<PathBuf>,
    /// Print the default config and exit.
    #[arg(long)]
    default_config: bool,
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand, Debug)]
enum Command {
    /// Check the connection to herdr-core and print what it reports.
    Doctor,
    /// View a specific core terminal (an address such as `t7` or `vps/t7`).
    Attach { terminal: String },
    /// Claude Code's status line, with its rate limits kept for corgi's `claude` usage
    /// segment: `corgi claude-statusline -- <your status line command>` as `statusLine`.
    ClaudeStatusline {
        /// The status line command to run with the same input (none: print nothing).
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        cmd: Vec<String>,
    },
    /// Make a corgi config from the herdr TUI's: its keys (on the herdr preset), theme,
    /// sidebar, tab bar and more; what has no counterpart is listed.
    Migrate {
        /// herdr's config (default: ~/.config/herdr/config.toml).
        #[arg(long)]
        from: Option<PathBuf>,
        /// Show it instead of writing it.
        #[arg(long)]
        print: bool,
        /// Replace a corgi config that's there (it's kept as config.toml.bak).
        #[arg(long)]
        force: bool,
    },
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    if cli.default_config {
        print!("{}", config::DEFAULT_TOML);
        return Ok(());
    }
    if let Some(Command::ClaudeStatusline { cmd }) = &cli.command {
        std::process::exit(claude_tap::run(cmd));
    }
    if let Some(Command::Migrate { from, print, force }) = &cli.command {
        let to = cli.config.clone().unwrap_or_else(config::default_path);
        return run_migrate(from.clone(), &to, *print, *force);
    }
    if let Some(Command::Doctor) = cli.command {
        return doctor::run(&cli.session, cli.socket.as_deref());
    }
    let log = logging::init()?;
    tracing::info!(pid = std::process::id(), session = %cli.session, socket = ?cli.socket, config = ?cli.config, "corgi starting");
    let transport = match cli.socket {
        #[cfg(unix)]
        Some(path) => hc_proto::Transport::Socket(path),
        #[cfg(not(unix))]
        Some(_) => anyhow::bail!("--socket is only supported on Unix; use HC_SESSION"),
        None => hc_proto::Transport::hc(Some(&cli.session)),
    };
    let attach = match cli.command {
        Some(Command::Attach { terminal }) => Some(terminal),
        _ => None,
    };
    // A broken config stops here, before the screen is taken, with the reason.
    let config_path = cli.config.clone().unwrap_or_else(config::default_path);
    let result = config::load(&config_path)
        .map_err(anyhow::Error::msg)
        .and_then(|config| {
            app::run(app::Options {
                transport,
                session: cli.session.clone(),
                attach,
                config,
                config_path,
            })
        });
    if let Err(e) = result {
        tracing::error!("{e:#}");
        eprintln!("corgi: {e:#}\nlog: {}", log.display());
        std::process::exit(1);
    }
    Ok(())
}

/// `corgi migrate`.
fn run_migrate(from: Option<PathBuf>, to: &std::path::Path, print: bool, force: bool) -> anyhow::Result<()> {
    let from = from.unwrap_or_else(|| {
        std::env::var_os("XDG_CONFIG_HOME")
            .filter(|v| !v.is_empty())
            .map_or_else(|| paths::home().join(".config"), PathBuf::from)
            .join("herdr/config.toml")
    });
    let text = std::fs::read_to_string(&from).map_err(|e| anyhow::anyhow!("{}: {e}", from.display()))?;
    let shown = from.display().to_string();
    let shown = match shown.strip_prefix(&paths::home().display().to_string()) {
        Some(rest) => format!("~{rest}"),
        None => shown,
    };
    let m = migrate::migrate(&text, &shown).map_err(anyhow::Error::msg)?;
    if print {
        print!("{}", m.text);
        return Ok(());
    }
    if to.exists() && !force {
        anyhow::bail!(
            "{} is there already: --print shows the new one, --force replaces it (keeping it as {}.bak)",
            to.display(),
            to.display()
        );
    }
    if let Some(dir) = to.parent() {
        std::fs::create_dir_all(dir)?;
    }
    if to.exists() {
        let bak = PathBuf::from(format!("{}.bak", to.display()));
        std::fs::copy(to, &bak)?;
        eprintln!("kept the old one as {}", bak.display());
    }
    std::fs::write(to, &m.text)?;
    eprintln!("wrote {} (from {shown})", to.display());
    if !m.notes.is_empty() {
        eprintln!("not carried over:");
        for n in &m.notes {
            eprintln!("  - {n}");
        }
    }
    Ok(())
}
