//! The README's demos, recorded: corgi driven in a PTY by a script of keys and pauses, what it
//! draws saved as asciinema casts (`docs/assets/demos/*.cast`; `scripts/demos.sh` makes GIFs of
//! them with agg). Each runs on a core session of its own, with HOME a scratch folder and a
//! plain `$ ` prompt, so nothing of yours shows. Not in the usual run:
//! `cargo test -p corgi-tui --test demos -- --ignored --test-threads=1`.

use std::ffi::OsString;
use std::io::{Read, Write};
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

const COLS: u16 = 100;
const ROWS: u16 = 28;

/// A core of its own, and a scratch HOME with a few projects in it.
struct Core {
    dir: PathBuf,
    id: String,
}

impl Core {
    fn start(id: &str) -> Option<Core> {
        if Command::new("hc").arg("--version").output().is_err() {
            eprintln!("skipping: hc not installed");
            return None;
        }
        let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("d").join(id);
        let _ = std::fs::remove_dir_all(&dir);
        for p in ["home/web/src", "home/api", "home/docs", "bin"] {
            std::fs::create_dir_all(dir.join(p)).unwrap();
        }
        // A plain prompt: shells start as login shells, after the system's files (which put
        // the real host and user name in it).
        std::fs::write(dir.join("home/.profile"), "PS1='$ '; export PS1\n").unwrap();
        // `corgi` on PATH, for the demo that starts it from a shell.
        std::os::unix::fs::symlink(env!("CARGO_BIN_EXE_corgi"), dir.join("bin/corgi")).unwrap();
        let core = Core { dir, id: id.into() };
        for repo in ["web", "api"] {
            let r = core.home().join(repo);
            let git = |args: &[&str]| {
                Command::new("git").current_dir(&r).args(args).output().unwrap();
            };
            git(&["init", "-q", "-b", "main"]);
            git(&[
                "-c",
                "user.email=demo@example.invalid",
                "-c",
                "user.name=demo",
                "commit",
                "-q",
                "--allow-empty",
                "-m",
                "start",
            ]);
        }
        Some(core)
    }

    fn home(&self) -> PathBuf {
        self.dir.join("home")
    }

    fn env(&self) -> Vec<(String, OsString)> {
        let path = format!(
            "{}:{}",
            self.dir.join("bin").display(),
            std::env::var("PATH").unwrap_or_default()
        );
        vec![
            ("HC_SESSION".into(), format!("d{}", self.id).into()),
            ("XDG_STATE_HOME".into(), self.dir.join("s").into()),
            ("XDG_CONFIG_HOME".into(), self.dir.join("c").into()),
            ("XDG_RUNTIME_DIR".into(), self.dir.join("r").into()),
            ("GIT_CEILING_DIRECTORIES".into(), self.dir.parent().unwrap().into()),
            ("HOME".into(), self.home().into()),
            ("CLAUDE_CONFIG_DIR".into(), self.dir.join("claude").into()),
            ("CODEX_HOME".into(), self.dir.join("codex").into()),
            ("SHELL".into(), "/bin/sh".into()),
            ("PS1".into(), "$ ".into()),
            ("ENV".into(), self.home().join(".profile").into()),
            ("PATH".into(), path.into()),
        ]
    }

    fn config(&self, toml: &str) {
        let dir = self.dir.join("c/corgi");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("config.toml"), toml).unwrap();
    }

    fn hc(&self, args: &[&str]) -> serde_json::Value {
        let out = Command::new("hc")
            .args(args)
            .current_dir(&self.dir)
            .envs(self.env())
            .output()
            .unwrap();
        serde_json::from_slice(&out.stdout).unwrap_or(serde_json::Value::Null)
    }
}

