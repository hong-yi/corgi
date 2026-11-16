//! corgi in a real PTY against a real, isolated herdr-core. Unix only.
//! The screen is read back through corgi-vt, the same emulator the panes use.
//! Skipped (with a note) when `hc` isn't installed.
#![cfg(unix)]

use std::ffi::OsString;
use std::io::{Read, Write};
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
use std::os::unix::process::CommandExt;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

const COLS: u16 = 100;
const ROWS: u16 = 30;

fn hc_bin() -> OsString {
    std::env::var_os("HC_BIN").unwrap_or_else(|| "hc".into())
}

/// An isolated core: own session, state and config, short paths (unix socket limit).
struct Core {
    dir: PathBuf,
    id: String,
}

impl Core {
    fn start(id: &str) -> Option<Core> {
        if Command::new(hc_bin()).arg("--version").output().is_err() {
            eprintln!("skipping: hc not installed");
            return None;
        }
        let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("h").join(id);
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        // The panes' shell: /bin/sh with a plain `$ ` prompt, the same on every machine. Core starts
        // shells as login shells, which take the system's prompt (with the host name in it: long
        // on CI machines, where it wraps in narrow panes). Commands (`-c …`) pass through.
        let sh = dir.join("sh");
        std::fs::write(
            &sh,
            "#!/bin/sh\nPS1='$ '; export PS1\nfor a; do shift; case \"$a\" in -l|--login) ;; *) set -- \"$@\" \"$a\" ;; esac; done\nexec /bin/sh \"$@\"\n",
        )
        .unwrap();
        let mut perm = std::fs::metadata(&sh).unwrap().permissions();
        std::os::unix::fs::PermissionsExt::set_mode(&mut perm, 0o755);
        std::fs::set_permissions(&sh, perm).unwrap();
        Some(Core { dir, id: id.into() })
    }

    /// The panes' shell (see `start`).
    fn shell(&self) -> PathBuf {
        self.dir.join("sh")
    }

    fn env(&self) -> Vec<(String, OsString)> {
        vec![
            ("HC_SESSION".into(), format!("e{}", self.id).into()),
            ("XDG_STATE_HOME".into(), self.dir.join("s").into()),
            ("XDG_CONFIG_HOME".into(), self.dir.join("c").into()),
            ("XDG_RUNTIME_DIR".into(), self.dir.join("r").into()),
            // The tests' folders are inside corgi's own checkout: git mustn't see it from them
            // (repositories a test makes inside its folder still work).
            ("GIT_CEILING_DIRECTORIES".into(), self.dir.parent().unwrap().into()),
            // The usage segments read these tools' logs: this test's own, not yours.
            ("CLAUDE_CONFIG_DIR".into(), self.dir.join("claude").into()),
            ("CODEX_HOME".into(), self.dir.join("codex").into()),
        ]
    }

    /// Write corgi's config for this core's tests (XDG_CONFIG_HOME is per test).
    fn config(&self, toml: &str) {
        let dir = self.dir.join("c/corgi");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("config.toml"), toml).unwrap();
    }

    fn hc(&self, args: &[&str]) -> serde_json::Value {
        let out = Command::new(hc_bin())
            .args(args)
            .current_dir(&self.dir)
            .envs(self.env())
            // A core this starts gives its terminals this shell, as corgi's tests expect.
            .env("SHELL", self.shell())
            .output()
            .unwrap();
        serde_json::from_slice(&out.stdout).unwrap_or(serde_json::Value::Null)
    }
}

impl Drop for Core {
    fn drop(&mut self) {
        let status = self.hc(&["server", "status"]);
        self.hc(&["server", "stop"]);
        for h in status["holders"].as_array().into_iter().flatten() {
            if let Some(pid) = h["pid"].as_i64() {
                let _ = Command::new("kill").arg(pid.to_string()).output();
            }
        }
    }
}

/// corgi on a PTY, answering the host-terminal queries a real terminal would.
struct Corgi {
    child: Child,
    master: std::fs::File,
    out: Arc<Mutex<Vec<u8>>>,
    /// The terminal's size now, and where in `out` it took it (the screen is read from there).
    size: (u16, u16),
    mark: usize,
}

impl Corgi {
    fn spawn(core: &Core) -> Corgi {
        Corgi::spawn_env(core, &[])
    }

    /// With some variables of its own.
    fn spawn_env(core: &Core, env: &[(&str, &str)]) -> Corgi {
        Corgi::spawn_at(core, &core.dir, env)
    }

    /// Started in `dir` (its first workspace's folder).
    fn spawn_at(core: &Core, dir: &std::path::Path, env: &[(&str, &str)]) -> Corgi {
        Corgi::spawn_sized(core, dir, env, (COLS, ROWS))
    }

    /// On a terminal of this size (columns, rows).
    fn spawn_sized(core: &Core, dir: &std::path::Path, env: &[(&str, &str)], size: (u16, u16)) -> Corgi {
        let (mut m, mut s) = (0, 0);
        let mut ws = libc::winsize {
            ws_row: size.1,
            ws_col: size.0,
            ws_xpixel: 0,
            ws_ypixel: 0,
        };
        // SAFETY: out-pointers for two fds; winsize is valid.
        assert_eq!(
            unsafe { libc::openpty(&mut m, &mut s, std::ptr::null_mut(), std::ptr::null_mut(), &raw mut ws) },
            0
        );
        // SAFETY: openpty returned two fresh fds we now own.
        let (master, slave) = unsafe { (OwnedFd::from_raw_fd(m), OwnedFd::from_raw_fd(s)) };
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_corgi"));
        cmd.current_dir(dir)
            .envs(core.env())
            .envs(env.iter().map(|(k, v)| (k.to_string(), v.to_string())))
            .env("SHELL", core.shell())
            .env("CORGI_LOG", "debug")
            .stdin(Stdio::from(slave.try_clone().unwrap()))
            .stdout(Stdio::from(slave.try_clone().unwrap()))
            .stderr(Stdio::from(slave));
        // SAFETY: async-signal-safe calls only: new session, then take the PTY as controlling terminal.
        unsafe {
            cmd.pre_exec(|| {
                libc::setsid();
                libc::ioctl(0, libc::TIOCSCTTY as _, 0);
                Ok(())
            });
        }
        let child = cmd.spawn().unwrap();
        let master = std::fs::File::from(master);
        let out = Arc::new(Mutex::new(Vec::new()));
        let (mut r, mut w, sink) = (master.try_clone().unwrap(), master.try_clone().unwrap(), out.clone());
        std::thread::spawn(move || {
            let mut buf = [0u8; 65536];
            while let Ok(n) = r.read(&mut buf) {
                if n == 0 {
                    break;
                }
                let chunk = &buf[..n];
                if chunk.windows(5).any(|x| x == b"\x1b]10;") {
                    let _ =
                        w.write_all(b"\x1b]10;rgb:cdcd/d6d6/f4f4\x1b\\\x1b]11;rgb:1e1e/1e1e/2e2e\x1b\\\x1b[6;18;9t");
                }
                // Like Ghostty or kitty: the kitty keyboard protocol is supported (answered before DA1).
                if chunk.windows(4).any(|x| x == b"\x1b[?u") {
                    let _ = w.write_all(b"\x1b[?0u");
                }
                if chunk.windows(3).any(|x| x == b"\x1b[c") {
                    let _ = w.write_all(b"\x1b[?62;22c");
                }
                sink.lock().unwrap().extend_from_slice(chunk);
            }
        });
        Corgi {
            child,
            master,
            out,
            size,
            mark: 0,
        }
    }

    /// The terminal changes size (corgi gets SIGWINCH and draws it all again).
    fn resize(&mut self, cols: u16, rows: u16) {
        let ws = libc::winsize {
            ws_row: rows,
            ws_col: cols,
            ws_xpixel: 0,
            ws_ypixel: 0,
        };
        self.mark = self.out.lock().unwrap().len();
        self.size = (cols, rows);
        // SAFETY: the master fd is ours and open; winsize is valid.
        assert_eq!(
            unsafe { libc::ioctl(self.master.as_raw_fd(), libc::TIOCSWINSZ, &ws) },
            0
        );
    }

    fn typ(&mut self, s: &str) {
        self.master.write_all(s.as_bytes()).unwrap();
    }

    /// The screen as text, one line per row.
    fn screen(&self) -> Vec<String> {
        let mut vt = corgi_vt::Vt::new(self.size.0, self.size.1).unwrap();
        vt.write(&self.out.lock().unwrap()[self.mark..]);
        let f = vt.frame().unwrap();
        (0..f.rows)
            .map(|y| {
                (0..f.cols)
                    .filter(|&x| f.cell(x, y).wide != corgi_vt::Wide::Tail)
                    .map(|x| match f.cell(x, y).text.as_str() {
                        "" => " ".to_string(),
                        t => t.to_string(),
                    })
                    .collect::<String>()
                    .trim_end()
                    .to_string()
            })
            .collect()
    }

    /// A cell's background colour on corgi's screen.
    fn bg(&self, x: u16, y: u16) -> Option<corgi_vt::Rgb> {
        let mut vt = corgi_vt::Vt::new(self.size.0, self.size.1).unwrap();
        vt.write(&self.out.lock().unwrap()[self.mark..]);
        vt.frame().unwrap().cell(x, y).bg
    }

    fn wait_for(&self, what: &str, secs: u64) -> Vec<String> {
        let end = Instant::now() + Duration::from_secs(secs);
        loop {
            let s = self.screen();
            if s.iter().any(|l| l.contains(what)) {
                return s;
            }
            assert!(Instant::now() < end, "never saw {what:?} on screen:\n{}", s.join("\n"));
            std::thread::sleep(Duration::from_millis(50));
        }
    }

    fn wait_exit(&mut self, secs: u64) -> i32 {
        let end = Instant::now() + Duration::from_secs(secs);
        loop {
            if let Some(st) = self.child.try_wait().unwrap() {
                return st.code().unwrap_or(-1);
            }
            assert!(Instant::now() < end, "corgi didn't exit");
            std::thread::sleep(Duration::from_millis(50));
        }
    }
}

impl Drop for Corgi {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        let _ = self.master.as_raw_fd();
    }
}

/// The row with the window list (it has the "+" button), wherever the tab bar is.
fn tab_row(s: &[String]) -> Option<usize> {
    // The new-window button: mid-line, or last (lines come back trimmed).
    s.iter()
        .position(|l| (l.contains(" + ") || l.ends_with(" +")) && l.contains(" 0:"))
}

/// Each live terminal's program's process id.
fn pids(core: &Core) -> Vec<i64> {
    live(core)
        .iter()
        .filter_map(|a| core.hc(&["get", a, "--json"])["pid"].as_i64())
        .collect()
}

fn live(core: &Core) -> Vec<String> {
    let ls = core.hc(&["ls"]);
    ls["terminals"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|t| t["state"] == "live")
        .filter_map(|t| t["address"].as_str().map(str::to_owned))
        .collect()
}

#[test]
fn shell_detach_reattach_restart_and_exit() {
    let Some(core) = Core::start("m") else { return };
    core.config(
        "[sidebar]\nmode = \"hidden\"\n[tab_bar]\nstyle = \"plain\"\nposition = \"top\"\n[panes]\nstyle = \"plain\"\ntitle = \"off\"\nseparate = false\ninset = 0\n",
    );

    // A fresh workspace with a shell in a core terminal; typing reaches it.
    let mut h = Corgi::spawn(&core);
    h.wait_for("NORMAL", 10);
    h.typ("echo hello-$((40+2))\r");
    h.wait_for("hello-42", 10);

    // A program that paints its background by clearing the screen with a colour set (vim,
    // nvim, less with colours) fills the whole pane, corners included.
    h.typ("printf '\\033[48;2;18;52;86m\\033[2J\\033[H'; echo BCE-DONE\r");
    h.wait_for("BCE-DONE", 10);
    let want = Some(corgi_vt::Rgb(18, 52, 86));
    let end = Instant::now() + Duration::from_secs(5);
    while h.bg(COLS - 1, ROWS - 1) != want {
        assert!(
            Instant::now() < end,
            "bottom-right cell bg {:?}",
            h.bg(COLS - 1, ROWS - 1)
        );
        std::thread::sleep(Duration::from_millis(50));
    }
    assert_eq!(h.bg(COLS / 2, ROWS / 2), want);
    h.typ("printf '\\033[0m'; clear; echo hello-$((40+2))\r");
    h.wait_for("hello-42", 10);

    // Running corgi inside its own pane is refused (it would draw itself forever), like tmux.
    h.typ(&format!("{} ; echo nested-exit=$?\r", env!("CARGO_BIN_EXE_corgi")));
    h.wait_for("refusing to run inside", 10);
    h.wait_for("nested-exit=1", 10);

    // Forced (CORGI_NESTED=1): the inner corgi never shows its own terminal, so nothing
    // mirrors; the outer corgi flags the pane. C-b C-b d sends C-b d to the inner one.
    h.typ(&format!("CORGI_NESTED=1 {}\r", env!("CARGO_BIN_EXE_corgi")));
    h.wait_for("corgi runs in this terminal, so it isn't shown here", 10);
    h.wait_for("is running corgi inside corgi", 10);
    // `corgi doctor` finds it and says how to stop it.
    let doc = Command::new(env!("CARGO_BIN_EXE_corgi"))
        .arg("doctor")
        .current_dir(&core.dir)
        .envs(core.env())
        .output()
        .unwrap();
    let text = String::from_utf8_lossy(&doc.stdout);
    assert_eq!(doc.status.code(), Some(1), "{text}");
    assert!(
        text.contains("runs inside a corgi pane, so it shows itself") && text.contains("kill "),
        "{text}"
    );
    h.typ("\x02\x02d");
    // Wait until it has gone (the warning clears) before typing for the shell again.
    let end = Instant::now() + Duration::from_secs(10);
    while h.screen().iter().any(|l| l.contains("inside corgi")) {
        assert!(
            Instant::now() < end,
            "inner corgi didn't leave:\n{}",
            h.screen().join("\n")
        );
        std::thread::sleep(Duration::from_millis(50));
    }
    h.typ("echo back-in-shell\r");
    h.wait_for("back-in-shell", 10);

    // The prefix as a kitty-protocol terminal sends it (CSI 98;5 u) shows the key hints; C-b d detaches.
    h.typ("\x1b[98;5u");
    h.wait_for("split right", 5);
    h.wait_for("detach", 5);
    h.typ("d");
    assert_eq!(h.wait_exit(5), 0);
    assert_eq!(live(&core).len(), 1, "the shell keeps running");

    // Running corgi again restores the layout from core; the first paint brings the screen back.
    let mut h = Corgi::spawn(&core);
    h.wait_for("hello-42", 10);
    assert!(h.screen()[0].contains("0:"), "window list: {:?}", h.screen()[0]);

    // The pane's program sees corgi's size (one row goes to the status line).
    h.typ("clear; echo size=$(tput cols)x$(tput lines)\r");
    h.wait_for(&format!("size={COLS}x{}", ROWS - 1), 10);

    // A core restart drops the line; corgi reconnects and the terminal still answers.
    core.hc(&["server", "restart"]);
    std::thread::sleep(Duration::from_millis(500));
    let end = Instant::now() + Duration::from_secs(15);
    loop {
        h.typ("echo back-$((1+1))\r");
        if h.screen().iter().any(|l| l.contains("back-2")) {
            break;
        }
        assert!(
            Instant::now() < end,
            "no echo after restart:\n{}",
            h.screen().join("\n")
        );
        std::thread::sleep(Duration::from_millis(500));
    }

    // As in tmux: the last pane's program exits, corgi says [exited] and leaves.
    h.typ("exit\r");
    assert_eq!(h.wait_exit(10), 0);
    assert!(String::from_utf8_lossy(&h.out.lock().unwrap()).contains("[exited]"));
    assert!(live(&core).is_empty());
}

