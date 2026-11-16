//! Pass notifications on to the terminal corgi runs in, as escape sequences it turns into
//! desktop notifications. Nothing outside the terminal is involved (no D-Bus, no
//! `osascript`), so it works over ssh too. Pure: it builds bytes; `term` writes them.
//!
//! Terminals can't be asked which of these they understand, so `auto` goes by what the
//! environment says the terminal is, and rings the bell when it can't tell.

use crate::config::HostNotify;

/// What the host terminal is sent.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Proto {
    Off,
    /// BEL: a sound, bounce or badge; everything has it.
    Bell,
    /// `OSC 9 ; body`: iTerm2, and others; no title.
    Osc9,
    /// `OSC 777 ; notify ; title ; body`: Ghostty, WezTerm, foot, urxvt, corgi.
    Osc777,
    /// `OSC 99`: kitty's own protocol.
    Osc99,
}

/// The protocol for `setting`, reading the environment through `env` for `auto`.
pub fn resolve(setting: HostNotify, env: impl Fn(&str) -> Option<String>) -> Proto {
    match setting {
        HostNotify::Off => Proto::Off,
        HostNotify::Bell => Proto::Bell,
        HostNotify::Osc9 => Proto::Osc9,
        HostNotify::Osc777 => Proto::Osc777,
        HostNotify::Osc99 => Proto::Osc99,
        HostNotify::Auto => detect(env),
    }
}

fn detect(env: impl Fn(&str) -> Option<String>) -> Proto {
    let program = env("TERM_PROGRAM").unwrap_or_default().to_ascii_lowercase();
    let term = env("TERM").unwrap_or_default();
    // Survives ssh and tmux, where TERM_PROGRAM doesn't.
    let lc = env("LC_TERMINAL").unwrap_or_default().to_ascii_lowercase();
    if env("KITTY_WINDOW_ID").is_some() || term == "xterm-kitty" {
        return Proto::Osc99;
    }
    if matches!(program.as_str(), "ghostty" | "wezterm" | "corgi")
        || term == "xterm-ghostty"
        || term == "wezterm"
        || term.starts_with("foot")
        || term.starts_with("rxvt-unicode")
    {
        return Proto::Osc777;
    }
    if program == "iterm.app" || lc == "iterm2" {
        return Proto::Osc9;
    }
    Proto::Bell
}

/// The bytes that show `title`/`body` on the host. `id` tells kitty notifications apart.
/// Inside tmux (`tmux`), sequences are wrapped to pass through it to the terminal outside
/// (needs `set -g allow-passthrough on`); the bell needs no wrapping.
pub fn encode(proto: Proto, title: &str, body: &str, id: u64, tmux: bool) -> Vec<u8> {
    // Escape sequences end at BEL or ESC: none may get through from what programs wrote.
    let title = crate::toast::clean(title);
    let body = crate::toast::clean(body);
    let seq = match proto {
        Proto::Off => return Vec::new(),
        Proto::Bell => return b"\x07".to_vec(),
        Proto::Osc9 => {
            let text = match (title.is_empty(), body.is_empty()) {
                (true, _) => body,
                (false, true) => title,
                (false, false) => format!("{title}: {body}"),
            };
            // `9;4…` would be read as a progress report.
            let text = if text.starts_with(|c: char| c.is_ascii_digit()) {
                format!(" {text}")
            } else {
                text
            };
            format!("\x1b]9;{text}\x07")
        }
        Proto::Osc777 => {
            let title = if title.is_empty() {
                "corgi".into()
            } else {
                title.replace(';', ",")
            };
            format!("\x1b]777;notify;{title};{body}\x07")
        }
        Proto::Osc99 => {
            // Title first, not done (d=0); then the body, which completes it.
            let title = if title.is_empty() { "corgi".into() } else { title };
            format!("\x1b]99;i=h{id}:d=0:p=title;{title}\x1b\\\x1b]99;i=h{id}:d=1:p=body;{body}\x1b\\")
        }
    };
    if tmux {
        passthrough(seq.as_bytes())
    } else {
        seq.into_bytes()
    }
}

/// The bytes that put `text` on the host terminal's clipboard (OSC 52), wrapped for tmux inside
/// tmux. Terminals that allow it (most do; some ask first) copy it to the system clipboard.
pub fn clipboard(text: &str, tmux: bool) -> Vec<u8> {
    let seq = format!("\x1b]52;c;{}\x07", base64(text.as_bytes()));
    if tmux {
        passthrough(seq.as_bytes())
    } else {
        seq.into_bytes()
    }
}