impl Drop for Core {
    fn drop(&mut self) {
        // Programs first (a busy core may not stop in time), then the core and its holders.
        if let Some(ts) = self.hc(&["ls", "--json"])["terminals"].as_array() {
            for t in ts {
                if let Some(a) = t["address"].as_str() {
                    self.hc(&["kill", a]);
                }
            }
        }
        let status = self.hc(&["server", "status"]);
        self.hc(&["server", "stop"]);
        for h in status["holders"].as_array().into_iter().flatten() {
            if let Some(pid) = h["pid"].as_i64() {
                let _ = Command::new("kill").arg(pid.to_string()).output();
            }
        }
    }
}

/// What the program wrote, and when.
type Output = Arc<Mutex<Vec<(Instant, Vec<u8>)>>>;

/// A program on a PTY, answering the host-terminal queries a real terminal would, its output
/// kept with when it came.
struct Rec {
    child: Child,
    master: std::fs::File,
    out: Output,
    /// Where the recording starts (an index into `out`).
    mark: usize,
}

impl Rec {
    fn spawn(core: &Core, program: &Path, args: &[&str], cwd: &Path) -> Rec {
        let (mut m, mut s) = (0, 0);
        let mut ws = libc::winsize {
            ws_row: ROWS,
            ws_col: COLS,
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
        let mut cmd = Command::new(program);
        cmd.args(args)
            .current_dir(cwd)
            .env_clear()
            .env("TERM", "xterm-256color")
            .env("LANG", "en_US.UTF-8")
            .env("USER", "demo")
            .envs(core.env())
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
                if chunk.windows(4).any(|x| x == b"\x1b[?u") {
                    let _ = w.write_all(b"\x1b[?0u");
                }
                if chunk.windows(3).any(|x| x == b"\x1b[c") {
                    let _ = w.write_all(b"\x1b[?62;22c");
                }
                sink.lock().unwrap().push((Instant::now(), chunk.to_vec()));
            }
        });
        Rec {
            child,
            master,
            out,
            mark: 0,
        }
    }

    /// corgi itself, in `cwd`.
    fn corgi(core: &Core, cwd: &Path) -> Rec {
        Rec::spawn(core, Path::new(env!("CARGO_BIN_EXE_corgi")), &[], cwd)
    }

    fn screen(&self) -> Vec<String> {
        let mut vt = corgi_vt::Vt::new(COLS, ROWS).unwrap();
        for (_, b) in self.out.lock().unwrap().iter() {
            vt.write(b);
        }
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
            })
            .collect()
    }

    fn wait_for(&self, what: &str, secs: u64) {
        let end = Instant::now() + Duration::from_secs(secs);
        while !self.screen().iter().any(|l| l.contains(what)) {
            assert!(
                Instant::now() < end,
                "never saw {what:?}:\n{}",
                self.screen().join("\n")
            );
            std::thread::sleep(Duration::from_millis(50));
        }
    }

    /// Keys at once (a chord, a control key).
    fn key(&mut self, s: &str) {
        self.master.write_all(s.as_bytes()).unwrap();
    }

    /// Typed, a character at a time, as a person would.
    fn typ(&mut self, s: &str) {
        for c in s.chars() {
            let mut b = [0u8; 4];
            self.master.write_all(c.encode_utf8(&mut b).as_bytes()).unwrap();
            std::thread::sleep(Duration::from_millis(if c == '\r' { 150 } else { 45 }));
        }
    }

    fn resize(&self, cols: u16, rows: u16) {
        let ws = libc::winsize {
            ws_row: rows,
            ws_col: cols,
            ws_xpixel: 0,
            ws_ypixel: 0,
        };
        // SAFETY: the master fd is ours and open; winsize is valid.
        unsafe { libc::ioctl(self.master.as_raw_fd(), libc::TIOCSWINSZ, &ws) };
    }

    /// Start recording here, with a whole fresh screen: a resize makes corgi draw it all again.
    fn start(&mut self) {
        self.resize(COLS, ROWS - 1);
        pause(400);
        self.mark = self.out.lock().unwrap().len();
        self.resize(COLS, ROWS);
        pause(600);
    }

    /// The recording so far, as an asciinema v2 cast.
    fn save(&self, name: &str, title: &str) {
        let out = self.out.lock().unwrap();
        let chunks = &out[self.mark..];
        let t0 = chunks.first().map_or(Instant::now(), |c| c.0);
        let header = serde_json::json!({
            "version": 2,
            "width": COLS,
            "height": ROWS,
            "title": title,
            "env": {"TERM": "xterm-256color", "SHELL": "/bin/sh"},
        });
        let mut text = header.to_string() + "\n";
        // Output split mid-character is held until the rest comes.
        let mut held: Vec<u8> = Vec::new();
        for (at, bytes) in chunks {
            held.extend_from_slice(bytes);
            let ok = match std::str::from_utf8(&held) {
                Ok(_) => held.len(),
                Err(e) => e.valid_up_to(),
            };
            if ok == 0 {
                continue;
            }
            let s = String::from_utf8(held[..ok].to_vec()).unwrap();
            held.drain(..ok);
            let t = at.duration_since(t0).as_secs_f64();
            text.push_str(&serde_json::json!([t, "o", s]).to_string());
            text.push('\n');
        }
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/assets/demos");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join(format!("{name}.cast")), text).unwrap();
    }

    fn quit(mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn pause(ms: u64) {
    std::thread::sleep(Duration::from_millis(ms));
}

/// What every demo starts from: the default look, with usage showing agents and the time.
const CONFIG: &str = "[[usage.segment]]\nprovider = \"agents\"\npriority = 90\n\
                      [[usage.segment]]\nprovider = \"clock\"\npriority = 10\n";

/// 1. The prefix shows what you can press; splits, zoom, a new window.
#[test]
#[ignore = "records a demo"]
fn demo_keys() {
    let Some(core) = Core::start("keys") else { return };
    core.config(CONFIG);
    let mut r = Rec::corgi(&core, &core.home().join("web"));
    r.wait_for("$ ", 15);
    pause(1500);
    r.start();
    r.typ("git log --oneline\r");
    pause(1200);
    // The prefix: every key you can press next.
    r.key("\x02");
    pause(2800);
    r.key("%");
    r.wait_for("│ $", 5);
    pause(900);
    r.typ("ls\r");
    pause(900);
    r.key("\x02\"");
    pause(1200);
    r.typ("echo split down\r");
    pause(1200);
    // Zoom, and back.
    r.key("\x02z");
    pause(1800);
    r.key("\x02z");
    pause(1200);
    // A new window.
    r.key("\x02c");
    pause(1000);
    r.typ("echo a second window\r");
    pause(1500);
    r.key("\x02");
    pause(300);
    r.key("0");
    pause(2200);
    r.save("keys", "corgi: keys, splits, zoom, windows");
    r.quit();
}

/// 2. An agent works, needs you, finishes: the sidebar, the spinner, toasts, C-b a.
#[test]
#[ignore = "records a demo"]
fn demo_agents() {
    let Some(core) = Core::start("agents") else { return };
    core.config(CONFIG);
    // A stand-in for a coding agent: it says what it's doing over OSC 7501, as agents do.
    std::fs::write(
        core.home().join("agent.sh"),
        r#"say() { printf '\033]7501;state=%s:app=claude:title=%s\033\\' "$1" "$(printf %s "$2" | base64)"; }
clear
say working "Reading the parser"
echo "> Reading src/parse.rs"; sleep 2
say working "Fixing an off-by-one"
echo "> Editing src/parse.rs (+3 -1)"; sleep 3
say blocked "Run the tests?"
echo; printf "Allow running cargo test? [y/n] "; read a
say working "Running the tests"
echo "> cargo test"; sleep 3
echo "test result: ok. 42 passed"
say done "Fixed the off-by-one"
sleep 600
"#,
    )
    .unwrap();
    let mut r = Rec::corgi(&core, &core.home().join("web"));
    r.wait_for("$ ", 15);
    pause(1500);
    r.start();
    // The agent in a window of its own.
    r.key("\x02c");
    pause(800);
    r.typ("sh ~/agent.sh\r");
    r.wait_for("Reading src/parse.rs", 10);
    pause(2500);
    // Back to the shell; the agent keeps going, the sidebar says how.
    r.key("\x02");
    pause(300);
    r.key("0");
    pause(800);
    r.typ("git status --short\r");
    // It needs you: a toast, a red dot. C-b a goes to it.
    r.wait_for("1 waiting", 20);
    pause(3000);
    r.key("\x02");
    pause(300);
    r.key("a");
    pause(1500);
    r.typ("y\r");
    pause(1200);
    r.key("\x02");
    pause(300);
    r.key("0");
    // It finished while you were elsewhere: a toast, a green dot.
    r.wait_for("1 done", 15);
    pause(3500);
    r.save("agents", "corgi: agents report their status");
    r.quit();
}

/// 3. Detach, and come back: the programs kept running.
#[test]
#[ignore = "records a demo"]
fn demo_detach() {
    let Some(core) = Core::start("detach") else { return };
    core.config(CONFIG);
    let home = core.home();
    let mut r = Rec::spawn(&core, Path::new("/bin/sh"), &["-i"], &home.join("web"));
    r.wait_for("$ ", 10);
    pause(500);
    // From a clear screen with its prompt (C-l redraws both, in the recording).
    r.mark = r.out.lock().unwrap().len();
    r.key("\x0c");
    pause(1000);
    r.typ("corgi\r");
    r.wait_for("NORMAL", 15);
    pause(1200);
    r.typ("i=0; while true; do i=$((i+1)); echo \"tick $i\"; sleep 1; done\r");
    pause(4000);
    // Detach: corgi goes, the loop doesn't.
    r.key("\x02");
    pause(500);
    r.key("d");
    pause(2500);
    r.typ("corgi\r");
    r.wait_for("NORMAL", 15);
    pause(4500);
    r.save("detach", "corgi: detach and come back");
    r.key("\x03");
    pause(300);
    r.quit();
}

/// 4. The chooser (filter, go), then copy mode: search, a block, copy and paste.
#[test]
#[ignore = "records a demo"]
fn demo_choose_and_copy() {
    let Some(core) = Core::start("choose") else { return };
    core.config(CONFIG);
    std::fs::write(
        core.home().join("api/routes.txt"),
        "GET    /users        list users\n\
         POST   /users        create a user\n\
         GET    /users/:id    one user\n\
         DELETE /users/:id    remove a user\n\
         GET    /health       liveness\n",
    )
    .unwrap();
    let mut r = Rec::corgi(&core, &core.home().join("web"));
    r.wait_for("$ ", 15);
    // Two more workspaces, before the recording starts.
    for ws in ["api", "docs"] {
        r.key(&format!("\x02:new-workspace -c ~/{ws}\r"));
        pause(1500);
    }
    r.key("\x02(");
    pause(400);
    r.key("\x02(");
    pause(1500);
    r.start();
    // The chooser: type to filter, Enter goes there.
    r.key("\x02w");
    pause(1800);
    r.typ("api");
    pause(1500);
    r.key("\r");
    pause(1200);
    r.typ("cat routes.txt\r");
    pause(1500);
    // Copy mode: a search, then a block (the paths column), copied.
    r.key("\x02[");
    pause(1000);
    r.typ("?create\r");
    pause(1500);
    r.typ("ggj7l");
    pause(700);
    r.key("\x16");
    pause(500);
    r.typ("3j9l");
    pause(1500);
    r.typ("y");
    pause(1500);
    // Paste it back.
    r.typ("echo '");
    r.key("\x02]");
    pause(600);
    r.typ("'\r");
    pause(2500);
    r.save("choose-and-copy", "corgi: the chooser and copy mode");
    r.quit();
}
