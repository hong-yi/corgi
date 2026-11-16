//! Usage in the tab bar's free space: segments (agents, Claude and Codex usage, a command's
//! output, …) placed by priority into whatever the window tabs leave, each in the widest of
//! its renderings that fits (full, short, icon), never pushing a tab out. Pure: providers
//! (`usage_sources`) produce `Reading`s; this places and formats them.

use std::collections::BTreeMap;

/// One segment's latest value.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Reading {
    /// Values a `format` can use (`{tokens_today}`); also shown in the details dialog.
    pub fields: BTreeMap<String, String>,
    pub full: String,
    pub short: String,
    pub icon: String,
    /// The details dialog's lines for it; `true` for a warning (little left).
    pub detail: Vec<(String, bool)>,
}

impl Reading {
    /// The renderings, widest first, without empty or repeated ones.
    fn renderings(&self) -> Vec<&str> {
        let mut out: Vec<&str> = Vec::new();
        for s in [&self.full, &self.short, &self.icon] {
            if !s.is_empty() && !out.contains(&s.as_str()) {
                out.push(s);
            }
        }
        out
    }
}

/// Each segment is a block: its text with a space either side, on its own background…
pub const PAD: usize = 2;
/// …and a column between blocks.
pub const GAP: usize = 1;

/// What goes where in `free` columns: `(index, text)` in the segments' own order, left to right.
/// Highest `priority` first gets its widest rendering that still fits; a segment that can't fit
/// at all is skipped, and smaller lower-priority ones may still go in.
pub fn place(free: usize, segments: &[(i32, Option<&Reading>)]) -> Vec<(usize, String)> {
    let mut order: Vec<usize> = (0..segments.len()).filter(|&i| segments[i].1.is_some()).collect();
    // Stable: equal priorities keep their configured order.
    order.sort_by_key(|&i| std::cmp::Reverse(segments[i].0));
    let mut chosen: Vec<(usize, String)> = Vec::new();
    let width = |s: &str| unicode_width::UnicodeWidthStr::width(s);
    let used =
        |c: &[(usize, String)]| c.iter().map(|(_, t)| width(t) + PAD).sum::<usize>() + GAP * c.len().saturating_sub(1);
    for i in order {
        let r = segments[i].1.unwrap();
        for text in r.renderings() {
            let gap = if chosen.is_empty() { 0 } else { GAP };
            if used(&chosen) + gap + width(text) + PAD <= free {
                chosen.push((i, text.to_string()));
                break;
            }
        }
    }
    chosen.sort_by_key(|(i, _)| *i);
    chosen
}

/// Fill `{field}`s from `fields`; unknown ones stay as they are written.
pub fn fill(template: &str, fields: &BTreeMap<String, String>) -> String {
    let mut out = String::new();
    let mut rest = template;
    while let Some(start) = rest.find('{') {
        out.push_str(&rest[..start]);
        match rest[start..].find('}') {
            Some(end) => {
                let key = &rest[start + 1..start + end];
                match fields.get(key) {
                    Some(v) => out.push_str(v),
                    None => out.push_str(&rest[start..=start + end]),
                }
                rest = &rest[start + end + 1..];
            }
            None => {
                out.push_str(&rest[start..]);
                rest = "";
            }
        }
    }
    out.push_str(rest);
    out
}

/// `950`, `12.3k`, `4.5M`, `1.2B`.
pub fn count(n: u64) -> String {
    match n {
        0..=999 => n.to_string(),
        1_000..=999_999 => trim(format!("{:.1}k", n as f64 / 1e3)),
        1_000_000..=999_999_999 => trim(format!("{:.1}M", n as f64 / 1e6)),
        _ => trim(format!("{:.1}B", n as f64 / 1e9)),
    }
}

fn trim(s: String) -> String {
    s.replace(".0k", "k").replace(".0M", "M").replace(".0B", "B")
}

