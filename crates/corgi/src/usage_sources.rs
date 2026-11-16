//! Where usage segments get their values: Claude Code's and Codex's local logs, a command,
//! the clock, the host, the load. Each `Source` keeps what it needs between reads (the log
//! readers remember how far into each file they got, so a read only parses what's new).
//! Reads block (files, processes): the hub runs them off the UI task.

use crate::config::{Limit, Segment, Show};
use crate::usage::{BLOCK, Reading, bar, blocks, count, parse_time, span};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

pub enum Source {
    /// The logs, what to show, the estimate's budget, where `corgi claude-statusline` saves
    /// Claude Code's own rate limits.
    Claude(Logs, Show, Option<Limit>, PathBuf),
    Codex(Logs, Show),
    Command {
        line: String,
        timeout: Duration,
    },
    Clock,
    Host,
    System,
}

impl Source {
    /// The source a segment's `provider` names (`agents` isn't one: corgi counts those itself).
    pub fn new(seg: &Segment) -> Option<Source> {
        let (provider, cmd, timeout) = (seg.provider.as_str(), seg.cmd.as_deref(), seg.timeout);
        let home = crate::paths::home();
        // Where the tools keep their logs: their own variables first, as they read them.
        let dir = |var: &str, default: &str| {
            std::env::var_os(var)
                .filter(|v| !v.is_empty())
                .map_or_else(|| home.join(default), PathBuf::from)
        };
        Some(match provider {
            "claude" => Source::Claude(
                Logs::new(dir("CLAUDE_CONFIG_DIR", ".claude").join("projects")),
                seg.show,
                seg.limit.clone(),
                crate::claude_tap::path(),
            ),
            "codex" => Source::Codex(Logs::new(dir("CODEX_HOME", ".codex").join("sessions")), seg.show),
            "command" => Source::Command {
                line: cmd?.to_string(),
                timeout,
            },
            "clock" => Source::Clock,
            "host" => Source::Host,
            "system" => Source::System,
            _ => return None,
        })
    }

    /// The latest value, or `None` when there's nothing to show (no usage today, a failed command).
    pub fn read(&mut self, now: SystemTime) -> Option<Reading> {
        let now = now
            .duration_since(SystemTime::UNIX_EPOCH)
            .map_or(0, |d| d.as_secs() as i64);
        match self {
            Source::Claude(logs, show, limit, tap) => claude(logs, now, *show, limit.as_ref(), tap),
            Source::Codex(logs, show) => codex(logs, now, *show),
            Source::Command { line, timeout } => command(line, *timeout),
            Source::Clock => {
                let t = (now + local_offset(now)).rem_euclid(86400);
                let s = format!("{:02}:{:02}", t / 3600, t % 3600 / 60);
                Some(reading(
                    &[("time", &s)],
                    s.clone(),
                    String::new(),
                    String::new(),
                    vec![],
                ))
            }
            Source::Host => Some(host_reading(&hostname())),
            Source::System => Some(load_reading(load()?)),
        }
    }

    /// What to run on another machine (through `machine.exec`) to read it there; `None` for
    /// sources that only make sense here (the logs, the clock).
    pub fn remote_line(&self) -> Option<String> {
        match self {
            Source::Command { line, .. } => Some(line.clone()),
            Source::Host => Some("hostname".into()),
            Source::System => Some("cat /proc/loadavg 2>/dev/null || sysctl -n vm.loadavg".into()),
            _ => None,
        }
    }

    /// The reading from what `remote_line` printed there.
    pub fn remote_reading(&self, out: &str) -> Option<Reading> {
        match self {
            Source::Command { .. } => command_reading(out),
            Source::Host => Some(host_reading(out.lines().next()?.trim())).filter(|r| !r.full.is_empty()),
            Source::System => {
                let n: Vec<f64> = out
                    .split(|c: char| c.is_whitespace() || c == '{' || c == '}')
                    .filter_map(|w| w.parse().ok())
                    .take(3)
                    .collect();
                (n.len() == 3).then(|| load_reading([n[0], n[1], n[2]]))
            }
            _ => None,
        }
    }
}

fn host_reading(h: &str) -> Reading {
    let short = h.split('.').next().unwrap_or(h).to_string();
    reading(
        &[("host", h)],
        short,
        String::new(),
        String::new(),
        vec![(h.to_string(), false)],
    )
}

fn load_reading(l: [f64; 3]) -> Reading {
    let s = format!("{:.2}", l[0]);
    let detail = vec![(
        format!("load {:.2} {:.2} {:.2} (1, 5, 15 min)", l[0], l[1], l[2]),
        false,
    )];
    reading(&[("load", &s)], format!("load {s}"), s.clone(), String::new(), detail)
}