#[test]
fn windows_splits_and_layout_survive_a_restart_of_corgi() {
    let Some(core) = Core::start("l") else { return };
    core.config(
        "[sidebar]\nmode = \"hidden\"\n[tab_bar]\nstyle = \"plain\"\nposition = \"top\"\n[panes]\nstyle = \"plain\"\ntitle = \"off\"\nseparate = false\ninset = 0\n",
    );
    let mut h = Corgi::spawn(&core);
    h.wait_for("NORMAL", 10);

    // C-b %: split right. The new pane is focused and has its own shell.
    h.typ("\x02%");
    h.wait_for("│", 10);
    h.typ("echo RIGHT-$((1+1))\r");
    let s = h.wait_for("RIGHT-2", 10);
    let col = |s: &[String], what: &str| s.iter().find_map(|l| l.find(what)).unwrap();
    assert!(
        col(&s, "RIGHT-2") > (COLS / 2) as usize,
        "typed into the right pane:\n{}",
        s.join("\n")
    );

    // C-b Left, then type: it goes to the left pane.
    h.typ("\x02\x1b[D");
    std::thread::sleep(Duration::from_millis(300));
    h.typ("echo LEFT-$((2+2))\r");
    let s = h.wait_for("LEFT-4", 10);
    assert!(col(&s, "LEFT-4") < (COLS / 2) as usize);

    // C-b c: a new window, listed as 1; C-b 0 goes back.
    h.typ("\x02c");
    h.wait_for(" 1:", 10);
    h.typ("echo WINDOW-ONE\r");
    h.wait_for("WINDOW-ONE", 10);
    h.typ("\x020");
    h.wait_for("LEFT-4", 10);

    // C-b z: zoom (flag Z, no separator), C-b z again: back to two panes.
    h.typ("\x02z");
    let end = Instant::now() + Duration::from_secs(5);
    while !h.screen()[0].contains("Z ") {
        assert!(Instant::now() < end, "no zoom flag: {:?}", h.screen()[0]);
        std::thread::sleep(Duration::from_millis(50));
    }
    assert!(!h.screen()[1..].iter().any(|l| l.contains('│')), "zoomed: one pane");
    h.typ("\x02z");
    h.wait_for("│", 5);
    assert_eq!(live(&core).len(), 3);

    // Detach, run again: same windows, same split, same screens.
    h.typ("\x02d");
    assert_eq!(h.wait_exit(5), 0);
    let mut h = Corgi::spawn(&core);
    let s = h.wait_for("LEFT-4", 10);
    assert!(s[0].contains(" 0:") && s[0].contains(" 1:"), "{:?}", s[0]);
    h.wait_for("RIGHT-2", 10);
    h.wait_for("│", 5);

    // C-b x: kill the focused (left) pane, a plain shell, so without asking (confirm_close =
    // "auto"); the right one takes the space.
    h.typ("\x02x");
    let end = Instant::now() + Duration::from_secs(10);
    while h.screen()[1..].iter().any(|l| l.contains('│')) {
        assert!(Instant::now() < end, "pane not closed:\n{}", h.screen().join("\n"));
        std::thread::sleep(Duration::from_millis(50));
    }
    assert_eq!(live(&core).len(), 2);
    h.typ("\x02d");
    assert_eq!(h.wait_exit(5), 0);
}

/// The way herdr app users run it: corgi itself inside a core terminal (driven
/// through core: `hc send`, `hc keys`, `hc read`).
#[test]
fn corgi_inside_a_core_terminal() {
    let Some(core) = Core::start("i") else { return };
    let bin = env!("CARGO_BIN_EXE_corgi");
    let run = core.hc(&["run", "--bg", "--name", "outer", "--", bin]);
    assert!(run["address"].is_string(), "{run}");
    let read = || {
        core.hc(&["read", "--json", "outer"])["text"]
            .as_str()
            .unwrap_or_default()
            .to_string()
    };
    let wait = |what: &str| {
        let end = Instant::now() + Duration::from_secs(15);
        loop {
            let s = read();
            if s.contains(what) {
                return s;
            }
            assert!(Instant::now() < end, "never saw {what:?}:\n{s}");
            std::thread::sleep(Duration::from_millis(100));
        }
    };
    wait("NORMAL");

    // Typing reaches corgi's pane, not corgi's own terminal.
    core.hc(&["send", "outer", "echo inside-$((2+3))"]);
    wait("inside-5");

    // From that pane, a nested corgi is refused.
    core.hc(&["send", "outer", &format!("{bin}; echo code=$?")]);
    wait("refusing to run inside");
    wait("code=1");

    // C-b d detaches: corgi's terminal finishes, its pane's shell keeps running.
    core.hc(&["keys", "outer", "ctrl+b", "d"]);
    let end = Instant::now() + Duration::from_secs(10);
    loop {
        let ls = core.hc(&["ls"]);
        let live: Vec<&serde_json::Value> = ls["terminals"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|t| t["state"] == "live")
            .collect();
        if live.len() == 1 {
            break;
        }
        assert!(Instant::now() < end, "corgi didn't detach: {ls}");
        std::thread::sleep(Duration::from_millis(100));
    }
}