/// `45s`, `12m`, `2h13m`, `3d4h`.
pub fn span(secs: i64) -> String {
    let s = secs.max(0);
    match s {
        0..=59 => format!("{s}s"),
        60..=3599 => format!("{}m", s / 60),
        3600..=86399 => format!("{}h{:02}m", s / 3600, s % 3600 / 60),
        _ => format!("{}d{}h", s / 86400, s % 86400 / 3600),
    }
}

/// Seconds since the epoch for an RFC 3339 UTC time (`2026-11-09T07:04:25.307Z`).
pub fn parse_time(s: &str) -> Option<i64> {
    let b = s.as_bytes();
    if b.len() < 19 || b[4] != b'-' || b[7] != b'-' || b[10] != b'T' {
        return None;
    }
    let n = |r: std::ops::Range<usize>| s.get(r)?.parse::<i64>().ok();
    let (y, mo, d) = (n(0..4)?, n(5..7)?, n(8..10)?);
    let (h, mi, se) = (n(11..13)?, n(14..16)?, n(17..19)?);
    // Offsets other than Z: `+09:00`, `-05:00` after the seconds (and any fraction).
    let tail = &s[19..];
    let tail = tail.trim_start_matches(|c: char| c == '.' || c.is_ascii_digit());
    let offset = match tail.as_bytes().first() {
        Some(b'+' | b'-') if tail.len() >= 6 => {
            let sign = if tail.starts_with('-') { -1 } else { 1 };
            sign * (tail[1..3].parse::<i64>().ok()? * 3600 + tail[4..6].parse::<i64>().ok()? * 60)
        }
        _ => 0,
    };
    Some(days_from_civil(y, mo, d) * 86400 + h * 3600 + mi * 60 + se - offset)
}

/// Days since 1970-01-01 (Howard Hinnant's algorithm).
fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let doy = (153 * (m + if m > 2 { -3 } else { 9 }) + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146097 + doe - 719468
}

/// The Claude Code 5-hour usage blocks, as ccusage counts them: a block starts at the hour
/// of the first message after the last block ended, and lasts five hours. Every block in
/// `entries` (time, tokens), oldest first, as (start, tokens).
pub fn blocks(mut entries: Vec<(i64, u64)>) -> Vec<(i64, u64)> {
    entries.sort_by_key(|e| e.0);
    let mut out: Vec<(i64, u64)> = Vec::new();
    for (ts, tokens) in entries {
        match out.last_mut() {
            Some((start, n)) if ts < *start + BLOCK => *n += tokens,
            _ => out.push((ts - ts.rem_euclid(3600), tokens)),
        }
    }
    out
}

pub const BLOCK: i64 = 5 * 3600;

/// The current block's (start, tokens), if `now` is in one.
#[cfg(test)]
pub fn current_block(entries: Vec<(i64, u64)>, now: i64) -> Option<(i64, u64)> {
    blocks(entries).pop().filter(|(start, _)| now < start + BLOCK)
}