fn reading(fields: &[(&str, &str)], full: String, short: String, icon: String, detail: Vec<(String, bool)>) -> Reading {
    Reading {
        fields: fields.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect(),
        full,
        short,
        icon,
        detail,
    }
}

/// One usage record: (time, tokens, input, output, cache written, cache read); Codex fills
/// only the first two.
type Entry = (i64, u64, u64, u64, u64, u64);

/// JSONL logs read a bit at a time: each file from where the last read stopped.
pub struct Logs {
    root: PathBuf,
    offsets: HashMap<PathBuf, u64>,
    /// The `.jsonl` files found under the root, and when they were looked for (they're looked
    /// for again every `REDISCOVER`; in between, only these are checked).
    files: Vec<PathBuf>,
    found_at: Option<i64>,
    /// Claude repeats a message's usage on each of its parts: count each (message, request)
    /// once. Kept as long as what it counted is (a week), with when it was.
    seen: HashMap<String, i64>,
    entries: Vec<Entry>,
    /// Codex: the latest rate-limit report and when.
    limits: Option<(i64, serde_json::Value)>,
}

impl Logs {
    pub fn new(root: PathBuf) -> Logs {
        Logs {
            root,
            offsets: HashMap::new(),
            files: Vec::new(),
            found_at: None,
            seen: HashMap::new(),
            entries: Vec::new(),
            limits: None,
        }
    }

    /// New complete lines of every `.jsonl` file under the root changed within a week, at most
    /// `CHUNK` of each per call (the rest the next time).
    fn new_lines(&mut self, now: i64, mut each: impl FnMut(&str)) {
        if self.found_at.is_none_or(|t| now - t >= REDISCOVER) {
            self.files.clear();
            walk(&self.root, 4, &mut self.files);
            self.found_at = Some(now);
            // Files that went: nothing to remember about them.
            let files: HashSet<&PathBuf> = self.files.iter().collect();
            self.offsets.retain(|p, _| files.contains(p));
        }
        // A week: today, and the past blocks the "max" limit is estimated from.
        let since = SystemTime::UNIX_EPOCH + Duration::from_secs((now - 7 * 86400).max(0) as u64);
        for path in self.files.clone() {
            let Ok(meta) = std::fs::metadata(&path) else { continue };
            if meta.modified().is_ok_and(|m| m < since) {
                continue;
            }
            let from = self.offsets.get(&path).copied().unwrap_or(0);
            // A file that shrank was replaced: start it over.
            let from = if meta.len() < from { 0 } else { from };
            if meta.len() == from {
                continue;
            }
            let Ok(mut f) = std::fs::File::open(&path) else {
                continue;
            };
            if f.seek(SeekFrom::Start(from)).is_err() {
                continue;
            }
            let mut buf = Vec::new();
            if f.take(CHUNK).read_to_end(&mut buf).is_err() {
                continue;
            }
            // Only up to the last newline: a line being written now waits for the next read.
            let Some(end) = buf.iter().rposition(|&b| b == b'\n') else {
                // A whole chunk without one: a line too long to be usage; past it.
                if buf.len() as u64 == CHUNK {
                    self.offsets.insert(path, from + CHUNK);
                }
                continue;
            };
            for line in buf[..end].split(|&b| b == b'\n') {
                if let Ok(s) = std::str::from_utf8(line) {
                    each(s);
                }
            }
            self.offsets.insert(path, from + end as u64 + 1);
        }
        self.entries.retain(|e| e.0 >= now - 7 * 86400);
    }
}

/// How much of a log file one poll reads at most.
const CHUNK: u64 = 4 * 1024 * 1024;
/// How often the log folders are looked through for new files (seconds).
const REDISCOVER: i64 = 60;

fn walk(dir: &Path, depth: u8, out: &mut Vec<PathBuf>) {
    let Ok(rd) = std::fs::read_dir(dir) else { return };
    for e in rd.flatten() {
        let p = e.path();
        if p.is_dir() {
            if depth > 0 {
                walk(&p, depth - 1, out);
            }
        } else if p.extension().is_some_and(|x| x == "jsonl") {
            out.push(p);
        }
    }
}

/// One line of a bar: `5-hour block  ██████▍░░░  52% left · resets in 2h13m`, a warning under 20%.
fn bar_line(name: &str, left: f64, tail: &str) -> (String, bool) {
    (
        format!("{name:<13} {}  {left:.0}% left{tail}", bar(left / 100.0, 20)),
        left < 20.0,
    )
}