/// Standard base64, padded.
fn base64(bytes: &[u8]) -> String {
    const A: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let n = chunk
            .iter()
            .enumerate()
            .fold(0u32, |acc, (i, &b)| acc | (b as u32) << (16 - 8 * i));
        for i in 0..4 {
            if i <= chunk.len() {
                out.push(A[(n >> (18 - 6 * i) & 63) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

/// tmux's DCS passthrough: `ESC P tmux ;` the sequence with each ESC doubled `ESC \`.
fn passthrough(seq: &[u8]) -> Vec<u8> {
    let mut out = b"\x1bPtmux;".to_vec();
    for &b in seq {
        if b == 0x1b {
            out.push(0x1b);
        }
        out.push(b);
    }
    out.extend_from_slice(b"\x1b\\");
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn env<'a>(vars: &'a [(&'a str, &'a str)]) -> impl Fn(&str) -> Option<String> + 'a {
        move |k| vars.iter().find(|(n, _)| *n == k).map(|(_, v)| v.to_string())
    }

    #[test]
    fn auto_goes_by_the_terminal() {
        let a = |v: &[(&str, &str)]| resolve(HostNotify::Auto, env(v));
        assert_eq!(a(&[("TERM_PROGRAM", "ghostty")]), Proto::Osc777);
        assert_eq!(a(&[("TERM", "xterm-ghostty"), ("TERM_PROGRAM", "tmux")]), Proto::Osc777);
        assert_eq!(a(&[("TERM_PROGRAM", "WezTerm")]), Proto::Osc777);
        assert_eq!(a(&[("TERM_PROGRAM", "iTerm.app")]), Proto::Osc9);
        assert_eq!(a(&[("TERM_PROGRAM", "tmux"), ("LC_TERMINAL", "iTerm2")]), Proto::Osc9);
        assert_eq!(a(&[("TERM", "xterm-kitty")]), Proto::Osc99);
        assert_eq!(a(&[("TERM", "foot")]), Proto::Osc777);
        assert_eq!(a(&[("TERM_PROGRAM", "Apple_Terminal")]), Proto::Bell);
        assert_eq!(a(&[]), Proto::Bell);
        assert_eq!(
            resolve(HostNotify::Off, env(&[("TERM_PROGRAM", "ghostty")])),
            Proto::Off
        );
        assert_eq!(resolve(HostNotify::Osc9, env(&[])), Proto::Osc9);
    }

    #[test]
    fn sequences() {
        let e = |p, t, b| String::from_utf8(encode(p, t, b, 7, false)).unwrap();
        assert_eq!(e(Proto::Osc9, "", "build done"), "\x1b]9;build done\x07");
        assert_eq!(e(Proto::Osc9, "claude", "needs you"), "\x1b]9;claude: needs you\x07");
        assert_eq!(
            e(Proto::Osc9, "", "4 tests failed"),
            "\x1b]9; 4 tests failed\x07",
            "not progress"
        );
        assert_eq!(e(Proto::Osc777, "a;b", "c;d"), "\x1b]777;notify;a,b;c;d\x07");
        assert_eq!(e(Proto::Osc777, "", "x"), "\x1b]777;notify;corgi;x\x07");
        assert_eq!(
            e(Proto::Osc99, "t", "b"),
            "\x1b]99;i=h7:d=0:p=title;t\x1b\\\x1b]99;i=h7:d=1:p=body;b\x1b\\"
        );
        assert_eq!(e(Proto::Bell, "t", "b"), "\x07");
        assert!(encode(Proto::Off, "t", "b", 0, false).is_empty());
    }

    #[test]
    fn nothing_escapes_the_sequence() {
        let s = String::from_utf8(encode(Proto::Osc777, "x\x07\x1b]0;pwn", "y\x1b\\z", 0, false)).unwrap();
        assert_eq!(s.matches('\x1b').count(), 1, "{s:?}");
        assert_eq!(s.matches('\x07').count(), 1, "{s:?}");
    }

    #[test]
    fn clipboard_sequences() {
        assert_eq!(base64(b""), "");
        assert_eq!(base64(b"a"), "YQ==");
        assert_eq!(base64(b"hello"), "aGVsbG8=");
        assert_eq!(base64("héllo wörld\n".as_bytes()), "aMOpbGxvIHfDtnJsZAo=");
        assert_eq!(clipboard("hi", false), b"\x1b]52;c;aGk=\x07");
        assert!(clipboard("hi", true).starts_with(b"\x1bPtmux;\x1b\x1b]52;"));
    }

    #[test]
    fn tmux_passthrough_doubles_escapes() {
        assert_eq!(
            encode(Proto::Osc99, "t", "", 1, true).windows(7).next().unwrap(),
            b"\x1bPtmux;"
        );
        assert_eq!(passthrough(b"\x1b]9;x\x07"), b"\x1bPtmux;\x1b\x1b]9;x\x07\x1b\\");
        assert_eq!(encode(Proto::Bell, "", "", 0, true), b"\x07");
    }
}
