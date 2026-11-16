//! Local files: which workspace this client looks at (`view-<session>.json`),
//! and rolling backups of every layout document corgi saved.
//! The layout itself lives in core (see `model::DOC`).
//!
//! Writes go to one thread of their own, in order, so the UI never waits on the disk; views
//! waiting to be written are coalesced to the latest. `flush` waits (bounded) for them all.

use std::path::PathBuf;
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Mutex, OnceLock};
use std::time::Duration;

enum Job {
    View(String, View),
    Backup(serde_json::Value, u64),
    Flush(Sender<()>),
}

fn writer() -> &'static Mutex<Sender<Job>> {
    static W: OnceLock<Mutex<Sender<Job>>> = OnceLock::new();
    W.get_or_init(|| {
        let (tx, rx) = channel();
        std::thread::Builder::new()
            .name("corgi-persist".into())
            .spawn(move || work(rx))
            .expect("a thread for writing files");
        Mutex::new(tx)
    })
}

fn send(job: Job) {
    if let Ok(w) = writer().lock() {
        let _ = w.send(job);
    }
}

fn work(rx: Receiver<Job>) {
    while let Ok(first) = rx.recv() {
        // Everything queued now: views coalesced (the latest per session), the rest in order.
        let mut jobs = vec![first];
        jobs.extend(rx.try_iter());
        let mut views: Vec<(String, View)> = Vec::new();
        for job in jobs {
            match job {
                Job::View(session, v) => match views.iter_mut().find(|(s, _)| *s == session) {
                    Some(slot) => slot.1 = v,
                    None => views.push((session, v)),
                },
                Job::Backup(doc, version) => write_backup(&doc, version),
                Job::Flush(done) => {
                    for (s, v) in views.drain(..) {
                        write_view(&s, &v);
                    }
                    let _ = done.send(());
                }
            }
        }
        for (s, v) in views {
            write_view(&s, &v);
        }
    }
}

/// Wait (up to `limit`) for everything asked so far to be written.
pub fn flush(limit: Duration) {
    let (tx, rx) = channel();
    send(Job::Flush(tx));
    let _ = rx.recv_timeout(limit);
}

const BACKUPS: usize = 20;

fn view_path(session: &str) -> PathBuf {
    crate::paths::state_dir().join(format!("view-{session}.json"))
}

/// What this client looks at, and how its tree is folded.
#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct View {
    #[serde(default)]
    pub workspace: String,
    #[serde(default)]
    pub fold: crate::tree::Fold,
    /// Set by dragging or resizing the sidebar; overrides `sidebar.width`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sidebar_width: Option<u16>,
}

pub fn load_view(session: &str) -> View {
    std::fs::read(view_path(session))
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default()
}

pub fn save_view(session: &str, view: &View) {
    send(Job::View(session.to_string(), view.clone()));
}

fn write_view(session: &str, view: &View) {
    let path = view_path(session);
    let body = serde_json::to_string(view).unwrap_or_default();
    if let Err(e) = write_atomic(&path, body.as_bytes()) {
        tracing::warn!("saving {}: {e}", path.display());
    }
}

/// Keep the last few saved documents, newest last, for recovering a layout by hand.
pub fn backup(doc: &serde_json::Value, version: u64) {
    send(Job::Backup(doc.clone(), version));
}

fn write_backup(doc: &serde_json::Value, version: u64) {
    let dir = crate::paths::state_dir().join("backups");
    let run = || -> std::io::Result<()> {
        std::fs::create_dir_all(&dir)?;
        let ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis();
        write_atomic(
            &dir.join(format!("{ms}-v{version}.json")),
            serde_json::to_string_pretty(doc)?.as_bytes(),
        )?;
        let mut files: Vec<PathBuf> = std::fs::read_dir(&dir)?
            .filter_map(|e| e.ok().map(|e| e.path()))
            .collect();
        files.sort();
        for old in files.iter().take(files.len().saturating_sub(BACKUPS)) {
            std::fs::remove_file(old)?;
        }
        Ok(())
    };
    if let Err(e) = run() {
        tracing::warn!("layout backup in {}: {e}", dir.display());
    }
}

fn write_atomic(path: &std::path::Path, bytes: &[u8]) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, bytes)?;
    std::fs::rename(tmp, path)
}