/// What a segment says from a share left (0–100) of `what`, per `show`.
fn shares(app: &str, show: Show, left: f64, what: &str, extra: &str) -> (String, String, String) {
    let (n, word) = match show {
        Show::Used => (100.0 - left, "used"),
        _ => (left, "left"),
    };
    (
        format!("{app} {n:.0}% {word}{what}{extra}"),
        format!("{app} {n:.0}%"),
        format!("{n:.0}%"),
    )
}

/// A rate-limit window: its name, the share left (0–100), when it resets.
type Window = (String, f64, Option<i64>);

/// Claude Code's rate limits as `corgi claude-statusline` saved them: when, and each window
/// (name, share left, reset). A window past its reset is fresh again.
fn tapped(tap: &Path, now: i64) -> Option<(i64, Vec<Window>)> {
    let v: serde_json::Value = serde_json::from_slice(&std::fs::read(tap).ok()?).ok()?;
    let mut windows = Vec::new();
    for (key, name) in [("five_hour", "5h"), ("seven_day", "week")] {
        let w = &v[key];
        let Some(used) = w["used_percentage"].as_f64() else {
            continue;
        };
        let resets = w["resets_at"].as_i64().filter(|r| *r > 0);
        let used = if resets.is_some_and(|r| r <= now) {
            0.0
        } else {
            used.clamp(0.0, 100.0)
        };
        windows.push((name.to_string(), 100.0 - used, resets));
    }
    (!windows.is_empty()).then(|| (v["at"].as_i64().unwrap_or(0), windows))
}

/// Claude Code: how much of its rate limits is left, and tokens today. The limits are Claude
/// Code's own, as `corgi claude-statusline` saved them from its status line; without them,
/// an estimate if `limit` asks for one (a budget: a number, or `max`, the most any finished
/// block used this week, as ccusage estimates it), else tokens.
fn claude(logs: &mut Logs, now: i64, show: Show, limit: Option<&Limit>, tap: &Path) -> Option<Reading> {
    let mut fresh = Vec::new();
    let mut seen = std::mem::take(&mut logs.seen);
    logs.new_lines(now, |line| {
        if !line.contains("\"usage\"") {
            return;
        }
        let Ok(v) = serde_json::from_str::<serde_json::Value>(line) else {
            return;
        };
        let u = &v["message"]["usage"];
        if !u.is_object() {
            return;
        }
        let key = format!(
            "{}:{}",
            v["message"]["id"].as_str().unwrap_or(""),
            v["requestId"].as_str().unwrap_or("")
        );
        let Some(ts) = v["timestamp"].as_str().and_then(parse_time) else {
            return;
        };
        if key != ":" && seen.insert(key, ts).is_some() {
            return;
        }
        let n = |k: &str| u[k].as_u64().unwrap_or(0);
        let (i, o, cw, cr) = (
            n("input_tokens"),
            n("output_tokens"),
            n("cache_creation_input_tokens"),
            n("cache_read_input_tokens"),
        );
        fresh.push((ts, i + o + cw + cr, i, o, cw, cr));
    });
    // Keys for what's past the week go with it (a day's slack for late lines).
    seen.retain(|_, t| *t >= now - 8 * 86400);
    logs.seen = seen;
    logs.entries.extend(fresh);
    let today = now - (now + local_offset(now)).rem_euclid(86400);
    let day: Vec<_> = logs.entries.iter().filter(|e| e.0 >= today).collect();
    let all = blocks(logs.entries.iter().map(|e| (e.0, e.1)).collect());
    let current = all.last().copied().filter(|(start, _)| now < start + BLOCK);
    let real = tapped(tap, now);
    if day.is_empty() && current.is_none() && real.is_none() {
        return None;
    }
    let sum = |f: fn(&&Entry) -> u64| day.iter().map(f).sum::<u64>();
    let total = sum(|e| e.1);
    let block_tokens = current.map_or(0, |b| b.1);
    let block_left = current.map(|(start, _)| span(start + BLOCK - now));
    let mut fields = BTreeMap::new();
    fields.insert("tokens_today".to_string(), count(total));
    fields.insert("messages_today".to_string(), day.len().to_string());
    fields.insert("block_tokens".to_string(), count(block_tokens));
    if let Some(l) = &block_left {
        fields.insert("block_left".into(), l.clone());
    }
    let mut detail = Vec::new();
    // What's left, and of what: Claude Code's windows (the tightest counts), or an estimate.
    let left: Option<(f64, String)> = match &real {
        Some((at, windows)) => {
            for (name, left, resets) in windows {
                let tail = resets
                    .filter(|r| *r > now)
                    .map(|r| format!(" · resets in {}", span(r - now)))
                    .unwrap_or_default();
                detail.push(bar_line(&format!("{name} limit"), *left, &tail));
            }
            detail.push((
                format!("{:13} from Claude Code's status line, {} ago", "", span(now - at)),
                false,
            ));
            windows
                .iter()
                .min_by(|a, b| a.1.total_cmp(&b.1))
                .map(|(name, left, _)| (*left, format!(" of {name}")))
        }
        None => {
            let past = all.iter().filter(|b| Some(**b) != current);
            let budget = match limit {
                Some(Limit::Tokens(n)) => Some((*n, "set in the config")),
                Some(Limit::Named(_)) => past
                    .map(|b| b.1)
                    .max()
                    .filter(|&n| n > 0)
                    .map(|n| (n, "your biggest block this week")),
                None => None,
            };
            let resets = block_left
                .as_ref()
                .map(|l| format!(" · resets in {l}"))
                .unwrap_or_default();
            match budget {
                Some((b, from)) => {
                    let l = 100.0 * (1.0 - block_tokens as f64 / b as f64).max(0.0);
                    detail.push(bar_line("5-hour block", l, &format!("{resets} (estimate)")));
                    detail.push((
                        format!(
                            "{:13} {} of {} tokens (limit: {from})",
                            "",
                            count(block_tokens),
                            count(b)
                        ),
                        false,
                    ));
                    fields.insert("limit".into(), count(b));
                    Some((l, String::new()))
                }
                None => {
                    detail.push((
                        format!("{:<13} {} tokens{resets}", "5-hour block", count(block_tokens)),
                        false,
                    ));
                    detail.push((
                        format!(
                            "{:13} for Claude Code's own limits: `corgi claude-statusline -- …` as its status line",
                            ""
                        ),
                        false,
                    ));
                    None
                }
            }
        }
    };
    if let Some((l, of)) = &left {
        fields.insert("left".into(), format!("{l:.0}%"));
        fields.insert("used".into(), format!("{:.0}%", 100.0 - l));
        fields.insert("window".into(), of.trim_start_matches(" of ").to_string());
    }
    detail.push((
        format!("{:<13} {} tokens over {} messages", "today", count(total), day.len()),
        false,
    ));
    detail.push((
        format!(
            "{:13} {} in · {} out · {} cache written · {} cache read",
            "",
            count(sum(|e| e.2)),
            count(sum(|e| e.3)),
            count(sum(|e| e.4)),
            count(sum(|e| e.5))
        ),
        false,
    ));
    let (full, short, icon) = match (show, &left) {
        (Show::Tokens, _) | (_, None) => (
            format!(
                "claude {} today{}",
                count(total),
                block_left.as_ref().map(|l| format!(" · {l} left")).unwrap_or_default()
            ),
            format!("claude {}", count(total)),
            format!("✻{}", count(total)),
        ),
        (_, Some((l, of))) => {
            let (f, s, i) = shares("claude", show, *l, of, "");
            (f, s, format!("✻{i}"))
        }
    };
    Some(Reading {
        fields,
        full,
        short,
        icon,
        detail,
    })
}

