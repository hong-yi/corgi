//! Git worktrees for new workspaces: the branch's folder name, a fresh branch name, and the
//! shell line that makes one. The line runs through core's `machine.exec`, so it works the
//! same on this machine and on others; it prints the worktree's absolute path. Pure.
//!
//! Layout as in herdr: `<worktrees.directory>/<repo>/<branch as a folder name>`.

/// A branch as a folder name: lowercase letters and digits, runs of anything else one `-`.
pub fn slug(branch: &str) -> String {
    let mut out = String::new();
    for c in branch.chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c.to_ascii_lowercase());
        } else if !out.ends_with('-') {
            out.push('-');
        }
    }
    let out = out.trim_matches('-');
    if out.is_empty() { "worktree".into() } else { out.into() }
}

/// A new branch name, `worktree/brave-river-1a2b`, from `seed` (the clock will do).
pub fn new_branch(seed: u64) -> String {
    const ADJECTIVES: &[&str] = &[
        "brave", "calm", "eager", "fair", "gentle", "keen", "lucky", "merry", "quiet", "swift", "tidy", "wise",
    ];
    const NOUNS: &[&str] = &[
        "river", "meadow", "harbor", "cedar", "comet", "falcon", "lantern", "maple", "pebble", "summit", "willow",
        "otter",
    ];
    let a = ADJECTIVES[(seed % ADJECTIVES.len() as u64) as usize];
    let n = NOUNS[((seed / ADJECTIVES.len() as u64) % NOUNS.len() as u64) as usize];
    format!("worktree/{a}-{n}-{:04x}", (seed >> 8) & 0xffff)
}

/// `s` for `sh`, quoted; a leading `~/` stays the home directory.
pub fn quote(s: &str) -> String {
    let q = |s: &str| format!("'{}'", s.replace('\'', "'\\''"));
    match s.strip_prefix("~/") {
        Some(rest) => format!("\"$HOME\"/{}", q(rest)),
        None if s == "~" => "\"$HOME\"".into(),
        None => q(s),
    }
}

/// The shell line that makes the worktree and prints its path: in the repository `folder`
/// is in, on `branch` (made from HEAD if it doesn't exist yet), at `path` (default: under
/// `dir`, by repository and branch: the repository's own checkout's name, even from one of
/// its worktrees).
pub fn script(folder: &str, branch: &str, path: Option<&str>, dir: &str) -> String {
    let path = match path {
        Some(p) => quote(p),
        None => format!(
            "{}/\"$name\"/{}",
            quote(dir.trim_end_matches('/')),
            quote(&slug(branch))
        ),
    };
    let b = quote(branch);
    format!(
        "set -e; root=$(git -C {f} rev-parse --show-toplevel); \
         repo=$(git -C {f} rev-parse --path-format=absolute --git-common-dir); \
         name=$(basename \"${{repo%/.git}}\"); p={path}; \
         if git -C \"$root\" show-ref --verify --quiet refs/heads/{b}; \
         then git -C \"$root\" worktree add --quiet \"$p\" {b}; \
         else git -C \"$root\" worktree add --quiet -b {b} \"$p\"; fi; \
         cd \"$p\" && pwd -P",
        f = quote(folder),
        b = b,
    )
}

/// Git facts about folders, as one shell line (for another machine, through `machine.exec`):
/// per folder a line `folder<TAB>branch<TAB>checkout<TAB>common dir`, empty where git says
/// nothing.
pub fn facts_script(folders: &[String]) -> String {
    let list: Vec<String> = folders.iter().map(|f| quote(f)).collect();
    format!(
        "for f in {}; do \
         b=$(git -C \"$f\" symbolic-ref --short -q HEAD 2>/dev/null); \
         t=$(git -C \"$f\" rev-parse --show-toplevel 2>/dev/null); \
         c=$(git -C \"$f\" rev-parse --path-format=absolute --git-common-dir 2>/dev/null); \
         printf '%s\\t%s\\t%s\\t%s\\n' \"$f\" \"$b\" \"$t\" \"$c\"; done",
        list.join(" ")
    )
}