#[test]
fn sidebar_toggle_and_config_reload() {
    let Some(core) = Core::start("c") else { return };
    let mut h = Corgi::spawn(&core);
    // Default: sidebar with the workspace (named after its folder); the tab bar at the bottom.
    // The header comes first; the workspace row once its shell has started.
    h.wait_for("Workspaces", 10);
    let end = Instant::now() + Duration::from_secs(10);
    let s = loop {
        let s = h.screen();
        // The box's name on a row of its own, a line under it (`sidebar.header = "row"`).
        if s[0].starts_with("┌──")
            && s[1].starts_with("│ Workspaces")
            && s[2].starts_with("├──")
            && s[3].starts_with("│ ▾ c")
            && s[4].contains("0:")
            && tab_row(&s).is_some()
        {
            break s;
        }
        assert!(Instant::now() < end, "workspace row: {:#?}", s);
        std::thread::sleep(Duration::from_millis(50));
    };
    let r = ROWS as usize;
    assert_eq!(tab_row(&s), Some(r - 1), "tab bar on the last row");
    // One plain row across the whole window, under the sidebar too: the mode badge starts it.
    assert!(s[r - 1].starts_with(" NORMAL "), "{:?}", s[r - 1]);
    assert!(
        !s[r - 1].contains('│') && !s[r - 1].contains('└'),
        "no borders on it: {:?}",
        s[r - 1]
    );
    // Above it, the sidebar's box and the panes' box (one column after the sidebar: 26 + padding 1).
    assert_eq!(s[r - 2].chars().next(), Some('└'), "sidebar box bottom: {:?}", s[r - 2]);
    assert_eq!(s[0].chars().nth(27), Some('┌'), "pane box: {:?}", s[0]);
    assert_eq!(s[r - 2].chars().nth(27), Some('└'), "pane box bottom: {:?}", s[r - 2]);

    // The key hints: across the whole window (sidebar too), just above the bottom tab bar.
    h.typ("\x02");
    let s = h.wait_for("┌ C-b", 5);
    let bar = tab_row(&s).unwrap();
    assert!(s[bar].contains(" PREFIX "), "badge in prefix mode, still in sight");
    let top = s.iter().position(|l| l.starts_with("┌ C-b")).unwrap();
    assert!(s[top].trim_end().ends_with('┐'), "full width: {:?}", s[top]);
    assert_eq!(s[top].trim_end().chars().count(), COLS as usize);
    assert!(s[bar - 1].starts_with('└'), "docked on the tab bar: {:?}", s[bar - 1]);
    assert!(top > 2, "only as tall as it needs");
    h.typ("\x1b");
    // A lone Esc then more keys at once would read as Alt+key: give it a moment.
    std::thread::sleep(Duration::from_millis(200));

    // The tree: a second window shows under the workspace; splitting one makes it foldable.
    h.typ("\x02c");
    h.wait_for(" 1:", 10);
    h.typ("\x02%");
    let end = Instant::now() + Duration::from_secs(10);
    while !h.screen()[5].contains("▸ 1:") {
        assert!(Instant::now() < end, "window rows:\n{}", h.screen()[..7].join("\n"));
        std::thread::sleep(Duration::from_millis(50));
    }
    h.typ("echo IN-WINDOW-ONE\r");
    h.wait_for("IN-WINDOW-ONE", 10);

    // C-b w: the chooser, at the current window. Centred on the whole window like every
    // dialog. Typing filters; Enter goes there.
    h.typ("\x02w");
    let s = h.wait_for("type to filter", 5);
    let top = s.iter().position(|l| l.contains("┌ choose")).unwrap();
    let left = s[top].chars().position(|c| c == '┌').unwrap();
    let right = s[top].chars().position(|c| c == '┐').unwrap();
    let bottom = (top..s.len()).find(|&y| s[y].chars().nth(left) == Some('└')).unwrap();
    assert!(
        (left as i32 - (COLS as i32 - 1 - right as i32)).abs() <= 1,
        "chooser horizontal: {left}..{right}"
    );
    assert!(
        (top as i32 - (ROWS as i32 - 1 - bottom as i32)).abs() <= 1,
        "chooser vertical: {top}..{bottom}"
    );
    assert!(s[tab_row(&s).unwrap()].contains(" CHOOSE "), "badge while choosing");
    h.typ("0:");
    h.wait_for("filter: 0:", 5);
    h.typ("\r");
    let end = Instant::now() + Duration::from_secs(10);
    loop {
        let s = h.screen();
        let line = tab_row(&s).map(|i| s[i].clone()).unwrap_or_default();
        if line.find(" 0:").zip(line.find('*')).is_some_and(|(a, b)| b > a) && !line.contains("CHOOSE") {
            break;
        }
        assert!(Instant::now() < end, "didn't go to window 0: {line:?}");
        std::thread::sleep(Duration::from_millis(50));
    }
    assert!(
        !h.screen().iter().any(|l| l.contains("type to filter")),
        "chooser closed"
    );

    // In the chooser, ← on the workspace closes it; the sidebar folds with it (shared state).
    h.typ("\x02s");
    h.wait_for("type to filter", 5);
    h.typ("\x1b[D");
    h.typ("\x1b");
    h.typ("\x1b");
    let end = Instant::now() + Duration::from_secs(5);
    while h.screen()[4].contains("0:") || !h.screen()[3].starts_with("│ ▸ c") {
        assert!(Instant::now() < end, "not folded:\n{}", h.screen()[..6].join("\n"));
        std::thread::sleep(Duration::from_millis(50));
    }

    // C-b b hides it; the pane takes the room.
    h.typ("\x02b");
    let end = Instant::now() + Duration::from_secs(5);
    while h.screen().iter().any(|l| l.contains("Workspaces")) {
        assert!(Instant::now() < end, "sidebar still shown");
        std::thread::sleep(Duration::from_millis(50));
    }
    // Window 0 is split in two inside the pane box: the right (focused) pane gets half of it.
    h.typ("clear; echo cols=$(tput cols)\r");
    // Each pane in its own box, less the inset each side.
    h.wait_for(&format!("cols={}", (COLS - 3) / 2 - 2), 10);

    // A config that moves the tab bar to the top and renames the badge, reloaded with C-b R.
    core.config("[tab_bar]\nposition = \"top\"\n[mode]\nnormal = \"-- N --\"\n[theme]\nname = \"nord\"\n[sidebar]\nmode = \"hidden\"\n");
    h.typ("\x02R");
    h.wait_for("config reloaded", 5);
    let s = h.screen();
    assert_eq!(
        tab_row(&s),
        Some(0),
        "tab bar on the first row:
{}",
        s[..3].join(
            "
"
        )
    );
    assert!(s[1].starts_with('┌'), "the pane box below it: {:?}", s[1]);
    assert!(s[0].contains(" -- N -- "), "custom badge: {:?}", s[0]);
    // A rename dialog says so in the badge.
    h.typ("\x02,");
    let s = h.wait_for("Rename Window", 5);
    assert!(s[0].contains(" RENAME "), "{:?}", s[0]);
    h.typ("\x1b");
    std::thread::sleep(Duration::from_millis(200));

    // A broken config keeps the current one and says why.
    core.config("[tab_bar]\nposition = \"sideways\"\n");
    h.typ("\x02R");
    h.wait_for("config not reloaded", 5);
    h.typ("\x02d");
    assert_eq!(h.wait_exit(5), 0);

    // At startup, a broken config is an error before the screen is taken.
    let out = Command::new(env!("CARGO_BIN_EXE_corgi"))
        .current_dir(&core.dir)
        .envs(core.env())
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(1));
    assert!(
        String::from_utf8_lossy(&out.stderr).contains("sideways"),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn sidebar_focus_resize_by_keys_and_mouse() {
    let Some(core) = Core::start("r") else { return };
    let mut h = Corgi::spawn(&core);
    // The box's top-right corner marks the sidebar's right edge.
    let edge = |h: &Corgi| h.screen()[0].chars().position(|c| c == '┐');
    let wait_edge = |h: &Corgi, want: usize| {
        let end = Instant::now() + Duration::from_secs(5);
        while edge(h) != Some(want) {
            assert!(
                Instant::now() < end,
                "edge at {:?}, want {want}: {:?}",
                edge(h),
                h.screen()[0]
            );
            std::thread::sleep(Duration::from_millis(50));
        }
    };
    h.wait_for("Workspaces", 10);
    h.wait_for("$", 10);
    wait_edge(&h, 25); // default width 26

    // C-b ← from the only pane: the sidebar takes the keyboard; C-b → gives it back.
    h.typ("\x02\x1b[D");
    h.wait_for(" SIDEBAR ", 5);
    std::thread::sleep(Duration::from_millis(600));
    h.typ("\x02\x1b[C");
    h.wait_for(" NORMAL ", 5);
    // The same with vi keys: C-b h; then j/k move in the sidebar, not into the shell.
    h.typ("\x02h");
    h.wait_for(" SIDEBAR ", 5);
    h.typ("jjkk");
    std::thread::sleep(Duration::from_millis(300));
    assert!(
        !h.screen().iter().any(|l| l.contains("jjkk")),
        "keys leaked into the pane"
    );

    // C-b C-→, then → twice more within the repeat time: three columns wider. C-b M-←: five narrower.
    h.typ("\x02\x1b[1;5C\x1b[1;5C\x1b[1;5C");
    wait_edge(&h, 28);
    std::thread::sleep(Duration::from_millis(600));
    h.typ("\x02\x1b[1;3D");
    wait_edge(&h, 23);

    // C-b l: back to the pane, which has the room the sidebar left.
    std::thread::sleep(Duration::from_millis(600));
    h.typ("\x02l");
    std::thread::sleep(Duration::from_millis(200));
    h.typ("echo pane-cols=$(tput cols)\r");
    // Sidebar 24 wide, then the default one column of padding.
    // …and the pane box's two side borders.
    // …and the inset each side (panes.inset = 1).
    h.wait_for(&format!("pane-cols={}", COLS - 24 - 1 - 2 - 2), 10);

    // Drag the edge with the mouse (SGR: press, drag, release; 1-based).
    h.typ("\x1b[<0;24;6M\x1b[<32;35;6M\x1b[<32;41;6M\x1b[<0;41;6m");
    wait_edge(&h, 40);

    // sidebar.padding widens the gap between sidebar and panes (reloaded live); the gap stays empty.
    core.config("[sidebar]\npadding = 4\n");
    h.typ("\x02R");
    h.wait_for("config reloaded", 5);
    h.typ("clear; echo pane-cols=$(tput cols)\r");
    h.wait_for(&format!("pane-cols={}", COLS - 41 - 4 - 2 - 2), 10);
    // Lines come back trimmed: pad to the full width first.
    let row: Vec<char> = format!("{:<w$}", h.screen()[5], w = COLS as usize).chars().collect();
    assert!(row[41..45].iter().all(|c| *c == ' '), "gap: {:?}", &row[38..48]);
    assert_eq!(edge(&h), Some(40), "the sidebar itself didn't move");

    // The width is this client's: kept across a restart of corgi.
    h.typ("\x02d");
    assert_eq!(h.wait_exit(5), 0);
    let h = Corgi::spawn(&core);
    h.wait_for("Workspaces", 10);
    wait_edge(&h, 40);
}

#[test]
fn an_agent_leaves_the_sidebar_when_it_clears_or_exits() {
    let Some(core) = Core::start("g") else { return };
    let mut h = Corgi::spawn(&core);
    h.wait_for("Workspaces", 10);
    h.wait_for("$", 10);
    // Look only at the sidebar's columns: the pane shows the commands that were typed.
    let sidebar = |h: &Corgi| -> Vec<String> { h.screen().iter().map(|l| l.chars().take(26).collect()).collect() };
    let listed = |h: &Corgi, name: &str| sidebar(h).iter().any(|l| l.contains(name));
    let wait_listed = |h: &Corgi, name: &str, want: bool| {
        let end = Instant::now() + Duration::from_secs(10);
        // The agents' half stays; with none left it says so.
        while listed(h, name) != want || (!want && !listed(h, "no agents running")) {
            assert!(
                Instant::now() < end,
                "{name} listed={}:\n{}",
                !want,
                sidebar(h)[..10].join("\n")
            );
            std::thread::sleep(Duration::from_millis(50));
        }
    };
    // A fake agent (any harness: a child of the shell that speaks OSC 7501) that clears its
    // status on the way out, as Claude Code does.
    h.typ("sh -c 'printf \"\\033]7501;state=working:app=agent-one\\033\\\\\"; sleep 1; printf \"\\033]7501;state=clear\\033\\\\\"; sleep 600'\r");
    wait_listed(&h, "agent-one", true);
    // In the sidebar's bottom half, its own box, under the tree's.
    let s = sidebar(&h);
    let at = s.iter().position(|l| l.contains("agent-one")).unwrap();
    let title = s.iter().position(|l| l.contains("│ Agents")).unwrap();
    assert!(
        title >= (ROWS as usize - 1) / 2 - 1 && at > title,
        "agents box at {title}, agent at {at}:\n{}",
        s.join("\n")
    );
    wait_listed(&h, "agent-one", false);
    h.typ("\x03");
    // One that never clears and is killed: once the shell is back in front, it's gone too.
    h.typ("sh -c 'printf \"\\033]7501;state=working:app=agent-two\\033\\\\\"; sleep 600'\r");
    wait_listed(&h, "agent-two", true);
    h.typ("\x03");
    wait_listed(&h, "agent-two", false);
    h.typ("\x02d");
    assert_eq!(h.wait_exit(5), 0);
}

#[test]
fn bindings_command_prompt_list_keys_and_pane_numbers() {
    let Some(core) = Core::start("k") else { return };
    core.config(
        r#"
[sidebar]
mode = "hidden"
[keys]
unbind = ["%"]
[[keys.bind]]
key = "|"
command = "split-window -h"
note = "split right (mine)"
[[keys.bind]]
key = "M-z"
table = "root"
command = "resize-pane -Z"
"#,
    );
    let mut h = Corgi::spawn(&core);
    h.wait_for("NORMAL", 10);
    h.wait_for("$", 10);
    let live_count = |n: usize| {
        let end = Instant::now() + Duration::from_secs(10);
        while live(&core).len() != n {
            assert!(Instant::now() < end, "{} terminals, want {n}", live(&core).len());
            std::thread::sleep(Duration::from_millis(50));
        }
    };
    let bar = |h: &Corgi| h.screen()[ROWS as usize - 1].clone();
    let wait_bar = |h: &Corgi, what: &str| {
        let end = Instant::now() + Duration::from_secs(5);
        while !bar(h).contains(what) {
            assert!(Instant::now() < end, "tab bar never showed {what:?}: {:?}", bar(h));
            std::thread::sleep(Duration::from_millis(50));
        }
    };

    // Unbound: C-b % does nothing. Bound: C-b | splits.
    h.typ("\x02%");
    std::thread::sleep(Duration::from_millis(500));
    assert_eq!(live(&core).len(), 1, "% was unbound");
    h.typ("\x02|");
    live_count(2);

    // The command prompt: badge, a command, a mistake, completion, history.
    h.typ("\x02:");
    wait_bar(&h, "COMMAND");
    h.typ("rename-window tests\r");
    wait_bar(&h, "0:tests");
    h.typ("\x02:frobnicate\r");
    wait_bar(&h, "unknown command");
    h.typ("\x02:spli\t");
    wait_bar(&h, ":  split-window");
    h.typ("\x1b");
    std::thread::sleep(Duration::from_millis(200));
    h.typ("\x02:\x1b[A");
    wait_bar(&h, "frobnicate");
    h.typ("\x1b[A");
    wait_bar(&h, "rename-window tests");
    h.typ("\x1b");
    std::thread::sleep(Duration::from_millis(200));

    // C-b ?: every key, with my binding's note; typing filters.
    h.typ("\x02?");
    h.wait_for("type to filter", 5);
    h.typ("mine");
    let s = h.wait_for("split right (mine)", 5);
    assert!(
        s.iter().any(|l| l.contains("|  split right (mine)  split-window -h")),
        "{}",
        s.join("\n")
    );
    assert!(bar(&h).contains("KEYS"));
    // Esc clears the filter, a second Esc closes (sent apart: together they'd read as M-Esc).
    h.typ("\x1b");
    std::thread::sleep(Duration::from_millis(200));
    h.typ("\x1b");
    std::thread::sleep(Duration::from_millis(300));
    assert!(
        !h.screen()
            .iter()
            .any(|l| l.contains("type to filter") || l.contains("filter:")),
        "closed"
    );

    // C-b q: pane numbers; 0 picks the left pane, where the next command lands.
    h.typ("\x02q");
    h.wait_for(" 1 ", 5);
    h.typ("0");
    std::thread::sleep(Duration::from_millis(200));
    h.typ("echo LEFT-$((3+3))\r");
    let s = h.wait_for("LEFT-6", 10);
    let col = s.iter().find_map(|l| l.find("LEFT-6")).unwrap();
    assert!(col < (COLS / 2) as usize, "typed into the left pane");

    // A root binding (no prefix): M-z zooms.
    h.typ("\x1bz");
    wait_bar(&h, "Z ");
    h.typ("\x02d");
    assert_eq!(h.wait_exit(5), 0);
}

#[test]
fn herdr_preset_and_resize_mode() {
    let Some(core) = Core::start("h") else { return };
    core.config("[sidebar]\nmode = \"hidden\"\n[keys]\npreset = \"herdr\"\n");
    let mut h = Corgi::spawn(&core);
    h.wait_for("NORMAL", 10);
    h.wait_for("$", 10);
    // herdr's v: split right.
    h.typ("\x02v");
    let end = Instant::now() + Duration::from_secs(10);
    while live(&core).len() != 2 {
        assert!(Instant::now() < end, "C-b v didn't split");
        std::thread::sleep(Duration::from_millis(50));
    }
    // C-b r: resize mode until Esc; the badge says so; h/l move the divider, repeatedly.
    // The left pane's box ends where the divider is (each pane has its own box).
    // One read of the row: two boxes, the first one's right edge.
    let divider = |h: &Corgi| {
        let row = h.screen()[0].clone();
        row.find('┐').filter(|&d| row[d..].contains('┌'))
    };
    let end = Instant::now() + Duration::from_secs(10);
    let before = loop {
        if let Some(d) = divider(&h) {
            break d;
        }
        assert!(Instant::now() < end, "no divider: {:?}", h.screen()[0]);
        std::thread::sleep(Duration::from_millis(50));
    };
    h.typ("\x02r");
    h.wait_for("RESIZE", 5);
    h.typ("hhhh");
    let end = Instant::now() + Duration::from_secs(5);
    while divider(&h) == Some(before) {
        assert!(Instant::now() < end, "resize mode didn't resize");
        std::thread::sleep(Duration::from_millis(50));
    }
    assert!(
        divider(&h).unwrap() < before,
        "h grows leftwards: {before} → {:?}",
        divider(&h)
    );
    h.typ("\x1b");
    std::thread::sleep(Duration::from_millis(200));
    assert!(
        !h.screen()[ROWS as usize - 1].contains("RESIZE"),
        "Esc left resize mode"
    );
    // herdr's q: detach.
    h.typ("\x02q");
    assert_eq!(h.wait_exit(5), 0);
}

#[test]
fn pane_tabs_show_the_directory_or_the_agents_summary() {
    let Some(core) = Core::start("pt") else { return };
    core.config("[sidebar]\nmode = \"hidden\"\n[panes]\ntitle_shape = \"rounded\"\n");
    let mut h = Corgi::spawn(&core);
    h.wait_for("NORMAL", 10);
    h.wait_for("$", 10);
    let top = |h: &Corgi| h.screen()[0].clone();
    let wait_top = |h: &Corgi, what: &str| {
        let end = Instant::now() + Duration::from_secs(10);
        while !top(h).contains(what) {
            assert!(Instant::now() < end, "pane tab never showed {what:?}: {:?}", top(h));
            std::thread::sleep(Duration::from_millis(50));
        }
    };
    // A shell: its directory's name, in a rounded tab on the box's top edge.
    wait_top(&h, "\u{e0b6}pt\u{e0b4}");
    assert!(top(&h).starts_with("┌─\u{e0b6}"), "{:?}", top(&h));
    // An agent: what it says it's doing (OSC 7501 `title`, base64), with its status glyph; then the
    // directory again once it's gone.
    h.typ("sh -c 'printf \"\\033]7501;state=working:app=agent-one:title=Rml4aW5nIHRoZSBidWc=\\033\\\\\"; sleep 600'\r");
    wait_top(&h, "Fixing the bug");
    assert!(top(&h).contains("● Fixing the bug"), "{:?}", top(&h));
    h.typ("\x03");
    wait_top(&h, "\u{e0b6}pt\u{e0b4}");
    // A split: each pane its own tab on the same edge; the focused one (the new, right pane)
    // is a solid pill, the other hollow.
    let (solid, hollow) = ("\u{e0b6}pt\u{e0b4}", "\u{e0b7}pt\u{e0b5}");
    h.typ("\x02%");
    let end = Instant::now() + Duration::from_secs(10);
    while !(top(&h).contains(solid) && top(&h).contains(hollow)) {
        assert!(Instant::now() < end, "one solid, one hollow: {:?}", top(&h));
        std::thread::sleep(Duration::from_millis(50));
    }
    assert!(
        top(&h).find(hollow) < top(&h).find(solid),
        "the left one is out of focus"
    );
    // Focus moves left: the pills swap.
    h.typ("\x02\x1b[D");
    let end = Instant::now() + Duration::from_secs(5);
    while top(&h).find(solid) > top(&h).find(hollow) {
        assert!(Instant::now() < end, "pills didn't follow focus: {:?}", top(&h));
        std::thread::sleep(Duration::from_millis(50));
    }
    h.typ("\x02d");
    assert_eq!(h.wait_exit(5), 0);
}

#[test]
fn notifications_toast_inside_corgi_and_reach_the_host() {
    let Some(core) = Core::start("n") else { return };
    core.config(
        "[sidebar]\nmode = \"hidden\"\n[tab_bar]\nposition = \"bottom\"\n[panes]\nstyle = \"plain\"\ntitle = \"off\"\n\
         [notifications]\nhost = \"osc777\"\nduration = \"30s\"\n",
    );
    let mut h = Corgi::spawn(&core);
    h.wait_for("$", 10);
    let host = |h: &Corgi, what: &str| {
        let out = h.out.lock().unwrap();
        String::from_utf8_lossy(&out).matches(what).count()
    };
    // The left pane notifies in a second; by then the right pane (split off it) is focused.
    h.typ("sleep 1; printf '\\033]777;notify;Build;all green\\007'\r");
    h.typ("\x02%");
    h.wait_for("│", 10);
    let s = h.wait_for("◆ Build", 10);
    let row = s.iter().position(|l| l.contains("◆ Build")).unwrap();
    assert!(s[row + 1].contains("all green"), "{}", s.join("\n"));
    assert!(
        s[row].find("◆").unwrap() > (COLS / 2) as usize,
        "top-right:\n{}",
        s.join("\n")
    );
    // The host hasn't said whether its window is focused: it's told too.
    assert_eq!(host(&h, "\x1b]777;notify;Build;all green\x07"), 1);

    // Now it says it's focused: toasts only. The pane you're in gets none.
    h.typ("\x1b[I");
    std::thread::sleep(Duration::from_millis(200));
    h.typ("printf '\\033]9;mine\\007'\r");
    std::thread::sleep(Duration::from_millis(800));
    assert!(!h.screen().iter().any(|l| l.contains("mine") && l.contains("◆")));
    assert_eq!(host(&h, "777;notify;sh;mine"), 0, "not sent on");

    // An agent in the left pane that needs you, through its status (OSC 7501).
    h.typ("\x02\x1b[D");
    std::thread::sleep(Duration::from_millis(300));
    h.typ("sleep 1; printf '\\033]7501;state=blocked:app=fake-agent:msg=QWxsb3cgcm0/\\033\\\\'; sleep 600\r");
    h.typ("\x02\x1b[C");
    let s = h.wait_for("fake-agent needs you", 10);
    assert!(
        !s.iter().any(|l| l.contains("◆ Build")),
        "replaced: same pane\n{}",
        s.join("\n")
    );
    let row = s.iter().position(|l| l.contains("fake-agent needs you")).unwrap();
    assert!(s[row + 1].contains("Allow rm?"), "{}", s.join("\n"));
    assert_eq!(host(&h, "777;notify;fake-agent"), 0, "host focused: not sent on");

    // Click it: the left pane is focused and the toast goes.
    let col = s[row].find("fake-agent").unwrap() + 1;
    h.typ(&format!("\x1b[<0;{col};{}M\x1b[<0;{col};{}m", row + 1, row + 1));
    let end = Instant::now() + Duration::from_secs(5);
    while h.screen().iter().any(|l| l.contains("needs you")) {
        assert!(Instant::now() < end, "toast stayed:\n{}", h.screen().join("\n"));
        std::thread::sleep(Duration::from_millis(50));
    }
    h.typ("\x03");
    h.typ("echo LEFT-$((3+3))\r");
    let s = h.wait_for("LEFT-6", 10);
    assert!(s.iter().find_map(|l| l.find("LEFT-6")).unwrap() < (COLS / 2) as usize);
    h.typ("\x02d");
    assert_eq!(h.wait_exit(5), 0);
}

#[test]
fn split_panes_in_their_own_boxes_or_sharing_dividers() {
    let Some(core) = Core::start("sp") else { return };
    core.config("[sidebar]\nmode = \"hidden\"\n[panes]\ntitle = \"off\"\npadding = 2\n");
    let mut h = Corgi::spawn(&core);
    h.wait_for("NORMAL", 10);
    h.wait_for("$", 10);
    let top = |h: &Corgi| h.screen()[0].clone();
    // Separate (the default): each pane its own box, here 2 cells apart.
    h.typ("\x02%");
    let end = Instant::now() + Duration::from_secs(10);
    while !top(&h).contains("┐  ┌") {
        assert!(Instant::now() < end, "two boxes, 2 apart: {:?}", top(&h));
        std::thread::sleep(Duration::from_millis(50));
    }
    assert!(!top(&h).contains('┬'), "no shared divider: {:?}", top(&h));
    // Shared dividers instead, after a config reload.
    core.config("[sidebar]\nmode = \"hidden\"\n[panes]\ntitle = \"off\"\nseparate = false\n");
    h.typ("\x02R");
    let end = Instant::now() + Duration::from_secs(10);
    while !top(&h).contains('┬') {
        assert!(Instant::now() < end, "one shared divider: {:?}", top(&h));
        std::thread::sleep(Duration::from_millis(50));
    }
    assert!(!top(&h).contains("┐┌") && !top(&h).contains("┐  ┌"), "{:?}", top(&h));
    h.typ("\x02d");
    assert_eq!(h.wait_exit(5), 0);
}

#[test]
fn new_window_and_workspace_dialogs() {
    let Some(core) = Core::start("dl") else { return };
    core.config("[panes]\ntitle = \"off\"\n");
    std::fs::create_dir_all(core.dir.join("proj")).unwrap();
    let mut h = Corgi::spawn(&core);
    h.wait_for("NORMAL", 10);
    h.wait_for("$", 10);
    let bar = |h: &Corgi| h.screen()[ROWS as usize - 1].clone();
    let wait = |what: &dyn Fn() -> bool, why: &str, h: &Corgi| {
        let end = Instant::now() + Duration::from_secs(10);
        while !what() {
            assert!(Instant::now() < end, "{why}:\n{}", h.screen().join("\n"));
            std::thread::sleep(Duration::from_millis(50));
        }
    };
    let pause = || std::thread::sleep(Duration::from_millis(250));

    // C-b C: the New Window dialog, with the NEW badge; a name, Enter.
    h.typ("\x02C");
    h.wait_for("New Window", 5);
    assert!(bar(&h).contains(" NEW "), "{:?}", bar(&h));
    h.typ("logs");
    pause();
    h.typ("\r");
    wait(&|| bar(&h).contains("1:logs"), "the named window", &h);
    assert_eq!(live(&core).len(), 2);

    // Esc: nothing.
    h.typ("\x02C");
    h.wait_for("New Window", 5);
    pause();
    h.typ("\x1b");
    wait(&|| !h.screen().iter().any(|l| l.contains("New Window")), "closed", &h);
    std::thread::sleep(Duration::from_millis(500));
    assert_eq!(live(&core).len(), 2, "cancelled: nothing started");

    // A window with an initial layout: down to Layout (Name, Folder, Program, Layout), → once.
    h.typ("\x02C");
    h.wait_for("New Window", 5);
    pause();
    for _ in 0..3 {
        h.typ("\x1b[B");
        pause();
    }
    h.typ("\x1b[C");
    h.wait_for("2 side by side", 5);
    h.typ("\r");
    let end = Instant::now() + Duration::from_secs(10);
    while live(&core).len() != 4 {
        assert!(Instant::now() < end, "{} terminals", live(&core).len());
        std::thread::sleep(Duration::from_millis(50));
    }
    wait(&|| bar(&h).contains("2:"), "the third window", &h);

    // C-b N: the folder starts where you are; a wrong one says so; a right one makes it.
    h.typ("\x02N");
    h.wait_for("New Workspace", 5);
    pause();
    h.typ("\x15/no/such/folder");
    pause();
    h.typ("\r");
    h.wait_for("isn't a folder here", 5);
    h.typ(&format!("\x15{}", core.dir.join("proj").display()));
    pause();
    h.typ("\t");
    pause();
    h.typ("\tmine");
    pause();
    h.typ("\r");
    h.wait_for("mine", 10);
    wait(
        &|| !h.screen().iter().any(|l| l.contains("New Workspace")),
        "closed",
        &h,
    );
    let end = Instant::now() + Duration::from_secs(10);
    while live(&core).len() != 5 {
        assert!(Instant::now() < end, "{} terminals", live(&core).len());
        std::thread::sleep(Duration::from_millis(50));
    }
    // The new workspace may not be on screen yet (its terminal exists before corgi has put it
    // in): `pwd` until it answers from there.
    let end = Instant::now() + Duration::from_secs(10);
    while !h.screen().iter().any(|l| l.contains("/proj")) {
        assert!(Instant::now() < end, "{:#?}", h.screen());
        h.typ("pwd\r");
        std::thread::sleep(Duration::from_millis(700));
    }
    h.typ("\x02d");
    assert_eq!(h.wait_exit(5), 0);
}

#[test]
fn rename_and_close_dialogs() {
    let Some(core) = Core::start("rc") else { return };
    core.config("[sidebar]\nmode = \"hidden\"\n");
    let mut h = Corgi::spawn(&core);
    h.wait_for("NORMAL", 10);
    h.wait_for("$", 10);
    let bar = |h: &Corgi| h.screen()[ROWS as usize - 1].clone();
    // The pane's title: on its top edge or the row under it, as the title style puts it.
    let top = |h: &Corgi| h.screen()[..2].join("\n");
    let until = |ok: &dyn Fn() -> bool, why: &str, h: &Corgi| {
        let end = Instant::now() + Duration::from_secs(10);
        while !ok() {
            assert!(Instant::now() < end, "{why}:\n{}", h.screen().join("\n"));
            std::thread::sleep(Duration::from_millis(50));
        }
    };
    let pause = || std::thread::sleep(Duration::from_millis(250));
    let count = |n: usize| {
        let end = Instant::now() + Duration::from_secs(10);
        while live(&core).len() != n {
            assert!(Instant::now() < end, "{} terminals, want {n}", live(&core).len());
            std::thread::sleep(Duration::from_millis(50));
        }
    };

    // C-b ,: the window's name, edited in a dialog.
    h.typ("\x02,");
    h.wait_for("Rename Window", 5);
    assert!(bar(&h).contains(" RENAME "), "{:?}", bar(&h));
    pause();
    h.typ("\x15edit\r");
    until(&|| bar(&h).contains("0:edit"), "renamed window", &h);

    // A pane's name shows in its tab; renaming it to nothing goes back to its folder.
    h.typ("\x02:rename-pane build\r");
    until(&|| top(&h).contains("build"), "named pane", &h);
    h.typ("\x02:rename-pane\r");
    h.wait_for("Rename Pane", 5);
    pause();
    h.typ("\x15\r");
    until(
        &|| !top(&h).contains("build") && top(&h).contains("rc"),
        "unnamed pane",
        &h,
    );

    // Closing a pane with nothing but a shell: no question (confirm_close = "auto").
    h.typ("\x02%");
    count(2);
    pause();
    h.typ("\x02x");
    count(1);
    assert!(!h.screen().iter().any(|l| l.contains("Close Pane")));

    // With an agent in it: asked, with a warning. n keeps it; y closes it.
    h.typ("\x02%");
    count(2);
    h.wait_for("$", 10);
    pause();
    h.typ("sh -c 'printf \"\\033]7501;state=working:app=agent-x\\033\\\\\"; sleep 600'\r");
    std::thread::sleep(Duration::from_millis(800));
    h.typ("\x02x");
    let s = h.wait_for("Close Pane", 5);
    assert!(s.iter().any(|l| l.contains("⚠ agent-x is working")), "{}", s.join("\n"));
    assert!(bar(&h).contains(" CONFIRM "), "{:?}", bar(&h));
    h.typ("n");
    until(
        &|| !h.screen().iter().any(|l| l.contains("Close Pane")),
        "closed the question",
        &h,
    );
    std::thread::sleep(Duration::from_millis(500));
    assert_eq!(live(&core).len(), 2, "kept");
    h.typ("\x02x");
    h.wait_for("Close Pane", 5);
    h.typ("y");
    count(1);

    // A workspace always asks.
    h.typ("\x02:kill-workspace\r");
    let s = h.wait_for("Close Workspace", 5);
    assert!(s.iter().any(|l| l.contains("1 window, 1 pane")), "{}", s.join("\n"));
    pause();
    h.typ("\x1b");
    until(
        &|| !h.screen().iter().any(|l| l.contains("Close Workspace")),
        "cancelled",
        &h,
    );
    std::thread::sleep(Duration::from_millis(300));
    assert_eq!(live(&core).len(), 1);
    h.typ("\x02d");
    assert_eq!(h.wait_exit(5), 0);
}

#[test]
fn new_workspace_from_the_chooser() {
    let Some(core) = Core::start("cw") else { return };
    core.config("[panes]\ntitle = \"off\"\n");
    let mut h = Corgi::spawn(&core);
    h.wait_for("Workspaces", 10);
    h.wait_for("$", 10);
    // C-b w: a big chooser, with a way to make a workspace at the bottom.
    h.typ("\x02w");
    let s = h.wait_for("+ New workspace…", 5);
    let top = s.iter().position(|l| l.contains(" choose ")).unwrap();
    let bottom = (top..s.len()).rev().find(|&y| s[y].contains("Esc close")).unwrap();
    assert!(bottom - top >= (ROWS as usize) * 3 / 4 - 2, "tall: {top}..{bottom}");
    // A filter that matches nothing: Enter opens New Workspace named after it.
    h.typ("scratch");
    h.wait_for("+ New workspace “scratch”…", 5);
    h.typ("\r");
    let s = h.wait_for("New Workspace", 5);
    assert!(
        s.iter().any(|l| l.contains("Name") && l.contains("scratch")),
        "{}",
        s.join("\n")
    );
    std::thread::sleep(Duration::from_millis(250));
    h.typ("\r");
    let end = Instant::now() + Duration::from_secs(10);
    while live(&core).len() != 2 {
        assert!(Instant::now() < end, "{} terminals", live(&core).len());
        std::thread::sleep(Duration::from_millis(50));
    }
    // In the sidebar (the dialog said it too: wait for the sidebar's).
    let end = Instant::now() + Duration::from_secs(10);
    while !h
        .screen()
        .iter()
        .any(|l| l.chars().take(26).collect::<String>().contains("scratch"))
    {
        assert!(Instant::now() < end, "in the sidebar:\n{}", h.screen().join("\n"));
        std::thread::sleep(Duration::from_millis(50));
    }
    h.typ("\x02d");
    assert_eq!(h.wait_exit(5), 0);
}

#[test]
fn new_workspace_in_a_git_worktree() {
    let Some(core) = Core::start("wt") else { return };
    if Command::new("git").arg("--version").output().is_err() {
        return;
    }
    let repo = core.dir.join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    for args in [
        &["init", "-q"][..],
        &[
            "-c",
            "user.email=h@example.invalid",
            "-c",
            "user.name=h",
            "commit",
            "-q",
            "--allow-empty",
            "-m",
            "x",
        ],
    ] {
        assert!(
            Command::new("git")
                .current_dir(&repo)
                .args(args)
                .status()
                .unwrap()
                .success()
        );
    }
    let trees = core.dir.join("trees");
    core.config(&format!(
        "[panes]\ntitle = \"off\"\n[worktrees]\ndirectory = \"{}\"\n",
        trees.display()
    ));
    let mut h = Corgi::spawn(&core);
    h.wait_for("Workspaces", 10);
    h.wait_for("$", 10);
    let pause = || std::thread::sleep(Duration::from_millis(250));
    let open = |h: &mut Corgi, folder: &str| {
        h.typ("\x02N");
        h.wait_for("New Workspace", 5);
        pause();
        h.typ(&format!("\x15{folder}"));
        pause();
        // Folder, Name, Machine, Program, then Worktree: switch it on; Branch appears.
        for _ in 0..4 {
            h.typ("\x1b[B");
            pause();
        }
        h.typ(" ");
        h.wait_for("Branch", 5);
        h.typ("\x1b[B");
        pause();
    };

    // Not a repository: git's reason, and the dialog stays. (Outside the checkout corgi's tests
    // run in, which is a repository itself.)
    let plain = std::env::temp_dir().join(format!("corgi-not-a-repo-{}", std::process::id()));
    std::fs::create_dir_all(&plain).unwrap();
    open(&mut h, &plain.display().to_string());
    h.typ("feat/x\r");
    h.wait_for("not a git repository", 10);
    h.typ("\x1b");
    std::thread::sleep(Duration::from_millis(300));

    // The repository: a worktree on a new branch, where the config says; the workspace opens in it.
    open(&mut h, &repo.display().to_string());
    h.typ("feat/x\r");
    let end = Instant::now() + Duration::from_secs(15);
    while live(&core).len() != 2 {
        assert!(Instant::now() < end, "no workspace:\n{}", h.screen().join("\n"));
        std::thread::sleep(Duration::from_millis(50));
    }
    assert!(trees.join("repo/feat-x/.git").exists(), "the worktree");
    h.wait_for("$", 10);
    h.typ("git branch --show-current; pwd\r");
    h.wait_for("trees/repo/feat-x", 10);
    h.wait_for("feat/x", 5);
    let _ = std::fs::remove_dir_all(&plain);
    h.typ("\x02d");
    assert_eq!(h.wait_exit(5), 0);
}

#[test]
fn action_menus_from_the_keyboard() {
    let Some(core) = Core::start("mn") else { return };
    core.config("[panes]\ntitle = \"off\"\n");
    let mut h = Corgi::spawn(&core);
    h.wait_for("Workspaces", 10);
    h.wait_for("$", 10);
    let bar = |h: &Corgi| h.screen()[ROWS as usize - 1].clone();
    let pause = || std::thread::sleep(Duration::from_millis(250));
    let count = |n: usize| {
        let end = Instant::now() + Duration::from_secs(10);
        while live(&core).len() != n {
            assert!(Instant::now() < end, "{} terminals, want {n}", live(&core).len());
            std::thread::sleep(Duration::from_millis(50));
        }
    };
    let gone = |h: &Corgi, what: &str| {
        let end = Instant::now() + Duration::from_secs(5);
        while h.screen().iter().any(|l| l.contains(what)) {
            assert!(Instant::now() < end, "{what} still up:\n{}", h.screen().join("\n"));
            std::thread::sleep(Duration::from_millis(50));
        }
    };

    // C-b m: everything for here, by section; an entry's key runs it.
    h.typ("\x02m");
    let s = h.wait_for("new worktree workspace", 5);
    let all = s.join("\n");
    for want in [
        "Pane",
        "%  split right",
        "Window",
        ",  rename window…",
        "W  new worktree workspace…",
    ] {
        assert!(all.contains(want), "{want}:\n{all}");
    }
    assert!(bar(&h).contains(" MENU "), "{:?}", bar(&h));
    h.typ("%");
    count(2);
    gone(&h, "split right");

    // j/k and Enter: down to "split down", the second entry.
    h.typ("\x02m");
    h.wait_for("split right", 5);
    pause();
    h.typ("j\r");
    count(3);

    // Esc does nothing.
    h.typ("\x02m");
    h.wait_for("split right", 5);
    pause();
    h.typ("\x1b");
    gone(&h, "split right");
    std::thread::sleep(Duration::from_millis(300));
    assert_eq!(live(&core).len(), 3);

    // In the sidebar, m opens the menu for the row under the cursor (the window: no Pane section).
    // C-b h twice: from the bottom-right pane to the left one, then into the sidebar.
    h.typ("\x02h");
    pause();
    h.typ("\x02h");
    h.wait_for(" SIDEBAR ", 5);
    pause();
    h.typ("m");
    let s = h.wait_for("rename window…", 5);
    assert!(
        !s.iter().any(|l| l.contains("split right")),
        "a window's menu:\n{}",
        s.join("\n")
    );
    h.typ("c");
    count(4);
    gone(&h, "rename window…");

    // Right-click on a pane: its menu too.
    h.typ("\x1b[<2;60;10M\x1b[<2;60;10m");
    h.wait_for("split right", 5);
    h.typ("\x1b");
    gone(&h, "split right");
    h.typ("\x02d");
    assert_eq!(h.wait_exit(5), 0);
}

#[test]
fn usage_in_the_tab_bar_and_its_details() {
    let Some(core) = Core::start("us") else { return };
    // Claude Code's log for today, as it writes it (a message's usage on each of its parts).
    let logs = core.dir.join("claude/projects/-p");
    std::fs::create_dir_all(&logs).unwrap();
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    // A minute ago, as RFC 3339 UTC.
    let ts = {
        let t = (now - 60) as i64;
        let (z, secs) = (t.div_euclid(86400) + 719_468, t.rem_euclid(86400));
        let era = z.div_euclid(146_097);
        let doe = z - era * 146_097;
        let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
        let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
        let mp = (5 * doy + 2) / 153;
        let (d, m) = (doy - (153 * mp + 2) / 5 + 1, if mp < 10 { mp + 3 } else { mp - 9 });
        let y = yoe + era * 400 + i64::from(m <= 2);
        format!(
            "{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}.000Z",
            secs / 3600,
            secs % 3600 / 60,
            secs % 60
        )
    };
    let line = format!(
        "{{\"type\":\"assistant\",\"requestId\":\"r1\",\"timestamp\":\"{ts}\",\"message\":{{\"id\":\"m1\",\"usage\":{{\"input_tokens\":1000,\"output_tokens\":200,\"cache_creation_input_tokens\":0,\"cache_read_input_tokens\":0}}}}}}\n"
    );
    std::fs::write(logs.join("s.jsonl"), line.repeat(2)).unwrap();
    core.config(
        "[sidebar]\nmode = \"hidden\"\n\
         [[usage.segment]]\nprovider = \"claude\"\npriority = 60\nlimit = 2000\n\
         [[usage.segment]]\nprovider = \"command\"\ncmd = \"echo build-ok\"\npriority = 40\nbg = \"#123456\"\n",
    );
    let mut h = Corgi::spawn(&core);
    h.wait_for("$", 10);
    let bar = |h: &Corgi| h.screen()[ROWS as usize - 1].clone();
    let end = Instant::now() + Duration::from_secs(10);
    // 1,200 of a 2,000-token block budget used: 40% left.
    while !(bar(&h).contains("claude 40% left") && bar(&h).contains("build-ok")) {
        assert!(Instant::now() < end, "usage in the tab bar: {:?}", bar(&h));
        std::thread::sleep(Duration::from_millis(50));
    }
    // After the tabs, at the right edge (no core version there now), each a block in its colours:
    // Claude's coral by default, the command's as configured.
    let b = bar(&h);
    assert!(
        b.find("0:").unwrap() < b.find("claude").unwrap() && b.ends_with("build-ok"),
        "{b:?}"
    );
    assert!(!b.contains("core "), "{b:?}");
    let row = ROWS - 1;
    let col = |what: &str| b[..b.find(what).unwrap()].chars().count() as u16;
    assert_eq!(h.bg(col("claude"), row), Some(corgi_vt::Rgb(0xd9, 0x77, 0x57)));
    assert_eq!(h.bg(col("build-ok"), row), Some(corgi_vt::Rgb(0x12, 0x34, 0x56)));
    assert_eq!(
        h.bg(col("build-ok") - 1, row),
        Some(corgi_vt::Rgb(0x12, 0x34, 0x56)),
        "padded"
    );
    // C-b u: the details, by segment.
    h.typ("\x02u");
    let s = h.wait_for("Usage", 5);
    let all = s.join("\n");
    assert!(all.contains("claude: claude 40% left"), "{all}");
    let block = s.iter().find(|l| l.contains("5-hour block")).expect(&all);
    assert!(
        block.contains("████████") && block.contains("░") && block.contains("40% left"),
        "a bar: {block}"
    );
    assert!(all.contains("1.2k of 2k tokens (limit: set in the config)"), "{all}");
    assert!(all.contains("1.2k tokens over 1 messages"), "counted once: {all}");
    assert!(all.contains("command: build-ok"), "{all}");
    assert!(
        all.contains("connected to core 0."),
        "the version lives here now: {all}"
    );
    assert!(bar(&h).contains(" USAGE "), "{:?}", bar(&h));
    h.typ("\x1b");
    std::thread::sleep(Duration::from_millis(300));
    h.typ("\x02d");
    assert_eq!(h.wait_exit(5), 0);
}

#[test]
fn claude_usage_from_its_status_line() {
    let Some(core) = Core::start("cs") else { return };
    core.config("[sidebar]\nmode = \"hidden\"\n[[usage.segment]]\nprovider = \"claude\"\n");
    // As Claude Code runs its status line: JSON on stdin; corgi keeps the rate limits and runs
    // the real status line with the same input.
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let input = format!(
        r#"{{"model":{{"id":"claude-opus"}},"rate_limits":{{"five_hour":{{"used_percentage":30,"resets_at":{}}},"seven_day":{{"used_percentage":64,"resets_at":{}}}}}}}"#,
        now + 3600,
        now + 86400
    );
    let mut tap = Command::new(env!("CARGO_BIN_EXE_corgi"))
        .args([
            "claude-statusline",
            "--",
            "sh",
            "-c",
            "grep -o claude-opus; echo status-line-ran",
        ])
        .envs(core.env())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    tap.stdin.take().unwrap().write_all(input.as_bytes()).unwrap();
    let out = tap.wait_with_output().unwrap();
    assert!(out.status.success());
    assert_eq!(
        String::from_utf8_lossy(&out.stdout),
        "claude-opus\nstatus-line-ran\n",
        "same input, its output"
    );

    // corgi's claude segment: Claude Code's numbers, the tightest window (the week, 36% left).
    let mut h = Corgi::spawn(&core);
    h.wait_for("$", 10);
    let end = Instant::now() + Duration::from_secs(10);
    while !h.screen()[ROWS as usize - 1].contains("claude 36% left of week") {
        assert!(Instant::now() < end, "{:?}", h.screen()[ROWS as usize - 1]);
        std::thread::sleep(Duration::from_millis(50));
    }
    h.typ("\x02u");
    let s = h.wait_for("from Claude Code's status line", 5);
    let all = s.join("\n");
    assert!(
        s.iter().any(|l| l.contains("5h limit") && l.contains("70% left")),
        "{all}"
    );
    assert!(
        s.iter().any(|l| l.contains("week limit") && l.contains("36% left")),
        "{all}"
    );
    h.typ("\x1b");
    std::thread::sleep(Duration::from_millis(300));
    h.typ("\x02d");
    assert_eq!(h.wait_exit(5), 0);
}

#[test]
fn pane_title_lines_with_an_activity_circle() {
    let Some(core) = Core::start("lt") else { return };
    core.config("[sidebar]\nmode = \"hidden\"\n[panes]\ntitle_shape = \"line\"\n");
    let mut h = Corgi::spawn(&core);
    h.wait_for("NORMAL", 10);
    h.wait_for("$", 10);
    let top = |h: &Corgi| h.screen()[0].clone();
    let until = |ok: &dyn Fn(&str) -> bool, why: &str, h: &Corgi| {
        let end = Instant::now() + Duration::from_secs(10);
        loop {
            let t = top(h);
            if ok(&t) {
                return t;
            }
            assert!(Instant::now() < end, "{why}: {t:?}");
            std::thread::sleep(Duration::from_millis(40));
        }
    };
    // The title as text on the box's top edge; a shell at its prompt has no circle.
    let t = until(&|t| t.starts_with("┌─ lt ─"), "the title", &h);
    assert!(t.ends_with("──┐"), "no circle at the prompt: {t:?}");
    // A command running: a spinner.
    let spin = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
    h.typ("sleep 3\r");
    until(&|t| spin.iter().any(|c| t.ends_with(&format!(" {c} ┐"))), "running", &h);
    h.typ("\x03");
    until(&|t| t.ends_with("──┐"), "back at the prompt", &h);
    // A working agent: its circle spins.
    h.typ("sh -c 'printf \"\\033]7501;state=working:app=agent-l\\033\\\\\"; sleep 3; printf \"\\033]7501;state=blocked:app=agent-l\\033\\\\\"; sleep 600'\r");
    let first = until(
        &|t| spin.iter().any(|c| t.ends_with(&format!(" {c} ┐"))),
        "spinning",
        &h,
    );
    until(
        &|t| spin.iter().any(|c| t.ends_with(&format!(" {c} ┐"))) && t != first,
        "it turns",
        &h,
    );
    // Waiting for you: ●.
    until(&|t| t.ends_with(" ● ┐"), "blocked", &h);
    h.typ("\x03");
    h.typ("\x02d");
    assert_eq!(h.wait_exit(5), 0);
}

#[test]
fn breadcrumbs_on_their_own_row() {
    let Some(core) = Core::start("bc") else { return };
    core.config("[sidebar]\nmode = \"hidden\"\n");
    let mut h = Corgi::spawn(&core);
    h.wait_for("NORMAL", 10);
    h.wait_for("$", 10);
    let row = |h: &Corgi, y: usize| h.screen()[y].clone();
    let until = |y: usize, ok: &dyn Fn(&str) -> bool, why: &str, h: &Corgi| {
        let end = Instant::now() + Duration::from_secs(10);
        loop {
            let t = row(h, y);
            if ok(&t) {
                return t;
            }
            assert!(Instant::now() < end, "{why}: {:?}", &h.screen()[..4]);
            std::thread::sleep(Duration::from_millis(40));
        }
    };
    // The default: under the box's top edge (left alone), the folder's path as crumbs.
    until(1, &|t| t.contains(" › h › bc"), "the path", &h);
    assert!(
        row(&h, 0).starts_with("┌──") && !row(&h, 0).contains("bc"),
        "the edge is just an edge: {:?}",
        row(&h, 0)
    );
    // In line with the terminal's text (one column in, panes.inset).
    assert!(row(&h, 1).starts_with("│  ~ › "), "{:?}", row(&h, 1));
    // Its own box: a line under it, joined to the pane's border; the shell starts below that.
    let line = row(&h, 2);
    assert!(line.starts_with("├──") && line.ends_with("──┤"), "{line:?}");
    assert!(row(&h, 3).contains('$'), "{:?}", &h.screen()[..4]);
    // A command running, then an agent working: the circle at the row's end, and the agent's crumbs.
    let spin = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
    h.typ("sleep 3\r");
    until(
        1,
        &|t| spin.iter().any(|c| t.ends_with(&format!(" {c}  │"))),
        "running: a spinner",
        &h,
    );
    h.typ("\x03");
    h.typ("sh -c 'printf \"\\033]7501;state=working:app=agent-b\\033\\\\\"; sleep 2; printf \"\\033]7501;state=blocked:app=agent-b\\033\\\\\"; sleep 600'\r");
    let t = until(
        1,
        &|t| t.contains("agent-b") && spin.iter().any(|c| t.ends_with(&format!(" {c}  │"))),
        "an agent working: spinning",
        &h,
    );
    assert!(t.contains("bc › agent-b"), "folder › agent: {t:?}");
    // Waiting for you: a solid circle.
    until(1, &|t| t.ends_with(" ●  │"), "blocked", &h);
    h.typ("\x03");
    // A program's background fills the inset too, to the border.
    h.wait_for("$", 5);
    h.typ("printf '\\033[48;2;18;52;86m\\033[2J\\033[H'; echo BG-DONE\r");
    h.wait_for("BG-DONE", 10);
    let want = Some(corgi_vt::Rgb(18, 52, 86));
    let end = Instant::now() + Duration::from_secs(5);
    while h.bg(1, 10) != want || h.bg(COLS - 2, 10) != want {
        assert!(
            Instant::now() < end,
            "inset cells: {:?} {:?}",
            h.bg(1, 10),
            h.bg(COLS - 2, 10)
        );
        std::thread::sleep(Duration::from_millis(50));
    }
    h.typ("\x02d");
    assert_eq!(h.wait_exit(5), 0);
}

#[test]
fn copy_mode_search_select_copy_paste() {
    let Some(core) = Core::start("cp") else { return };
    core.config("[sidebar]\nmode = \"hidden\"\n");
    let mut h = Corgi::spawn(&core);
    h.wait_for("NORMAL", 10);
    h.wait_for("$", 10);
    let bar = |h: &Corgi| h.screen()[ROWS as usize - 1].clone();
    let pause = || std::thread::sleep(Duration::from_millis(250));
    h.typ("for i in $(seq 1 60); do echo row-$i; done\r");
    h.wait_for("row-60", 10);
    pause();

    // C-b [: copy mode; the badge and where you are say so.
    h.typ("\x02[");
    let end = Instant::now() + Duration::from_secs(5);
    while !bar(&h).contains(" COPY ") {
        assert!(Instant::now() < end, "{:?}", bar(&h));
        std::thread::sleep(Duration::from_millis(50));
    }
    assert!(
        h.screen()
            .iter()
            .any(|l| l.contains("/") && l.contains('[') && l.contains(']')),
        "the position"
    );
    // Up to row-12 (scrolled into view), select to its end, copy.
    h.typ("?row-12");
    pause();
    h.typ("\r");
    pause();
    assert!(
        h.screen().iter().any(|l| l.contains("row-12")),
        "{}",
        h.screen().join("\n")
    );
    h.typ("v$");
    pause();
    h.typ("y");
    h.wait_for("copied 6 characters", 5);
    // To the host's clipboard (OSC 52), and back at the bottom.
    assert!(String::from_utf8_lossy(&h.out.lock().unwrap()).contains("\x1b]52;c;cm93LTEy\x07"));
    assert!(bar(&h).contains(" NORMAL "), "{:?}", bar(&h));
    h.wait_for("row-60", 5);

    // C-b ]: paste it.
    h.typ("echo pasted-");
    pause();
    h.typ("\x02]");
    pause();
    h.typ("\r");
    h.wait_for("pasted-row-12", 10);

    // C-b PageUp: copy mode a screen up at once; q leaves.
    h.typ("\x02\x1b[5~");
    let end = Instant::now() + Duration::from_secs(5);
    while !bar(&h).contains(" COPY ") {
        assert!(Instant::now() < end, "{:?}", bar(&h));
        std::thread::sleep(Duration::from_millis(50));
    }
    assert!(
        !h.screen()
            .iter()
            .any(|l| l.contains("pasted-row-12") && !l.contains("echo")),
        "scrolled up from the end"
    );
    h.typ("q");
    let end = Instant::now() + Duration::from_secs(5);
    while !bar(&h).contains(" NORMAL ") {
        assert!(Instant::now() < end, "{:?}", bar(&h));
        std::thread::sleep(Duration::from_millis(50));
    }
    h.typ("\x02d");
    assert_eq!(h.wait_exit(5), 0);
}

#[test]
fn popups_run_shell_and_the_scrollback_in_an_editor() {
    let Some(core) = Core::start("po") else { return };
    core.config("[sidebar]\nmode = \"hidden\"\n");
    // An "editor" that shows the file's last line, then waits for Enter.
    let mut h = Corgi::spawn_env(
        &core,
        &[("EDITOR", "sh -c 'echo \"editing: $(tail -1 \"$2\")\"; read x' sh")],
    );
    h.wait_for("NORMAL", 10);
    h.wait_for("$", 10);
    let bar = |h: &Corgi| h.screen()[ROWS as usize - 1].clone();
    let pause = || std::thread::sleep(Duration::from_millis(300));
    let badge = |h: &Corgi, want: &str| {
        let end = Instant::now() + Duration::from_secs(10);
        while !bar(h).contains(want) {
            assert!(Instant::now() < end, "{want}: {:?}\n{}", bar(h), h.screen().join("\n"));
            std::thread::sleep(Duration::from_millis(50));
        }
    };
    let gone = |h: &Corgi, what: &str| {
        let end = Instant::now() + Duration::from_secs(10);
        while h.screen().iter().any(|l| l.contains(what)) {
            assert!(Instant::now() < end, "{what} still there:\n{}", h.screen().join("\n"));
            std::thread::sleep(Duration::from_millis(50));
        }
    };

    // -E: it goes when its program ends.
    h.typ("\x02:display-popup -E -w 60% -h 10 -T try sh -c 'echo popped-$((1+1)); read x'\r");
    h.wait_for("popped-2", 10);
    assert!(h.screen().iter().any(|l| l.contains(" try ")), "its title");
    badge(&h, " POPUP ");
    pause();
    h.typ("\r");
    gone(&h, "popped-2");
    badge(&h, " NORMAL ");

    // Without -E: it says it ended; a key closes it.
    h.typ("\x02:display-popup -h 8 sh -c 'echo said-it'\r");
    h.wait_for("exited (0)", 10);
    h.typ("q");
    gone(&h, "said-it");

    // C-b x closes one still running; its terminal goes too.
    h.typ("\x02:display-popup sleep 100\r");
    badge(&h, " POPUP ");
    pause();
    h.typ("\x02x");
    badge(&h, " NORMAL ");
    let end = Instant::now() + Duration::from_secs(10);
    while live(&core).len() != 1 {
        assert!(Instant::now() < end, "{} terminals", live(&core).len());
        std::thread::sleep(Duration::from_millis(50));
    }

    // run-shell: in the background; what it printed, in the status line.
    h.typ("\x02:run-shell echo bg-$((2+3))\r");
    h.wait_for("bg-5", 10);

    // C-b e: the scrollback in $EDITOR, in a popup.
    h.typ("echo marker-$((70+7))\r");
    h.wait_for("marker-77", 10);
    pause();
    h.typ("\x02e");
    h.wait_for("editing: ", 10);
    let s = h.screen();
    assert!(
        s.iter().any(|l| l.contains("editing: ") && l.contains("$")),
        "its last line: the prompt\n{}",
        s.join("\n")
    );
    pause();
    h.typ("\r");
    gone(&h, "editing: ");
    h.typ("\x02d");
    assert_eq!(h.wait_exit(5), 0);
}

#[test]
fn worktrees_listed_opened_grouped_and_removed() {
    let Some(core) = Core::start("wl") else { return };
    if Command::new("git").arg("--version").output().is_err() {
        return;
    }
    // A repository of its own (never corgi's checkout), and a worktree of it with a change.
    let repo = core.dir.join("repo");
    let tree = core.dir.join("trees/feat");
    std::fs::create_dir_all(&repo).unwrap();
    let git = |args: &[&str]| {
        assert!(
            Command::new("git")
                .current_dir(&repo)
                .args(args)
                .output()
                .unwrap()
                .status
                .success(),
            "{args:?}"
        )
    };
    git(&["init", "-q", "-b", "main"]);
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
    git(&["worktree", "add", "-q", "-b", "feat", tree.to_str().unwrap()]);
    std::fs::write(tree.join("wip.txt"), "x").unwrap();
    core.config("[panes]\ntitle = \"off\"\n");
    let mut h = Corgi::spawn_at(&core, &repo, &[]);
    h.wait_for("Workspaces", 10);
    h.wait_for("$", 10);
    let pause = || std::thread::sleep(Duration::from_millis(300));
    let sidebar = |h: &Corgi| {
        h.screen()
            .iter()
            .map(|l| l.chars().take(26).collect::<String>())
            .collect::<Vec<_>>()
            .join("\n")
    };

    // C-b W: the repository's worktrees; the main one first, the other with its change.
    h.typ("\x02W");
    let s = h.wait_for("worktrees of repo", 10);
    let all = s.join("\n");
    assert!(
        s.iter().any(|l| l.contains("● main") && l.contains("main")),
        "the open, main checkout:\n{all}"
    );
    assert!(s.iter().any(|l| l.contains("feat") && l.contains("changes")), "{all}");
    // Open feat: a workspace, listed under the repository's.
    pause();
    h.typ("j\r");
    let end = Instant::now() + Duration::from_secs(15);
    while live(&core).len() != 2 || !sidebar(&h).contains("↳") {
        assert!(
            Instant::now() < end,
            "{} terminals:\n{}",
            live(&core).len(),
            sidebar(&h)
        );
        std::thread::sleep(Duration::from_millis(100));
    }
    let sb = sidebar(&h);
    let (r, f) = (sb.find("repo").unwrap(), sb.find("↳ ").unwrap());
    assert!(r < f && sb[f..].starts_with("↳ feat"), "under it:\n{sb}");

    // Remove it: asked, with what would be lost; its workspace goes, then the worktree.
    h.typ("\x02W");
    h.wait_for("worktrees of repo", 10);
    pause();
    h.typ("j");
    pause();
    h.typ("d");
    let s = h.wait_for("Remove Worktree", 5);
    let all = s.join("\n");
    assert!(
        all.contains("uncommitted changes") && all.contains("its workspace closes too"),
        "{all}"
    );
    h.typ("y");
    h.wait_for("removed the worktree", 10);
    let end = Instant::now() + Duration::from_secs(10);
    while live(&core).len() != 1 || tree.exists() {
        assert!(
            Instant::now() < end,
            "{} terminals; {:?} there: {}",
            live(&core).len(),
            tree,
            tree.exists()
        );
        std::thread::sleep(Duration::from_millis(100));
    }
    // The main checkout can't go.
    h.typ("\x02W");
    h.wait_for("worktrees of repo", 10);
    pause();
    h.typ("d");
    h.wait_for("can't be removed", 5);
    h.typ("\x1b");
    pause();
    h.typ("\x02d");
    assert_eq!(h.wait_exit(5), 0);
}

#[test]
fn workspace_names_follow_its_first_pane() {
    let Some(core) = Core::start("fl") else { return };
    if Command::new("git").arg("--version").output().is_err() {
        return;
    }
    let repo = core.dir.join("proj");
    std::fs::create_dir_all(repo.join("src/deep")).unwrap();
    std::fs::create_dir_all(core.dir.join("notes")).unwrap();
    assert!(
        Command::new("git")
            .current_dir(&repo)
            .args(["init", "-q", "-b", "main"])
            .status()
            .unwrap()
            .success()
    );
    core.config("[panes]\ntitle = \"off\"\n");
    let mut h = Corgi::spawn(&core);
    h.wait_for("Workspaces", 10);
    h.wait_for("$", 10);
    // Under the sidebar's header row and its line.
    let row = |h: &Corgi| h.screen()[3].chars().take(26).collect::<String>();
    let until = |want: &str, h: &Corgi| {
        let end = Instant::now() + Duration::from_secs(15);
        while !row(h).contains(want) {
            assert!(Instant::now() < end, "workspace row {:?}, want {want:?}", row(h));
            std::thread::sleep(Duration::from_millis(100));
        }
    };
    until("▾ fl", &h);
    // Another folder: named after it.
    h.typ("cd notes\r");
    until("▾ notes", &h);
    // Deep in a git checkout: named after the checkout, with its branch.
    h.typ("cd ../proj/src/deep\r");
    until("▾ proj main", &h);
    // A name of its own stays, wherever it goes.
    h.typ("\x02$");
    h.wait_for("Rename Workspace", 5);
    std::thread::sleep(Duration::from_millis(250));
    h.typ("\x15fixed\r");
    until("▾ fixed", &h);
    h.typ("cd /\r");
    std::thread::sleep(Duration::from_millis(1500));
    assert!(row(&h).contains("▾ fixed"), "{:?}", row(&h));
    h.typ("\x02d");
    assert_eq!(h.wait_exit(5), 0);
}

/// Another machine: a second core session on this one, reached with `--via` a script that
/// drops its line while a `down` file exists (no ssh needed). Machines head the sidebar; a
/// workspace on it works; when its line drops, its pane shows why (and what it last showed),
/// and once it's back (Enter in the Machines dialog dials now) the pane works again.
#[test]
fn a_machine_reached_by_a_command() {
    let Some(core) = Core::start("mc") else { return };
    // The other end: its own core session in this test's folders (stopped when this drops).
    let other = Core {
        dir: core.dir.clone(),
        id: "mcx".into(),
    };
    // The line's command runs this line in the background with the connection as its input. The
    // input is kept on fd 3 first: dash (Linux's sh) points a background command's input at
    // /dev/null before its own redirections, so `<&0` would copy /dev/null. The command is run with
    // `exec` so `$!` is `herdr-core connect` itself: dash forks for a lone `sh -c` command where
    // bash execs it, and killing only that outer shell would leave the line up.
    let via = core.dir.join("via.sh");
    std::fs::write(
        &via,
        "D=$(dirname \"$0\")\n\
         [ -e \"$D/down\" ] && { echo 'line down' >&2; exit 1; }\n\
         exec 3<&0\n\
         sh -c \"exec $1\" <&3 3<&- &\n\
         c=$!\n\
         exec 3<&-\n\
         while [ ! -e \"$D/down\" ] && kill -0 $c 2>/dev/null; do sleep 0.2; done\n\
         kill $c 2>/dev/null\n\
         exit 1\n",
    )
    .unwrap();
    let add = Command::new(hc_bin())
        .args(["machine", "add", "--json", "--via"])
        .arg(format!("sh {}", via.display()))
        .args(["--name", "loop", "--session", "emcx"])
        .current_dir(&core.dir)
        .envs(core.env())
        .env("SHELL", core.shell())
        .output()
        .unwrap();
    let added: serde_json::Value = serde_json::from_slice(&add.stdout).unwrap_or(serde_json::Value::Null);
    let added_err = String::from_utf8_lossy(&add.stderr).to_string();
    if added["name"] != "loop" {
        // Say why: hc's own words, and what the line's script and the other core said.
        let out = Command::new(hc_bin())
            .args(["machine", "ls", "--json"])
            .envs(core.env())
            .output()
            .unwrap();
        let log = |p: &str| std::fs::read_to_string(core.dir.join(p)).unwrap_or_default();
        panic!(
            "machine add: {added} {added_err} (stdout {:?})\nmachine ls: {}\n{}\ncore log: {}\nother core log: {}",
            String::from_utf8_lossy(&add.stdout),
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr),
            log("s/herdr-core/emc/core.log"),
            log("s/herdr-core/emcx/core.log"),
        );
    }
    // Dropped first (even on a failure): forget it, or this core dials it again and so restarts
    // the other session after that's stopped.
    let _forget = Forget(&core, "loop");
    // A repository for the workspace there (its branch is read there, through core), and a
    // segment that says which core session it runs in: this one's, or the other's.
    let repo = core.dir.join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    let git_ok = Command::new("git")
        .args(["init", "-q", "-b", "trunk"])
        .current_dir(&repo)
        .status()
        .is_ok_and(|s| s.success());
    let png = core.dir.join("clip.png");
    std::fs::write(&png, PNG).unwrap();
    core.config(&format!(
        "[paste]\nimage_command = \"cat {}\"\n\
         [[usage.segment]]\nprovider = \"command\"\ncmd = 'echo \"[$HC_SESSION]\"'\non = \"workspace\"\ninterval = \"1s\"\n",
        png.display()
    ));
    let mut h = Corgi::spawn(&core);
    h.wait_for("Workspaces", 10);
    h.wait_for("$", 10);
    h.wait_for("[emc]", 10);
    let pause = || std::thread::sleep(Duration::from_millis(250));

    // The Machines dialog lists it, connected.
    h.typ("\x02M");
    let s = h.wait_for(" machines ", 10);
    assert!(s.iter().any(|l| l.contains("● loop") && l.contains("via sh")), "{s:#?}");
    h.typ("\x1b");
    pause();

    // A workspace on it, in this test's folder (the same disk, seen from the other session).
    h.typ("\x02:machine new-workspace loop\r");
    h.wait_for("New Workspace", 5);
    pause();
    h.typ(&format!("\x15{}\r", repo.display()));
    let s = h.wait_for("● loop", 15);
    assert!(
        s.iter().any(|l| l.contains("⌂")),
        "this machine heads its own group: {s:#?}"
    );
    pause();
    h.typ("echo remote-$((6*7))\r");
    h.wait_for("remote-42", 10);
    // The segment follows the workspace there; its branch comes from there.
    h.wait_for("[emcx]", 10);
    if git_ok {
        h.wait_for("repo trunk", 15);
    }
    // An image pasted there (the clipboard holds only one: the paste is empty) lands beside
    // its terminal, in the other session, and its path is pasted.
    h.typ("cmp ");
    pause();
    h.typ("\x1b[200~\x1b[201~");
    h.wait_for("pasted an image", 10);
    h.typ(&format!(" {} && echo same-$((3*3))\r", png.display()));
    h.wait_for("same-9", 10);
    assert!(
        other.hc(&["ls", "--json"])["terminals"]
            .as_array()
            .is_some_and(|t| !t.is_empty()),
        "the shell runs in the other session"
    );

    // Its line drops: the pane says so, and keeps what it showed.
    std::fs::write(core.dir.join("down"), "").unwrap();
    let end = Instant::now() + Duration::from_secs(20);
    while !h.screen().iter().any(|l| l.contains("loop unreachable")) {
        if Instant::now() > end {
            // Say why: the line's state as core has it, and what still runs of it.
            let ls = Command::new(hc_bin())
                .args(["machine", "ls", "--json"])
                .envs(core.env())
                .output()
                .unwrap();
            let ps = Command::new("ps").args(["-eo", "pid,ppid,stat,args"]).output().unwrap();
            let ps: Vec<String> = String::from_utf8_lossy(&ps.stdout)
                .lines()
                .filter(|l| l.contains("via.sh") || l.contains("connect") || l.contains("emcx"))
                .map(str::to_string)
                .collect();
            panic!(
                "never unreachable\nmachine ls: {}\nprocesses:\n{}\nscreen:\n{}",
                String::from_utf8_lossy(&ls.stdout),
                ps.join("\n"),
                h.screen().join("\n")
            );
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    let s = h.screen();
    assert!(s.iter().any(|l| l.contains("remote-42")), "{s:#?}");
    assert!(
        s.iter()
            .any(|l| l.contains("loop unreachable for") && l.contains("its last screen")),
        "{s:#?}"
    );

    // Back: dial it now from the Machines dialog; the pane works again.
    std::fs::remove_file(core.dir.join("down")).unwrap();
    h.typ("\x02M");
    h.wait_for(" machines ", 5);
    pause();
    h.typ("\r");
    h.wait_for("loop is connected", 20);
    h.typ("\x1b");
    pause();
    let end = Instant::now() + Duration::from_secs(20);
    while h.screen().iter().any(|l| l.contains("loop unreachable")) {
        assert!(Instant::now() < end, "{:#?}", h.screen());
        std::thread::sleep(Duration::from_millis(100));
    }
    h.typ("echo back-$((2*3))\r");
    h.wait_for("back-6", 10);
    h.typ("\x02d");
    assert_eq!(h.wait_exit(5), 0);
}

/// Forgets a machine when dropped.
struct Forget<'a>(&'a Core, &'a str);

impl Drop for Forget<'_> {
    fn drop(&mut self) {
        self.0.hc(&["machine", "rm", self.1]);
    }
}

/// A tiny PNG's first bytes: enough for corgi to see it's one.
const PNG: &[u8] = b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR\0\0\0\x01\0\0\0\x01";

/// Images into panes (here for every pane, `images = "always"`): an empty paste takes the
/// clipboard's image, a dropped image file is sent as itself, `C-b C-v` asks for it; each lands
/// beside the terminal and its path is pasted.
#[test]
fn images_are_put_beside_the_terminal_and_their_path_pasted() {
    let Some(core) = Core::start("img") else { return };
    let png = core.dir.join("clip.png");
    std::fs::write(&png, PNG).unwrap();
    let drop_dir = core.dir.join("my shots");
    std::fs::create_dir_all(&drop_dir).unwrap();
    let shot = drop_dir.join("shot 1.png");
    std::fs::write(&shot, PNG).unwrap();
    core.config(&format!(
        "[sidebar]\nmode = \"hidden\"\n[paste]\nimages = \"always\"\nimage_command = \"cat {}\"\n",
        png.display()
    ));
    let mut h = Corgi::spawn(&core);
    h.wait_for("$", 10);
    let pause = || std::thread::sleep(Duration::from_millis(250));
    let check = |h: &mut Corgi, paste: &str, orig: &std::path::Path, n: u32| {
        // The last one's note goes first, so the next wait sees this one's.
        let end = Instant::now() + Duration::from_secs(10);
        while h.screen().iter().any(|l| l.contains("pasted an image")) {
            assert!(Instant::now() < end);
            std::thread::sleep(Duration::from_millis(100));
        }
        h.typ("cmp ");
        pause();
        h.typ(paste);
        h.wait_for("pasted an image", 10);
        pause();
        h.typ(&format!(" '{}' && echo ok-$((100+{n}))\r", orig.display()));
        h.wait_for(&format!("ok-{}", 100 + n), 10);
        h.typ("clear\r");
        pause();
    };
    // The clipboard's image.
    check(&mut h, "\x1b[200~\x1b[201~", &png, 1);
    // A file dropped on the terminal: its path, escaped as terminals do.
    let dropped = shot.display().to_string().replace(' ', "\\ ");
    check(&mut h, &format!("\x1b[200~{dropped}\x1b[201~"), &shot, 2);
    // On purpose.
    check(&mut h, "\x02\x16", &png, 3);
    // Text pastes as it is.
    h.typ("\x1b[200~echo plain-$((40+2))\x1b[201~\r");
    h.wait_for("plain-42", 10);
    h.typ("\x02d");
    assert_eq!(h.wait_exit(5), 0);
}

/// The narrow layout (a phone over ssh): at 64 columns or fewer, a header with a switch button
/// instead of the sidebar and tab bar, one pane at a time; wide again, the usual layout.
#[test]
fn narrow_screens_get_a_header_and_one_pane() {
    let Some(core) = Core::start("nar") else { return };
    let mut h = Corgi::spawn_sized(&core, &core.dir, &[], (44, 24));
    h.wait_for("switch", 10);
    let s = h.wait_for("$", 10);
    let pause = || std::thread::sleep(Duration::from_millis(250));
    assert!(!s.iter().any(|l| l.contains("Workspaces")), "no sidebar: {s:#?}");
    assert!(
        s[0].contains("nar") && s[0].contains("0:"),
        "the workspace and window: {s:#?}"
    );
    assert!(s[1].contains("no agents"), "{s:#?}");
    // The pane has the whole width: no boxes, no inset.
    h.typ("tput cols\r");
    let end = Instant::now() + Duration::from_secs(10);
    while !h.screen().iter().skip(2).any(|l| l.trim() == "44") {
        assert!(Instant::now() < end, "{:#?}", h.screen());
        std::thread::sleep(Duration::from_millis(100));
    }
    // A split: still one pane on screen, the header says which.
    h.typ("\x02%");
    let s = h.wait_for("pane 2/2", 10);
    assert!(
        !s.iter().skip(2).any(|l| l.contains('│')),
        "one pane, no divider: {s:#?}"
    );
    pause();
    h.typ("\x02o");
    h.wait_for("pane 1/2", 5);
    // The switcher: the chooser, the whole screen.
    h.typ("\x02w");
    let s = h.wait_for("choose", 5);
    assert!(s[0].starts_with('┌') && s[0].contains("choose"), "{s:#?}");
    h.typ("\x1b");
    pause();
    // Wide: the usual layout; narrow again: the header.
    h.resize(100, 30);
    let s = h.wait_for("Workspaces", 10);
    assert!(!s.iter().any(|l| l.contains("switch")), "{s:#?}");
    h.resize(44, 24);
    h.wait_for("switch", 10);
    h.typ("\x02d");
    assert_eq!(h.wait_exit(5), 0);
}

/// Selecting with the mouse: a drag over a word copies it when you let go (into the paste
/// buffer and the terminal's clipboard); a double click copies the word under it.
#[test]
fn the_mouse_selects_and_copies() {
    let Some(core) = Core::start("ms") else { return };
    core.config(
        "[sidebar]\nmode = \"hidden\"\n[tab_bar]\nstyle = \"plain\"\nposition = \"top\"\n\
         [panes]\nstyle = \"plain\"\ntitle = \"off\"\nseparate = false\ninset = 0\n",
    );
    let mut h = Corgi::spawn(&core);
    h.wait_for("$", 10);
    let pause = || std::thread::sleep(Duration::from_millis(250));
    h.typ("printf 'alpha beta gamma\\n'\r");
    let s = h.wait_for("alpha beta gamma", 10);
    let (y, line) = s
        .iter()
        .enumerate()
        .find(|(_, l)| l.starts_with("alpha beta gamma"))
        .expect("the printed line");
    let x = line.find("beta").unwrap() as u16;
    let (y, g) = (y as u16, line.find("gamma").unwrap() as u16);
    // SGR mouse reports are 1-based.
    let at = |b: u8, x: u16, end: char| format!("\x1b[<{b};{};{}{end}", x + 1, y + 1);
    h.typ(&at(0, x, 'M'));
    h.typ(&at(32, x + 2, 'M'));
    h.typ(&at(32, x + 3, 'M'));
    h.typ(&at(0, x + 3, 'm'));
    h.wait_for("copied 4 characters", 5);
    h.typ("echo ");
    pause();
    h.typ("\x02]");
    pause();
    h.typ("-done\r");
    h.wait_for("beta-done", 5);
    // A double click: the word.
    let s = h.screen();
    let y2 = s.iter().position(|l| l.starts_with("alpha beta gamma")).unwrap() as u16;
    let at = |b: u8, end: char| format!("\x1b[<{b};{};{}{end}", g + 2, y2 + 1);
    h.typ(&(at(0, 'M') + &at(0, 'm') + &at(0, 'M') + &at(0, 'm')));
    h.wait_for("copied 5 characters", 5);
    h.typ("echo ");
    pause();
    h.typ("\x02]");
    pause();
    h.typ("-twice\r");
    h.wait_for("gamma-twice", 5);
    h.typ("\x02d");
    assert_eq!(h.wait_exit(5), 0);
}

/// The mouse for programs: once one asks for mouse reports, its clicks reach it (SGR, in its
/// own coordinates); Shift+drag is still corgi's selection; a full-screen program that didn't
/// ask gets the wheel as arrow keys.
#[test]
fn programs_that_ask_get_the_mouse() {
    let Some(core) = Core::start("mp") else { return };
    core.config(
        "[sidebar]\nmode = \"hidden\"\n[tab_bar]\nstyle = \"plain\"\nposition = \"top\"\n\
         [panes]\nstyle = \"plain\"\ntitle = \"off\"\nseparate = false\ninset = 0\n",
    );
    let mut h = Corgi::spawn(&core);
    h.wait_for("$", 10);
    let pause = || std::thread::sleep(Duration::from_millis(400));
    // Mouse reports on (button events, SGR); what arrives is printed with ESC as E.
    h.typ("printf '\\033[?1002h\\033[?1006h'; stty -echo -icanon; head -c 20 | tr '\\033' E; printf '\\033[?1002l\\033[?1006l'; stty sane; echo ' got'\r");
    pause();
    // Screen (10, 5) is the pane's (10, 4): 1-based, 11;5.
    h.typ("\x1b[<0;11;6M\x1b[<0;11;6m");
    h.wait_for("E[<0;11;5ME[<0;11;5m got", 10);
    // The alternate screen, no mouse reports: the wheel is arrow keys.
    h.typ("clear; printf '\\033[?1049h'; stty -echo -icanon; head -c 9 | tr '\\033' E > wheel.txt; printf '\\033[?1049l'; stty sane; cat wheel.txt; echo ' wheel'\r");
    pause();
    h.typ("\x1b[<64;5;5M");
    h.wait_for("E[AE[AE[A wheel", 10);
    // Shift held: corgi's selection, even over a program with the mouse.
    h.typ("clear; printf 'pick this\\n'; printf '\\033[?1002h\\033[?1006h'\r");
    let s = h.wait_for("pick this", 10);
    let y = s.iter().position(|l| l.starts_with("pick this")).unwrap() + 1;
    // SGR with Shift: +4.
    h.typ(&format!("\x1b[<4;1;{y}M\x1b[<36;3;{y}M\x1b[<36;4;{y}M\x1b[<4;4;{y}m"));
    h.wait_for("copied 4 characters", 5);
    h.typ("printf '\\033[?1002l\\033[?1006l'\r");
    pause();
    h.typ("\x02d");
    assert_eq!(h.wait_exit(5), 0);
}

/// Recovery: the core restarts under corgi (as after `hc update`: programs keep running);
/// corgi reconnects, and the same shell, with what it printed, takes keys again.
#[test]
fn a_core_restart_keeps_the_panes() {
    let Some(core) = Core::start("rr") else { return };
    let mut h = Corgi::spawn(&core);
    h.wait_for("$", 10);
    h.typ("x=kept-$((20+3)); echo before-$((1+1))\r");
    h.wait_for("before-2", 10);
    let before = live(&core);
    let shell = pids(&core);
    let r = core.hc(&["server", "restart", "--json"]);
    assert!(!r.is_null(), "restart said nothing");
    // The same terminal, with its shell's state: the variable set before is still there.
    let end = Instant::now() + Duration::from_secs(20);
    loop {
        h.typ("echo \"[$x]\"\r");
        std::thread::sleep(Duration::from_millis(700));
        if h.screen().iter().any(|l| l.contains("[kept-23]")) {
            break;
        }
        assert!(Instant::now() < end, "{:#?}", h.screen());
    }
    assert_eq!(live(&core), before, "the same terminals");
    assert!(!shell.is_empty());
    assert_eq!(pids(&core), shell, "the same shell: programs keep running");
    assert!(
        h.screen().iter().any(|l| l.contains("before-2")),
        "what it showed stays"
    );
    h.typ("\x02d");
    assert_eq!(h.wait_exit(5), 0);
}

/// Recovery: a pane's program dies by a signal; its terminal is kept, so core gives it a new
/// shell (`lost`, then `restarted`) and corgi shows that one and types into it.
#[test]
fn a_killed_shell_comes_back() {
    let Some(core) = Core::start("rk") else { return };
    let mut h = Corgi::spawn(&core);
    h.wait_for("$", 10);
    let before = live(&core);
    let shell = pids(&core);
    h.typ("kill -9 $$\r");
    let end = Instant::now() + Duration::from_secs(30);
    loop {
        h.typ("echo again-$((4*4))\r");
        std::thread::sleep(Duration::from_millis(800));
        if h.screen().iter().any(|l| l.contains("again-16")) {
            break;
        }
        assert!(Instant::now() < end, "{:#?}", h.screen());
    }
    assert_eq!(live(&core), before, "the same terminal, a new shell in it");
    let now = pids(&core);
    assert!(!now.is_empty() && now != shell, "a new shell: {shell:?} then {now:?}");
    h.typ("\x02d");
    assert_eq!(h.wait_exit(5), 0);
}

/// Recovery: the core stops (its kept terminals parked) and starts again (they get new
/// shells); corgi reconnects with its layout: the same windows, each pane taking keys.
#[test]
fn a_core_stop_and_start_keeps_the_layout() {
    let Some(core) = Core::start("rs") else { return };
    core.config(
        "[sidebar]\nmode = \"hidden\"\n[tab_bar]\nstyle = \"plain\"\nposition = \"top\"\n\
         [panes]\nstyle = \"plain\"\ntitle = \"off\"\nseparate = false\ninset = 0\n",
    );
    let mut h = Corgi::spawn(&core);
    h.wait_for("$", 10);
    h.typ("\x02c");
    h.wait_for(" 1:", 10);
    let addrs = live(&core);
    assert_eq!(addrs.len(), 2);
    let shells = pids(&core);
    core.hc(&["server", "stop", "--json"]);
    // corgi starts it again as it reconnects (`hc connect` does); the windows come back.
    let end = Instant::now() + Duration::from_secs(30);
    loop {
        h.typ("echo back-$((7*6))\r");
        std::thread::sleep(Duration::from_millis(800));
        if h.screen().iter().any(|l| l.contains("back-42")) {
            break;
        }
        assert!(Instant::now() < end, "{:#?}", h.screen());
    }
    let s = h.screen();
    assert!(s[0].contains(" 0:") && s[0].contains(" 1:"), "both windows: {:?}", s[0]);
    assert_eq!(live(&core), addrs, "the same terminals, new shells");
    let now = pids(&core);
    assert!(
        now.len() == 2 && now.iter().all(|p| !shells.contains(p)),
        "{shells:?} then {now:?}"
    );
    // The other window takes keys too.
    h.typ("\x02p");
    std::thread::sleep(Duration::from_millis(300));
    h.typ("echo first-$((5+5))\r");
    h.wait_for("first-10", 10);
    h.typ("\x02d");
    assert_eq!(h.wait_exit(5), 0);
}

/// corgi's CPU time so far, in seconds (`ps`).
fn cpu_secs(pid: u32) -> f64 {
    let out = Command::new("ps")
        .args(["-o", "time=", "-p", &pid.to_string()])
        .output()
        .unwrap();
    let t = String::from_utf8_lossy(&out.stdout).trim().to_string();
    // [[dd-]hh:]mm:ss.ss
    t.rsplit(['-', ':'])
        .zip([1.0, 60.0, 3600.0, 86400.0])
        .map(|(v, k)| v.parse::<f64>().unwrap_or(0.0) * k)
        .sum()
}

/// Idle is idle: waiting in prefix mode with the key hints off (nothing will appear) doesn't
/// keep waking corgi to redraw, and neither does plain idle.
#[test]
fn idle_doesnt_spin() {
    let Some(core) = Core::start("idle") else { return };
    core.config("[keys]\nhints = \"off\"\n[usage]\nenabled = false\n");
    let mut h = Corgi::spawn(&core);
    h.wait_for("$", 10);
    std::thread::sleep(Duration::from_secs(2));
    let pid = h.child.id();
    let measure = |what: &str| {
        let before = cpu_secs(pid);
        std::thread::sleep(Duration::from_secs(3));
        let used = cpu_secs(pid) - before;
        assert!(used < 0.15, "{what}: {used:.2} CPU seconds in 3");
        used
    };
    measure("idle");
    h.typ("\x02");
    std::thread::sleep(Duration::from_millis(300));
    measure("prefix mode, hints off");
    h.typ("\x1b");
    std::thread::sleep(Duration::from_millis(300));
    h.typ("\x02d");
    assert_eq!(h.wait_exit(5), 0);
}

/// corgi's resident memory, in MB (`ps`).
fn rss_mb(pid: u32) -> f64 {
    let out = Command::new("ps")
        .args(["-o", "rss=", "-p", &pid.to_string()])
        .output()
        .unwrap();
    String::from_utf8_lossy(&out.stdout)
        .trim()
        .parse::<f64>()
        .unwrap_or(0.0)
        / 1024.0
}

/// A flood: three panes printing as fast as they can (`yes`). A fourth still answers quickly,
/// and corgi's memory stays bounded: output is credited back to core only once the emulator
/// has it, so what waits for corgi stays within each pane's credit (the rest waits in the
/// program). (Seven floods measured the same, about 0.1 s and 20 MB, release or debug.)
///
/// Not in the usual run: the floods take whole cores, which starves the tests running beside
/// it. Run it on its own: `cargo test -p corgi-tui --test e2e -- --ignored a_flood`.
#[test]
#[ignore = "floods the machine; run on its own with --ignored"]
fn a_flood_doesnt_drown_the_others() {
    let Some(core) = Core::start("fl") else { return };
    core.config(
        "[sidebar]\nmode = \"hidden\"\n[usage]\nenabled = false\n[tab_bar]\nstyle = \"plain\"\nposition = \"top\"\n\
         [panes]\nstyle = \"plain\"\ntitle = \"off\"\nseparate = false\ninset = 0\n",
    );
    let mut h = Corgi::spawn(&core);
    h.wait_for("$", 10);
    let pause = || std::thread::sleep(Duration::from_millis(400));
    for _ in 0..3 {
        // It stops itself after a while, whatever happens to the test.
        h.typ("sh -c 'yes flood-flood-flood-flood & sleep 12; kill $!'\r");
        pause();
        h.typ("\x02%");
        pause();
        // Tiled, so every pane has room.
        h.typ("\x02\x1b5");
        pause();
    }
    // The floods end with the test, not in the teardown (a core this busy may not stop in
    // time), even if it fails.
    let _stop = Floods(&core);
    let pid = h.child.id();
    std::thread::sleep(Duration::from_secs(3));
    let mut worst = Duration::ZERO;
    for k in 0..3 {
        let t = Instant::now();
        h.typ(&format!("echo lat-$((100+{k}))\r"));
        h.wait_for(&format!("lat-{}", 100 + k), 20);
        worst = worst.max(t.elapsed());
        h.typ("clear\r");
        pause();
    }
    let rss = rss_mb(pid);
    eprintln!("flood: slowest echo {worst:?}, corgi {rss:.0} MB");
    assert!(worst < Duration::from_secs(3), "slowest echo {worst:?}");
    assert!(rss < 300.0, "corgi {rss:.0} MB");
    h.typ("\x02d");
    assert_eq!(h.wait_exit(10), 0);
}

/// Kills every terminal of a core when dropped (even on a failure).
struct Floods<'a>(&'a Core);