/// Codex: tokens today, and how much of its rate-limit windows is left (as Codex reports them).
fn codex(logs: &mut Logs, now: i64, show: Show) -> Option<Reading> {
    let mut fresh = Vec::new();
    let mut limits = None;
    logs.new_lines(now, |line| {
        if !line.contains("\"token_count\"") {
            return;
        }
        let Ok(v) = serde_json::from_str::<serde_json::Value>(line) else {
            return;
        };
        let p = &v["payload"];
        if p["type"] != "token_count" {
            return;
        }
        let Some(ts) = v["timestamp"].as_str().and_then(parse_time) else {
            return;
        };
        if let Some(n) = p["info"]["last_token_usage"]["total_tokens"].as_u64() {
            fresh.push((ts, n, 0, 0, 0, 0));
        }
        if p["rate_limits"].is_object() && limits.as_ref().is_none_or(|(t, _)| ts >= *t) {
            limits = Some((ts, p["rate_limits"].clone()));
        }
    });
    logs.entries.extend(fresh);
    if let Some(l) = limits
        && logs.limits.as_ref().is_none_or(|(t, _)| l.0 >= *t)
    {
        logs.limits = Some(l);
    }
    let today = now - (now + local_offset(now)).rem_euclid(86400);
    let day: Vec<_> = logs.entries.iter().filter(|e| e.0 >= today).collect();
    let total: u64 = day.iter().map(|e| e.1).sum();
    let mut fields = BTreeMap::new();
    fields.insert("tokens_today".to_string(), count(total));
    let mut detail = Vec::new();
    // Each window: its name, the share left, when it resets. A window past its reset is fresh.
    let mut windows: Vec<Window> = Vec::new();
    if let Some((_, l)) = &logs.limits {
        for which in ["primary", "secondary"] {
            let w = &l[which];
            let Some(used) = w["used_percent"].as_f64() else {
                continue;
            };
            let resets = w["resets_at"].as_i64();
            let used = if resets.is_some_and(|r| r <= now) { 0.0 } else { used };
            let name = window_name(w["window_minutes"].as_i64().unwrap_or(0));
            fields.insert(format!("{which}_used"), format!("{used:.0}%"));
            fields.insert(format!("{which}_left"), format!("{:.0}%", 100.0 - used));
            windows.push((name, 100.0 - used, resets));
        }
    }
    for (name, left, resets) in &windows {
        let tail = resets
            .filter(|r| *r > now)
            .map(|r| format!(" · resets in {}", span(r - now)))
            .unwrap_or_default();
        detail.push(bar_line(&format!("{name} limit"), *left, &tail));
    }
    detail.push((
        format!("{:<13} {} tokens over {} turns", "today", count(total), day.len()),
        false,
    ));
    // The tightest window is the one that matters.
    let tight = windows.iter().min_by(|a, b| a.1.total_cmp(&b.1));
    if let Some((name, left, _)) = tight {
        fields.insert("left".into(), format!("{left:.0}%"));
        fields.insert("used".into(), format!("{:.0}%", 100.0 - left));
        fields.insert("window".into(), name.clone());
    }
    if day.is_empty() && tight.is_none() {
        return None;
    }
    let (full, short) = match (show, tight) {
        (Show::Tokens, _) | (_, None) => (
            match tight {
                Some((name, left, _)) => format!("codex {} today · {left:.0}% of {name} left", count(total)),
                None => format!("codex {} today", count(total)),
            },
            format!("codex {}", count(total)),
        ),
        (_, Some((name, left, _))) => {
            let (f, s, _) = shares("codex", show, *left, &format!(" of {name}"), "");
            (f, s)
        }
    };
    Some(Reading {
        fields,
        full,
        short,
        icon: String::new(),
        detail,
    })
}

