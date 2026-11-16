//! Logs go to a file: the screen belongs to the UI.

use std::path::PathBuf;
use tracing_subscriber::EnvFilter;

/// Starts logging to `<state>/corgi.log`; level from `CORGI_LOG` (default `info`).
pub fn init() -> anyhow::Result<PathBuf> {
    let dir = crate::paths::state_dir();
    std::fs::create_dir_all(&dir)?;
    let path = dir.join("corgi.log");
    let file = std::fs::OpenOptions::new().create(true).append(true).open(&path)?;
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_env("CORGI_LOG").unwrap_or_else(|_| EnvFilter::new("info")))
        .with_writer(std::sync::Mutex::new(file))
        .with_ansi(false)
        .init();
    Ok(path)
}