/// A bar `width` cells wide, `fraction` (0–1) of it full, in eighths: `█████▍░░░░`.
pub fn bar(fraction: f64, width: usize) -> String {
    const PARTS: [&str; 8] = ["", "▏", "▎", "▍", "▌", "▋", "▊", "▉"];
    let eighths = (fraction.clamp(0.0, 1.0) * (width * 8) as f64).round() as usize;
    let (full, part) = (eighths / 8, eighths % 8);
    let mut s = "█".repeat(full);
    if full < width {
        s.push_str(PARTS[part]);
        let used = full + usize::from(part > 0);
        s.push_str(&"░".repeat(width - used));
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    fn r(full: &str, short: &str, icon: &str) -> Reading {
        Reading {
            full: full.into(),
            short: short.into(),
            icon: icon.into(),
            ..Default::default()
        }
    }

    #[test]
    fn placing_by_priority_and_width() {
        let agents = r("2 working · 1 waiting", "●2 !1", "●3");
        let claude = r("claude 1.2M today · 2h13m left", "claude 1.2M", "✻");
        let clock = r("14:05", "", "");
        let segs = [(90, Some(&agents)), (60, Some(&claude)), (10, Some(&clock))];
        // Room for everything, in full, in their order.
        let all = place(300, &segs);
        assert_eq!(all.iter().map(|p| p.0).collect::<Vec<_>>(), [0, 1, 2]);
        assert_eq!(all[1].1, "claude 1.2M today · 2h13m left");
        // Less room: the highest priority stays full, the rest shrink.
        let some = place(40, &segs);
        assert_eq!(some[0].1, "2 working · 1 waiting");
        assert_eq!(some[1].1, "claude 1.2M", "shorter, to fit");
        let w = |t: &str| unicode_width::UnicodeWidthStr::width(t) + PAD;
        assert!(some.iter().map(|p| w(&p.1)).sum::<usize>() + GAP * (some.len() - 1) <= 40);
        // Very little: icons (with their padding); nothing fits: nothing.
        assert_eq!(place(6, &segs), vec![(0, "●3".into())]);
        assert_eq!(place(3, &segs), vec![(1, "✻".into())], "the only thing that fits");
        assert!(place(2, &segs).is_empty());
        // A segment with no reading yet takes no room.
        assert_eq!(place(300, &[(5, None), (1, Some(&clock))]), vec![(1, "14:05".into())]);
        // A big one that can't fit doesn't keep a smaller lower one out.
        let wide = r("wwwwwwwwwwwwwwwwwwww", "", "");
        assert_eq!(
            place(10, &[(9, Some(&wide)), (1, Some(&clock))]),
            vec![(1, "14:05".into())]
        );
    }

    #[test]
    fn formatting() {
        let f: BTreeMap<String, String> = [("a".to_string(), "1".to_string())].into();
        assert_eq!(fill("{a} and {b} {", &f), "1 and {b} {");
        assert_eq!(count(950), "950");
        assert_eq!(count(12_345), "12.3k");
        assert_eq!(count(4_000_000), "4M");
        assert_eq!(span(42), "42s");
        assert_eq!(span(8000), "2h13m");
        assert_eq!(span(3 * 86400 + 4 * 3600), "3d4h");
    }

    #[test]
    fn bars() {
        assert_eq!(bar(0.0, 4), "░░░░");
        assert_eq!(bar(1.0, 4), "████");
        assert_eq!(bar(0.5, 4), "██░░");
        assert_eq!(bar(0.3, 4), "█▎░░", "in eighths: 9.6 of 32 rounds to 10");
        assert_eq!(bar(2.0, 3), "███", "clamped");
        for f in [0.0, 0.13, 0.5, 0.77, 1.0] {
            assert_eq!(bar(f, 20).chars().count(), 20, "always {f}'s full width");
        }
        let past = blocks(vec![(0, 5), (60, 5), (6 * 3600, 7)]);
        assert_eq!(past, vec![(0, 10), (6 * 3600, 7)]);
    }

    #[test]
    fn times_and_blocks() {
        assert_eq!(parse_time("1970-01-01T00:00:00Z"), Some(0));
        assert_eq!(parse_time("2026-11-09T07:04:25.307Z"), Some(1_794_207_865));
        assert_eq!(parse_time("2026-11-09T16:04:25+09:00"), Some(1_794_207_865));
        assert_eq!(parse_time("nope"), None);
        let h = 3600;
        let t0 = 1_000 * h + 1_234; // 20:34 into hour 1000
        // Messages within five hours of the block's start (its hour) count to it.
        let e = vec![(t0, 10), (t0 + h, 20), (t0 + 4 * h, 5)];
        assert_eq!(current_block(e.clone(), t0 + 4 * h + 60), Some((1_000 * h, 35)));
        // After it ends, there's no block until the next message.
        assert_eq!(current_block(e.clone(), t0 + 6 * h), None);
        // The next message starts a new one, at its hour.
        let mut e2 = e;
        e2.push((t0 + 7 * h, 1));
        assert_eq!(current_block(e2, t0 + 7 * h + 5), Some((1_007 * h, 1)));
    }
}