fn window_name(minutes: i64) -> String {
    match minutes {
        10080 => "week".into(),
        m if m >= 1440 => format!("{}d", m / 1440),
        m if m >= 60 => format!("{}h", m / 60),
        m => format!("{m}m"),
    }
}

/// The first line of what a shell line prints, if it succeeds in time (its first 64 KiB are
/// enough for that).
fn command(line: &str, timeout: Duration) -> Option<Reading> {
    let o = crate::process::run("sh", &["-c", line], timeout, 64 * 1024).ok()?;
    if !o.ok {
        return None;
    }
    command_reading(&String::from_utf8_lossy(&o.bytes))
}

/// A command's reading: its first line, the first dozen as details.
fn command_reading(out: &str) -> Option<Reading> {
    let first = crate::toast::clean(out.lines().find(|l| !l.trim().is_empty())?);
    let short: String = first.chars().take(16).collect();
    let short = if short.chars().count() < first.chars().count() {
        format!("{short}…")
    } else {
        String::new()
    };
    Some(Reading {
        fields: [("output".to_string(), first.clone())].into(),
        detail: out.lines().take(12).map(|l| (crate::toast::clean(l), false)).collect(),
        full: first,
        short,
        icon: String::new(),
    })
}

/// Seconds east of UTC here, at `now`.
#[cfg(unix)]
pub fn local_offset(now: i64) -> i64 {
    // SAFETY: localtime_r writes only into the tm we give it.
    unsafe {
        let t = now as libc::time_t;
        let mut tm: libc::tm = std::mem::zeroed();
        if libc::localtime_r(&t, &mut tm).is_null() {
            return 0;
        }
        tm.tm_gmtoff
    }
}

#[cfg(not(unix))]
pub fn local_offset(_now: i64) -> i64 {
    0
}

#[cfg(unix)]
pub fn hostname() -> String {
    let mut buf = [0u8; 256];
    // SAFETY: gethostname writes at most buf.len() bytes into buf.
    if unsafe { libc::gethostname(buf.as_mut_ptr() as *mut libc::c_char, buf.len()) } != 0 {
        return String::new();
    }
    let end = buf.iter().position(|&b| b == 0).unwrap_or(buf.len());
    String::from_utf8_lossy(&buf[..end]).into_owned()
}

#[cfg(not(unix))]
pub fn hostname() -> String {
    std::env::var("COMPUTERNAME").unwrap_or_default()
}