/// A folder's git facts: (folder, branch, checkout, repository's own checkout).
pub type Facts = (String, Option<String>, Option<String>, Option<String>);

/// What `facts_script` printed.
pub fn parse_facts(out: &str) -> Vec<Facts> {
    let some = |s: &str| Some(s.trim().to_string()).filter(|s| !s.is_empty());
    out.lines()
        .filter_map(|l| {
            let mut p = l.split('\t');
            let folder = p.next()?.to_string();
            let (b, t, c) = (p.next()?, p.next()?, p.next().unwrap_or(""));
            Some((folder, some(b), some(t), some(c).map(|c| repo_of(&c))))
        })
        .collect()
}

/// The repository's own checkout, from its common git dir (`/src/r/.git` → `/src/r`).
pub fn repo_of(common: &str) -> String {
    let p = std::path::Path::new(common);
    let repo = if p.file_name().is_some_and(|n| n == ".git") {
        p.parent().unwrap_or(p)
    } else {
        p
    };
    repo.display().to_string()
}

/// Standard base64 (what `machine.exec` returns output in). `None` if it isn't.
pub fn unbase64(s: &str) -> Option<Vec<u8>> {
    let val = |c: u8| match c {
        b'A'..=b'Z' => Some(c - b'A'),
        b'a'..=b'z' => Some(c - b'a' + 26),
        b'0'..=b'9' => Some(c - b'0' + 52),
        b'+' => Some(62),
        b'/' => Some(63),
        _ => None,
    };
    // Whitespace and padding are skipped; four characters make three bytes, a last two or
    // three make one or two, a last one alone is an error.
    let mut out = Vec::with_capacity(s.len() / 4 * 3 + 3);
    let (mut n, mut k) = (0u32, 0u32);
    for c in s.bytes() {
        if c.is_ascii_whitespace() || c == b'=' {
            continue;
        }
        n = n << 6 | val(c)? as u32;
        k += 1;
        if k == 4 {
            out.extend_from_slice(&[(n >> 16) as u8, (n >> 8) as u8, n as u8]);
            (n, k) = (0, 0);
        }
    }
    match k {
        0 => {}
        2 => out.push((n >> 4) as u8),
        3 => out.extend_from_slice(&[(n >> 10) as u8, (n >> 2) as u8]),
        _ => return None,
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;

    #[test]
    fn slugs_and_branches() {
        assert_eq!(slug("worktree/brave-river"), "worktree-brave-river");
        assert_eq!(slug("issue/137 Worktree Spaces"), "issue-137-worktree-spaces");
        assert_eq!(slug("///"), "worktree");
        let b = new_branch(12345);
        assert!(b.starts_with("worktree/") && b.matches('-').count() == 2, "{b}");
        assert_ne!(new_branch(1), new_branch(2));
    }

    #[test]
    fn quoting() {
        assert_eq!(quote("a b"), "'a b'");
        assert_eq!(quote("it's"), "'it'\\''s'");
        assert_eq!(quote("~/x y"), "\"$HOME\"/'x y'");
        assert_eq!(quote("~"), "\"$HOME\"");
    }

    #[test]
    fn base64() {
        assert_eq!(unbase64("aGVsbG8=").unwrap(), b"hello");
        assert_eq!(unbase64("aGVsbG8gd29ybGQK").unwrap(), b"hello world\n");
        assert_eq!(unbase64("").unwrap(), b"");
        assert_eq!(unbase64("YQ==").unwrap(), b"a");
        assert!(unbase64("!!!!").is_none());
    }

    /// The real thing, with git, in a scratch repository.
    #[test]
    fn script_makes_worktrees() {
        if Command::new("git").arg("--version").output().is_err() {
            return;
        }
        let root = std::env::temp_dir().join(format!("corgi-wt-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let repo = root.join("my repo");
        std::fs::create_dir_all(repo.join("sub")).unwrap();
        let git = |args: &[&str]| {
            let ok = Command::new("git").current_dir(&repo).args(args).output().unwrap();
            assert!(ok.status.success(), "{args:?}: {}", String::from_utf8_lossy(&ok.stderr));
        };
        git(&["init", "-q"]);
        git(&[
            "-c",
            "user.email=h@example.invalid",
            "-c",
            "user.name=h",
            "commit",
            "-q",
            "--allow-empty",
            "-m",
            "x",
        ]);
        let dir = root.join("trees").display().to_string();
        let run = |line: &str| Command::new("sh").arg("-c").arg(line).output().unwrap();

        // A new branch, from a folder inside the repo, in the default place.
        let out = run(&script(&repo.join("sub").display().to_string(), "feat/one", None, &dir));
        assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
        let path = String::from_utf8(out.stdout).unwrap().trim().to_string();
        assert!(path.ends_with("trees/my repo/feat-one"), "{path}");
        assert!(std::path::Path::new(&path).join(".git").exists());

        // From inside that worktree: filed under the repository's name, not the worktree's.
        let out = run(&script(&path, "feat/two", None, &dir));
        assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
        let two = String::from_utf8(out.stdout).unwrap().trim().to_string();
        assert!(two.ends_with("trees/my repo/feat-two"), "{two}");

        // An existing branch, at a path given.
        git(&["branch", "old"]);
        let at = root.join("elsewhere").display().to_string();
        let out = run(&script(&repo.display().to_string(), "old", Some(&at), &dir));
        assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
        assert!(String::from_utf8(out.stdout).unwrap().trim().ends_with("elsewhere"));

        // Not a repository: fails, saying why.
        let out = run(&script(&root.display().to_string(), "x", None, &dir));
        assert!(!out.status.success());
        assert!(String::from_utf8_lossy(&out.stderr).contains("not a git repository"));
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn git_facts_for_another_machine() {
        let out = "/src/r\tmain\t/src/r\t/src/r/.git\n/t/feat\tfeat\t/t/feat\t/src/r/.git\n/tmp\t\t\t\n";
        let f = parse_facts(out);
        assert_eq!(f.len(), 3);
        assert_eq!(
            f[0],
            (
                "/src/r".into(),
                Some("main".into()),
                Some("/src/r".into()),
                Some("/src/r".into())
            )
        );
        assert_eq!(f[1].3.as_deref(), Some("/src/r"), "a worktree's repository");
        assert_eq!(f[2], ("/tmp".into(), None, None, None));
        if std::process::Command::new("git").arg("--version").output().is_err() {
            return;
        }
        let dir = std::env::temp_dir().join(format!("corgi-facts-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let ok = std::process::Command::new("git")
            .args(["init", "-q", "-b", "trunk"])
            .current_dir(&dir)
            .status()
            .unwrap()
            .success();
        assert!(ok);
        let d = dir.canonicalize().unwrap().display().to_string();
        let out = std::process::Command::new("sh")
            .arg("-c")
            .arg(facts_script(&[d.clone(), "/nonexistent dir".into()]))
            .output()
            .unwrap();
        let f = parse_facts(&String::from_utf8_lossy(&out.stdout));
        assert_eq!(f[0], (d.clone(), Some("trunk".into()), Some(d.clone()), Some(d)));
        assert_eq!(f[1], ("/nonexistent dir".into(), None, None, None));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn base64_decodes_every_length() {
        let enc = crate::clipboard::base64;
        for n in 0..40usize {
            let data: Vec<u8> = (0..n).map(|i| (i * 37 + 11) as u8).collect();
            assert_eq!(unbase64(&enc(&data)).unwrap(), data, "{n} bytes");
        }
        assert_eq!(unbase64("Zm9v\nYmFy\n").unwrap(), b"foobar", "whitespace skipped");
        assert_eq!(unbase64("Zm8").unwrap(), b"fo", "padding optional");
        assert_eq!(unbase64("Z"), None, "one left over");
        assert_eq!(unbase64("Zm9v!"), None, "not base64");
        let big: Vec<u8> = (0..8 * 1024 * 1024).map(|i| (i % 251) as u8).collect();
        let t = std::time::Instant::now();
        assert_eq!(unbase64(&enc(&big)).unwrap(), big);
        assert!(t.elapsed() < std::time::Duration::from_secs(5), "{:?}", t.elapsed());
    }
}
