//! Where corgi keeps state and config, per OS.

use std::path::PathBuf;

pub fn home() -> PathBuf {
    std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" })
        .map(PathBuf::from)
        .unwrap_or_default()
}

fn env_dir(key: &str) -> Option<PathBuf> {
    std::env::var_os(key).filter(|v| !v.is_empty()).map(PathBuf::from)
}

/// Logs, backups: `$XDG_STATE_HOME/corgi` (`%LOCALAPPDATA%\corgi` on Windows).
pub fn state_dir() -> PathBuf {
    if cfg!(windows) {
        return env_dir("LOCALAPPDATA").unwrap_or_else(home).join("corgi");
    }
    env_dir("XDG_STATE_HOME")
        .unwrap_or_else(|| home().join(".local/state"))
        .join("corgi")
}

/// Config: `$XDG_CONFIG_HOME/corgi` (`%APPDATA%\corgi` on Windows).
pub fn config_dir() -> PathBuf {
    if cfg!(windows) {
        return env_dir("APPDATA").unwrap_or_else(home).join("corgi");
    }
    env_dir("XDG_CONFIG_HOME")
        .unwrap_or_else(|| home().join(".config"))
        .join("corgi")
}