#[cfg(unix)]
fn load() -> Option<[f64; 3]> {
    let mut l = [0f64; 3];
    // SAFETY: getloadavg writes at most 3 doubles into l.
    (unsafe { libc::getloadavg(l.as_mut_ptr(), 3) } == 3).then_some(l)
}

#[cfg(not(unix))]
fn load() -> Option<[f64; 3]> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn scratch(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("corgi-usage-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn iso(t: i64) -> String {
        // Enough for tests: a UTC time from seconds.
        let days = t.div_euclid(86400);
        let (z, secs) = (days + 719468, t.rem_euclid(86400));
        let era = z.div_euclid(146097);
        let doe = z - era * 146097;
        let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
        let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
        let mp = (5 * doy + 2) / 153;
        let d = doy - (153 * mp + 2) / 5 + 1;
        let m = if mp < 10 { mp + 3 } else { mp - 9 };
        let y = yoe + era * 400 + i64::from(m <= 2);
        format!(
            "{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}.000Z",
            secs / 3600,
            secs % 3600 / 60,
            secs % 60
        )
    }

    fn seg(provider: &str) -> Segment {
        Segment {
            provider: provider.into(),
            ..Segment::default()
        }
    }

    fn claude_line(id: &str, ts: i64, out: u64) -> String {
        format!(
            "{{\"type\":\"assistant\",\"requestId\":\"r{id}\",\"timestamp\":\"{}\",\"message\":{{\"id\":\"m{id}\",\"usage\":{{\"input_tokens\":10,\"output_tokens\":{out},\"cache_creation_input_tokens\":100,\"cache_read_input_tokens\":1000}}}}}}\n",
            iso(ts)
        )
    }

    fn now() -> i64 {
        SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64
    }

    #[test]
    fn claude_counts_today_once_per_message_and_reads_on() {
        let root = scratch("claude");
        let dir = root.join("-proj");
        std::fs::create_dir_all(&dir).unwrap();
        let t = now();
        let path = dir.join("s.jsonl");
        // Two parts of one message (counted once), another message, and one 3 days ago.
        let body = claude_line("1", t - 60, 5)
            + &claude_line("1", t - 60, 5)
            + &claude_line("2", t - 30, 40)
            + &claude_line("3", t - 3 * 86400, 7);
        std::fs::write(&path, body).unwrap();
        let mut logs = Logs::new(root.clone());
        let r = claude(&mut logs, t, Show::Tokens, None, Path::new("/no/tap")).expect("usage today");
        assert_eq!(r.fields["messages_today"], "2");
        assert_eq!(r.fields["tokens_today"], count(2 * 1110 + 5 + 40));
        assert!(r.full.starts_with("claude ") && r.full.contains("today"), "{}", r.full);
        assert!(r.detail.iter().any(|(d, _)| d.contains("5-hour block")));
        // More written: only that is read.
        let mut f = std::fs::OpenOptions::new().append(true).open(&path).unwrap();
        f.write_all(claude_line("4", t - 10, 1).as_bytes()).unwrap();
        f.write_all(b"{\"partial").unwrap();
        let r = claude(&mut logs, t, Show::Tokens, None, Path::new("/no/tap")).unwrap();
        assert_eq!(r.fields["messages_today"], "3", "the half-written line waits");
        std::fs::remove_dir_all(&root).unwrap();
        // Nothing at all: nothing to show.
        assert!(
            claude(
                &mut Logs::new(scratch("none")),
                t,
                Show::Left,
                None,
                Path::new("/no/tap")
            )
            .is_none()
        );
    }

    #[test]
    fn claude_percent_left_from_claude_code_or_an_estimate() {
        let root = scratch("claude-left");
        std::fs::create_dir_all(root.join("-p")).unwrap();
        let t = now();
        // This block: 1,115 + 1,110 = 2,225 tokens.
        std::fs::write(
            root.join("-p/s.jsonl"),
            claude_line("a", t - 120, 5) + &claude_line("b", t - 60, 0),
        )
        .unwrap();
        let none = Path::new("/no/tap");
        let read = |show, limit: Option<&Limit>, tap: &Path| {
            claude(&mut Logs::new(root.clone()), t, show, limit, tap).unwrap()
        };

        // Claude Code's own numbers, saved from its status line: the tightest window counts.
        let tap = root.join("claude-usage.json");
        let saved = format!(
            r#"{{"at":{},"five_hour":{{"used_percentage":48,"resets_at":{}}},"seven_day":{{"used_percentage":91,"resets_at":{}}}}}"#,
            t - 30,
            t + 2 * 3600,
            t + 3 * 86400
        );
        std::fs::write(&tap, &saved).unwrap();
        let r = read(Show::Left, None, &tap);
        assert_eq!(r.full, "claude 9% left of week");
        assert_eq!(r.short, "claude 9%");
        assert_eq!(r.fields["window"], "week");
        assert!(
            r.detail[0].0.starts_with("5h limit") && r.detail[0].0.contains("52% left · resets in 2h00m"),
            "{:?}",
            r.detail
        );
        assert!(
            r.detail[1].0.contains("9% left") && r.detail[1].1,
            "a warning: {:?}",
            r.detail[1]
        );
        assert!(
            r.detail[2].0.contains("from Claude Code's status line, 30s ago"),
            "{:?}",
            r.detail
        );
        assert_eq!(read(Show::Used, None, &tap).full, "claude 91% used of week");
        // They win over an estimate.
        assert_eq!(
            read(Show::Left, Some(&Limit::Tokens(10_000)), &tap).full,
            "claude 9% left of week"
        );
        // A window past its reset is fresh.
        std::fs::write(&tap, saved.replace(&(t + 3 * 86400).to_string(), &(t - 1).to_string())).unwrap();
        assert_eq!(read(Show::Left, None, &tap).full, "claude 52% left of 5h");

        // Without them, an estimate only if asked: of a budget…
        let r = read(Show::Left, Some(&Limit::Tokens(10_000)), none);
        assert_eq!(r.fields["left"], "78%");
        assert!(r.full.starts_with("claude 78% left"), "{}", r.full);
        assert!(
            r.detail[0].0.contains("(estimate)")
                && r.detail[1].0.contains("2.2k of 10k tokens (limit: set in the config)"),
            "{:?}",
            r.detail
        );
        // …or of the biggest finished block this week (here 4,440 tokens, two days ago).
        let old = (0..4)
            .map(|k| claude_line(&format!("o{k}"), t - 2 * 86400 + k, 0))
            .collect::<String>();
        std::fs::write(root.join("-p/old.jsonl"), old).unwrap();
        let r = read(Show::Left, Some(&Limit::Named("max".into())), none);
        assert_eq!((r.fields["limit"].as_str(), r.fields["left"].as_str()), ("4.4k", "50%"));
        // …else tokens, and the details say how to get the real numbers.
        let r = read(Show::Left, None, none);
        assert!(r.full.contains("today") && !r.fields.contains_key("left"), "{}", r.full);
        assert!(
            r.detail.iter().any(|(d, _)| d.contains("corgi claude-statusline")),
            "{:?}",
            r.detail
        );
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn codex_tokens_and_limits() {
        let root = scratch("codex");
        let dir = root.join("2026/11/10");
        std::fs::create_dir_all(&dir).unwrap();
        let t = now();
        let ev = |ts: i64, n: u64, primary: f64, secondary: f64, reset: i64| {
            format!(
                "{{\"timestamp\":\"{}\",\"type\":\"event_msg\",\"payload\":{{\"type\":\"token_count\",\"info\":{{\"last_token_usage\":{{\"total_tokens\":{n}}}}},\"rate_limits\":{{\"primary\":{{\"used_percent\":{primary},\"window_minutes\":300,\"resets_at\":{reset}}},\"secondary\":{{\"used_percent\":{secondary},\"window_minutes\":10080,\"resets_at\":{}}}}}}}}}\n",
                iso(ts),
                t + 3 * 86400
            )
        };
        let path = dir.join("rollout.jsonl");
        std::fs::write(
            &path,
            ev(t - 50, 1000, 30.0, 10.0, t + 3600) + &ev(t - 10, 500, 85.0, 12.0, t + 3600),
        )
        .unwrap();
        let r = codex(&mut Logs::new(root.clone()), t, Show::Left).unwrap();
        assert_eq!(r.fields["tokens_today"], "1.5k");
        // The tightest window: the 5-hour one, 15% left.
        assert_eq!(r.full, "codex 15% left of 5h");
        assert_eq!(r.short, "codex 15%");
        assert!(
            r.detail[0].0.starts_with("5h limit") && r.detail[0].0.contains("15% left · resets in 1h00m"),
            "{:?}",
            r.detail
        );
        assert!(r.detail[0].1, "little left: a warning");
        assert!(r.detail[1].0.contains("88% left") && !r.detail[1].1, "{:?}", r.detail);
        assert_eq!(
            codex(&mut Logs::new(root.clone()), t, Show::Used).unwrap().full,
            "codex 85% used of 5h"
        );
        assert!(
            codex(&mut Logs::new(root.clone()), t, Show::Tokens)
                .unwrap()
                .full
                .starts_with("codex 1.5k today")
        );
        // Past its reset, a window is fresh again.
        std::fs::write(&path, ev(t - 50, 1000, 85.0, 12.0, t - 5)).unwrap();
        assert_eq!(
            codex(&mut Logs::new(root.clone()), t, Show::Left).unwrap().full,
            "codex 88% left of week"
        );
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn commands_clock_host() {
        let r = command(
            "printf 'first line of it, a long one\\nsecond\\n'",
            Duration::from_secs(5),
        )
        .unwrap();
        assert_eq!(r.full, "first line of it, a long one");
        assert_eq!(r.short, "first line of it…");
        assert!(command("exit 3", Duration::from_secs(5)).is_none(), "failed: nothing");
        assert!(
            command("sleep 5", Duration::from_millis(100)).is_none(),
            "too slow: nothing"
        );
        let clock = Source::Clock.read(SystemTime::now()).unwrap();
        assert_eq!(clock.full.len(), 5);
        assert!(!Source::Host.read(SystemTime::now()).unwrap().full.is_empty());
        assert!(Source::new(&seg("nope")).is_none());
        assert!(Source::new(&seg("command")).is_none(), "needs cmd");
    }

    #[test]
    fn readings_from_another_machine() {
        let sys = Source::System;
        assert!(sys.remote_line().unwrap().contains("loadavg"));
        assert_eq!(
            sys.remote_reading("0.52 0.48 0.50 1/234 5678\n").unwrap().full,
            "load 0.52"
        );
        assert_eq!(
            sys.remote_reading("{ 1.25 1.10 0.98 }\n").unwrap().full,
            "load 1.25",
            "macOS sysctl"
        );
        assert!(sys.remote_reading("nope").is_none());
        let host = Source::Host;
        assert_eq!(host.remote_reading("box.example.org\n").unwrap().full, "box");
        let cmd = Source::Command {
            line: "echo hi".into(),
            timeout: Duration::from_secs(1),
        };
        assert_eq!(cmd.remote_line().as_deref(), Some("echo hi"));
        assert_eq!(cmd.remote_reading("\nhi there\n").unwrap().full, "hi there");
        assert!(Source::Clock.remote_line().is_none());
    }

    #[test]
    fn logs_are_looked_for_now_and_then_and_read_in_chunks() {
        let root = scratch("chunks");
        let dir = root.join("p");
        std::fs::create_dir_all(&dir).unwrap();
        let t = now();
        std::fs::write(dir.join("a.jsonl"), claude_line("1", t - 60, 5)).unwrap();
        let mut logs = Logs::new(root.clone());
        let r = claude(&mut logs, t, Show::Tokens, None, Path::new("/no/tap")).unwrap();
        assert_eq!(r.fields["messages_today"], "1");
        // A new file: found at the next look round (a minute on), not on every poll.
        std::fs::write(dir.join("b.jsonl"), claude_line("2", t - 30, 5)).unwrap();
        let r = claude(&mut logs, t + 1, Show::Tokens, None, Path::new("/no/tap")).unwrap();
        assert_eq!(r.fields["messages_today"], "1");
        let r = claude(&mut logs, t + REDISCOVER, Show::Tokens, None, Path::new("/no/tap")).unwrap();
        assert_eq!(r.fields["messages_today"], "2");
        // A file that went is forgotten at the next look round.
        std::fs::remove_file(dir.join("a.jsonl")).unwrap();
        claude(&mut logs, t + 2 * REDISCOVER, Show::Tokens, None, Path::new("/no/tap"));
        assert_eq!(logs.offsets.len(), 1);
        // More than a chunk: read a chunk at a time, nothing lost.
        let one = claude_line("x", t - 20, 1);
        let n = (CHUNK as usize / one.len()) + 50;
        let body: String = (0..n).map(|k| claude_line(&format!("m{k}"), t - 20, 1)).collect();
        std::fs::write(dir.join("c.jsonl"), body).unwrap();
        let mut logs = Logs::new(root.clone());
        let first = claude(&mut logs, t, Show::Tokens, None, Path::new("/no/tap")).unwrap();
        let first: usize = first.fields["messages_today"].parse().unwrap();
        assert!(first < n + 1, "one chunk first: {first}");
        let r = claude(&mut logs, t, Show::Tokens, None, Path::new("/no/tap")).unwrap();
        assert_eq!(r.fields["messages_today"], (n + 1).to_string(), "the rest next time");
        // Keys for what's past the week go with it.
        assert!(logs.seen.len() == n + 1);
        claude(&mut logs, t + 9 * 86400, Show::Tokens, None, Path::new("/no/tap"));
        assert!(logs.seen.is_empty());
        std::fs::remove_dir_all(&root).unwrap();
    }
}
