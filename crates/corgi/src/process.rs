//! Running a short program for its output (a usage `command`, the clipboard reader) within a
//! deadline: its output is read while it runs (so a full pipe can't stall it), at most `keep`
//! bytes kept; one deadline covers it ending and its output closing. It runs in a process
//! group of its own, killed whole on the deadline (or once it's done, if something it started
//! still holds the output open), so nothing it left behind keeps corgi waiting.

use std::io::Read;
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

/// After the program ends, how long its output may stay open (something it started in the
/// background holding it) before that's killed.
const LINGER: Duration = Duration::from_millis(250);

#[derive(Debug, PartialEq)]
pub struct Output {
    pub bytes: Vec<u8>,
    /// It printed more than was kept.
    pub cut: bool,
    pub ok: bool,
}

/// Run `prog args` and collect what it prints, within `timeout`.
pub fn run(prog: &str, args: &[&str], timeout: Duration, keep: usize) -> Result<Output, String> {
    let mut cmd = Command::new(prog);
    cmd.args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    #[cfg(unix)]
    std::os::unix::process::CommandExt::process_group(&mut cmd, 0);
    let mut child = cmd.spawn().map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            format!("{prog} not found")
        } else {
            format!("{prog}: {e}")
        }
    })?;
    let pid = child.id();
    let mut out = child.stdout.take().ok_or("no output")?;
    // The output, read as it comes: the first `keep` bytes kept, the rest read and dropped.
    let (otx, orx) = mpsc::channel();
    std::thread::spawn(move || {
        let (mut buf, mut cut) = (Vec::new(), false);
        let mut chunk = [0u8; 8192];
        loop {
            match out.read(&mut chunk) {
                Ok(0) | Err(_) => break,
                Ok(n) => {
                    let room = keep.saturating_sub(buf.len());
                    buf.extend_from_slice(&chunk[..n.min(room)]);
                    cut |= n > room;
                }
            }
        }
        let _ = otx.send((buf, cut));
    });
    let (wtx, wrx) = mpsc::channel();
    std::thread::spawn(move || {
        let _ = wtx.send(child.wait().map(|s| s.success()).unwrap_or(false));
    });
    let deadline = Instant::now() + timeout;
    let left = || deadline.saturating_duration_since(Instant::now());
    let ok = match wrx.recv_timeout(left()) {
        Ok(ok) => ok,
        Err(_) => {
            kill_group(pid);
            return Err(format!("{prog} took too long"));
        }
    };
    let (bytes, cut) = match orx.recv_timeout(left().min(LINGER)) {
        Ok(r) => r,
        Err(_) => {
            // Something it started still holds its output: that goes, and what came stays.
            kill_group(pid);
            orx.recv_timeout(Duration::from_secs(1)).unwrap_or_default()
        }
    };
    Ok(Output { bytes, cut, ok })
}

/// Kill the process group the program led (it and whatever it started).
fn kill_group(pid: u32) {
    #[cfg(unix)]
    // SAFETY: a plain signal to a process group we created; a stale id fails harmlessly.
    unsafe {
        libc::kill(-(pid as libc::pid_t), libc::SIGKILL);
    }
    #[cfg(not(unix))]
    let _ = pid;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sh(line: &str, secs: u64, keep: usize) -> Result<Output, String> {
        run("sh", &["-c", line], Duration::from_secs(secs), keep)
    }

    #[test]
    fn output_exit_and_limits() {
        let o = sh("echo hi; echo there", 5, 1024).unwrap();
        assert_eq!((o.bytes.as_slice(), o.cut, o.ok), (&b"hi\nthere\n"[..], false, true));
        assert!(!sh("echo no; exit 3", 5, 1024).unwrap().ok, "a failure is said");
        // More than a pipe holds: read while it runs, the rest dropped, and it still ends.
        let t = Instant::now();
        let o = sh("head -c 1000000 /dev/zero", 5, 100).unwrap();
        assert!(o.ok && o.cut && o.bytes.len() == 100);
        assert!(t.elapsed() < Duration::from_secs(3));
        assert!(
            run("no-such-program-here", &[], Duration::from_secs(1), 10)
                .unwrap_err()
                .contains("not found")
        );
    }

    #[test]
    fn the_deadline_holds() {
        // It never ends: killed at the deadline.
        let t = Instant::now();
        assert!(sh("sleep 30", 1, 10).unwrap_err().contains("took too long"));
        assert!(t.elapsed() < Duration::from_secs(3), "{:?}", t.elapsed());
        // It ends, but something it started keeps its output open: what it printed comes back
        // at once, and the straggler goes.
        let t = Instant::now();
        let o = sh("sleep 30 & echo done", 5, 100).unwrap();
        assert_eq!(o.bytes, b"done\n");
        assert!(t.elapsed() < Duration::from_secs(2), "{:?}", t.elapsed());
    }
}
