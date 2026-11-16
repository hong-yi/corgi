//! Ask the host terminal for its colours and cell size, so core can answer the
//! program's colour and pixel queries as the person's terminal would (FACTS).
//! Runs once, in raw mode, before crossterm starts reading input.

use hc_proto::Display;

#[cfg(unix)]
pub fn query() -> Option<Display> {
    use std::io::Write;
    use std::time::{Duration, Instant};
    // fg, bg, cursor colour, cell size in pixels, then DA1, which every terminal answers: our end marker.
    let ask = b"\x1b]10;?\x1b\\\x1b]11;?\x1b\\\x1b]12;?\x1b\\\x1b[16t\x1b[c";
    let mut out = std::io::stdout();
    out.write_all(ask).ok()?;
    out.flush().ok()?;
    let deadline = Instant::now() + Duration::from_millis(400);
    let mut got = Vec::new();
    let mut buf = [0u8; 512];
    while !has_da1(&got) {
        let left = deadline.saturating_duration_since(Instant::now());
        if left.is_zero() {
            break;
        }
        let mut pfd = libc::pollfd {
            fd: 0,
            events: libc::POLLIN,
            revents: 0,
        };
        // SAFETY: one valid pollfd; stdin stays open for the process lifetime.
        let ready = unsafe { libc::poll(&mut pfd, 1, left.as_millis() as libc::c_int) };
        if ready <= 0 {
            break;
        }
        // SAFETY: reading into a stack buffer of the given length.
        let n = unsafe { libc::read(0, buf.as_mut_ptr().cast(), buf.len()) };
        if n <= 0 {
            break;
        }
        got.extend_from_slice(&buf[..n as usize]);
    }
    let d = parse(&got);
    tracing::info!(?d, answered = has_da1(&got), "host display facts");
    (d != Display::default()).then_some(d)
}

#[cfg(not(unix))]
pub fn query() -> Option<Display> {
    None
}

fn has_da1(b: &[u8]) -> bool {
    // ESC [ ? ... c
    b.windows(3)
        .enumerate()
        .any(|(i, w)| w == b"\x1b[?" && b[i + 3..].iter().take(64).any(|&c| c == b'c'))
}

/// Parse answers: `ESC ] 10 ; rgb:RRRR/GGGG/BBBB (BEL|ST)`, `ESC [ 6 ; h ; w t`.
pub fn parse(b: &[u8]) -> Display {
    let s = String::from_utf8_lossy(b);
    let color = |n: &str| {
        let start = s.find(&format!("\x1b]{n};rgb:"))? + n.len() + 7;
        let rest = &s[start..];
        let end = rest.find(['\x07', '\x1b'])?;
        let parts: Vec<&str> = rest[..end].split('/').collect();
        let [r, g, bl] = parts.as_slice() else { return None };
        let scale = |h: &str| -> Option<u8> {
            let v = u32::from_str_radix(h, 16).ok()?;
            let max = (1u32 << (4 * h.len() as u32)) - 1;
            Some((v * 255 / max.max(1)) as u8)
        };
        Some(format!("#{:02x}{:02x}{:02x}", scale(r)?, scale(g)?, scale(bl)?))
    };
    let cell = s.find("\x1b[6;").and_then(|i| {
        let rest = &s[i + 4..];
        let end = rest.find('t')?;
        let mut it = rest[..end].split(';').map(|p| p.parse::<u16>().ok());
        let (h, w) = (it.next()??, it.next()??);
        (w > 0 && h > 0).then_some([w, h])
    });
    Display {
        fg: color("10"),
        bg: color("11"),
        cursor: color("12"),
        cell,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_colour_and_cell_answers() {
        let reply = b"\x1b]10;rgb:cdcd/d6d6/f4f4\x1b\\\x1b]11;rgb:1e/1e/2e\x07\x1b[6;18;9t\x1b[?62;22c";
        let d = parse(reply);
        assert_eq!(d.fg.as_deref(), Some("#cdd6f4"));
        assert_eq!(d.bg.as_deref(), Some("#1e1e2e"));
        assert_eq!(d.cursor, None);
        assert_eq!(d.cell, Some([9, 18]));
        assert!(has_da1(reply));
        assert!(!has_da1(b"\x1b]10;rgb:0/0/0\x07"));
    }
}
