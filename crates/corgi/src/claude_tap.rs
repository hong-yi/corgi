//! `corgi claude-statusline -- <your status line command>`: Claude Code tells its status line
//! command how much of its rate limits is used (`rate_limits.five_hour` and `.seven_day`,
//! each `used_percentage` and `resets_at`, on stdin) and keeps it nowhere else. Put this in
//! front of your status line, and corgi saves that (to `claude-usage.json` in its state
//! folder) for the `claude` usage segment, then runs your command with the same input, so
//! the status line stays as it was.

use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

/// Where the latest rate limits are kept.
pub fn path() -> PathBuf {
    crate::paths::state_dir().join("claude-usage.json")
}

/// Save what `input` (Claude Code's status line JSON) says about rate limits, if anything.
pub fn save(input: &[u8], to: &Path, now: i64) -> bool {
    let Ok(v) = serde_json::from_slice::<serde_json::Value>(input) else {
        return false;
    };
    let limits = &v["rate_limits"];
    if !limits.is_object() {
        return false;
    }
    let keep = serde_json::json!({
        "at": now,
        "five_hour": limits["five_hour"],
        "seven_day": limits["seven_day"],
    });
    if let Some(dir) = to.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    // Whole or not at all: write beside it, then rename over it.
    let tmp = to.with_extension(format!("json.{}", std::process::id()));
    std::fs::write(&tmp, keep.to_string()).is_ok() && std::fs::rename(&tmp, to).is_ok()
}

/// Read stdin, save the rate limits, run `cmd` (one word: by the shell) with the same input.
/// Its exit code is ours.
pub fn run(cmd: &[String]) -> i32 {
    let mut input = Vec::new();
    let _ = std::io::stdin().read_to_end(&mut input);
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as i64);
    save(&input, &path(), now);
    let Some(first) = cmd.first() else { return 0 };
    let mut c = if cmd.len() == 1 {
        let mut c = Command::new("sh");
        c.arg("-c").arg(first);
        c
    } else {
        let mut c = Command::new(first);
        c.args(&cmd[1..]);
        c
    };
    let Ok(mut child) = c.stdin(Stdio::piped()).spawn() else {
        eprintln!("corgi claude-statusline: can't run {first}");
        return 127;
    };
    if let Some(mut stdin) = child.stdin.take() {
        let _ = stdin.write_all(&input);
    }
    child.wait().ok().and_then(|s| s.code()).unwrap_or(1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_only_the_rate_limits() {
        let dir = std::env::temp_dir().join(format!("corgi-tap-{}", std::process::id()));
        let to = dir.join("claude-usage.json");
        let input = br#"{"model":{"id":"x"},"rate_limits":{"five_hour":{"used_percentage":42.5,"resets_at":1800000000},"seven_day":{"used_percentage":7,"resets_at":1800500000}},"cwd":"/x"}"#;
        assert!(save(input, &to, 123));
        let v: serde_json::Value = serde_json::from_slice(&std::fs::read(&to).unwrap()).unwrap();
        assert_eq!(v["at"], 123);
        assert_eq!(v["five_hour"]["used_percentage"], 42.5);
        assert_eq!(v["seven_day"]["resets_at"], 1_800_500_000);
        assert!(v.get("model").is_none(), "nothing else");
        // No rate limits (an API key, an older Claude Code): the last ones stay.
        assert!(!save(br#"{"model":{}}"#, &to, 456));
        assert!(!save(b"not json", &to, 456));
        assert_eq!(
            serde_json::from_slice::<serde_json::Value>(&std::fs::read(&to).unwrap()).unwrap()["at"],
            123
        );
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