impl Drop for Floods<'_> {
    fn drop(&mut self) {
        for t in live(self.0) {
            self.0.hc(&["kill", &t]);
        }
    }
}

/// `corgi migrate`: a herdr config in, a corgi one out; it won't replace one that's there
/// without --force, and then keeps the old one.
#[test]
fn migrate_writes_a_corgi_config_from_herdrs() {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("h").join("migrate");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let herdr = dir.join("herdr.toml");
    std::fs::write(
        &herdr,
        "[keys]\nprefix = \"ctrl+a\"\nsplit_vertical = \"prefix+|\"\n[ui]\nstatus_indicators = \"symbols\"\n",
    )
    .unwrap();
    let to = dir.join("corgi/config.toml");
    let run = |args: &[&str]| {
        Command::new(env!("CARGO_BIN_EXE_corgi"))
            .arg("--config")
            .arg(&to)
            .arg("migrate")
            .arg("--from")
            .arg(&herdr)
            .args(args)
            .env("HOME", &dir)
            .output()
            .unwrap()
    };
    let shown = run(&["--print"]);
    let text = String::from_utf8_lossy(&shown.stdout);
    assert!(
        text.contains("prefix = \"C-a\"") && text.contains("key = \"|\""),
        "{text}"
    );
    assert!(!to.exists(), "--print writes nothing");
    let first = run(&[]);
    assert!(first.status.success(), "{}", String::from_utf8_lossy(&first.stderr));
    assert_eq!(std::fs::read_to_string(&to).unwrap(), text);
    std::fs::write(&to, "# mine\n").unwrap();
    let again = run(&[]);
    assert!(!again.status.success());
    assert!(String::from_utf8_lossy(&again.stderr).contains("--force"));
    assert_eq!(std::fs::read_to_string(&to).unwrap(), "# mine\n", "left alone");
    let forced = run(&["--force"]);
    assert!(forced.status.success());
    assert_eq!(
        std::fs::read_to_string(dir.join("corgi/config.toml.bak")).unwrap(),
        "# mine\n"
    );
    assert_eq!(std::fs::read_to_string(&to).unwrap(), text);
    std::fs::remove_dir_all(&dir).unwrap();
}
