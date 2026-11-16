//! corgi inside corgi. A corgi running in one of its own panes would show
//! that pane, which shows corgi, which shows the pane… and every key would
//! loop back into itself. These checks keep that from happening, and point
//! at it when an older or forced corgi does it anyway.

use crate::model;
use hc_proto::{Client, Terminal};

/// The core terminal this process runs in, if it is on the core we're talking to.
/// `HC_TERMINAL_ID` alone could name a terminal on another core, so also check that its
/// program leads our session (core starts each program as a session leader).
pub async fn own_terminal(client: &Client) -> Option<String> {
    let id = std::env::var("HC_TERMINAL_ID").ok()?;
    let rec = client.get(&id).await.ok()?;
    #[cfg(unix)]
    {
        // SAFETY: getsid(0) only reads this process's session id.
        let sid = unsafe { libc::getsid(0) } as i64;
        (rec.pid == Some(sid)).then_some(id)
    }
    #[cfg(not(unix))]
    {
        let _ = rec;
        Some(id)
    }
}

/// corgi shows this terminal as a pane (it carries corgi's labels).
pub fn is_corgi_pane(t: &Terminal) -> bool {
    t.labels.contains_key(model::L_WORKSPACE) || t.labels.contains_key(model::L_M2)
}

/// The program in the foreground of this terminal is corgi.
pub fn runs_corgi(t: &Terminal) -> bool {
    let Some(f) = &t.foreground else { return false };
    let base = |s: &str| {
        s.rsplit(['/', '\\'])
            .next()
            .unwrap_or(s)
            .trim_end_matches(".exe")
            .to_string()
    };
    base(&f.name) == "corgi" || f.argv.first().is_some_and(|a| base(a) == "corgi")
}

#[cfg(test)]
mod tests {
    use super::*;
    use hc_proto::Foreground;

    fn with_fg(argv0: &str) -> Terminal {
        let fg = Foreground {
            pid: 7,
            name: argv0.rsplit('/').next().unwrap().into(),
            argv: vec![argv0.into()],
            cwd: "/".into(),
        };
        Terminal {
            foreground: Some(fg),
            ..Default::default()
        }
    }

    #[test]
    fn recognises_corgi_by_program_name() {
        assert!(runs_corgi(&with_fg("target/debug/corgi")));
        assert!(runs_corgi(&with_fg("/usr/local/bin/corgi")));
        assert!(!runs_corgi(&with_fg("/bin/zsh")));
        assert!(!runs_corgi(&with_fg("corgitool")));
        assert!(!runs_corgi(&Terminal::default()));
        let mut t = Terminal::default();
        t.labels.insert(model::L_WORKSPACE.into(), "s1".into());
        assert!(is_corgi_pane(&t));
    }
}
