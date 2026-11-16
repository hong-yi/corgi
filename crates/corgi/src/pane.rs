//! One pane: a core terminal, the emulator that draws it, and the attach
//! stream that feeds it. Each (re)attach gets a fresh emulator and a new
//! generation number; frames from older generations are ignored.

use crate::app::Msg;
use corgi_vt::Vt;
use hc_proto::{AttachTx, Client, CloseReason, Display, StreamFrame, Terminal};
use serde_json::json;
use tokio::sync::mpsc::UnboundedSender;
use tokio::task::JoinHandle;

#[derive(Clone, Debug, PartialEq)]
pub enum State {
    /// Waiting for the attach reply.
    Attaching,
    Live,
    /// The program ended for good.
    Exited(CloseReason),
    /// A kept terminal lost its program; core is starting a new shell.
    Recovering(String),
    /// No connection to core right now.
    Offline,
    /// The terminal corgi runs in: never attached, it would show itself.
    Myself,
}

pub struct Pane {
    pub addr: String,
    pub vt: Vt,
    pub tx: Option<AttachTx>,
    pub state: State,
    /// Last record from core (title, status, foreground).
    pub record: Option<Terminal>,
    pub title: Option<String>,
    /// Size as laid out on screen.
    pub size: (u16, u16),
    epoch: u64,
    task: Option<JoinHandle<()>>,
    focused: bool,
    /// Typed while attaching; sent once the stream is up so no keystroke is lost.
    pending: Vec<u8>,
    /// Output used since the last credit (for this stream).
    owed: u32,
}

impl Pane {
    pub fn new(addr: String, size: (u16, u16)) -> anyhow::Result<Pane> {
        Ok(Pane {
            addr,
            vt: Vt::new(size.0, size.1)?,
            tx: None,
            state: State::Offline,
            record: None,
            title: None,
            size,
            epoch: 0,
            task: None,
            owed: 0,
            focused: true,
            pending: Vec::new(),
        })
    }

    pub fn is_current(&self, epoch: u64) -> bool {
        epoch == self.epoch
    }

    /// Attach (again). `resumed`: after a dropped line, so we don't grab the size before the person acts.
    pub fn attach(&mut self, client: &Client, display: Option<&Display>, resumed: bool, out: UnboundedSender<Msg>) {
        self.detach();
        self.epoch += 1;
        self.owed = 0;
        let epoch = self.epoch;
        self.state = State::Attaching;
        let (client, addr, (cols, rows)) = (client.clone(), self.addr.clone(), self.size);
        let opts = json!({"history": 2000, "display": display, "resumed": resumed});
        self.task = Some(tokio::spawn(async move {
            let mut a = match client.attach(&addr, cols, rows, opts).await {
                Ok(a) => a,
                Err(e) => {
                    let _ = out.send(Msg::Attached {
                        pane: addr,
                        epoch,
                        result: Err(e),
                    });
                    return;
                }
            };
            let ok = Ok((a.tx(), a.terminal.clone(), (cols, rows)));
            let _ = out.send(Msg::Attached {
                pane: addr.clone(),
                epoch,
                result: ok,
            });
            loop {
                // Credited by the app once the emulator has it (`Pane::used`).
                let f = a.recv_uncredited().await;
                let end = !matches!(f, Some(StreamFrame::Data(_) | StreamFrame::Resize(..)));
                if out
                    .send(Msg::Stream {
                        pane: addr.clone(),
                        epoch,
                        frame: f,
                    })
                    .is_err()
                    || end
                {
                    break;
                }
            }
        }));
    }

    /// Stop viewing (the terminal keeps running). Dropping the stream sends CLOSE.
    pub fn detach(&mut self) {
        if let Some(t) = self.task.take() {
            t.abort();
        }
        self.tx = None;
    }

    /// The attach reply arrived: start a fresh screen for the first paint.
    pub fn attached(&mut self, tx: AttachTx, record: Terminal, asked: (u16, u16)) -> anyhow::Result<()> {
        self.vt = Vt::new(asked.0, asked.1)?;
        self.record = Some(record);
        if self.size != asked {
            // Laid out again while the attach was in flight.
            self.vt.resize(self.size.0, self.size.1)?;
            tx.resize(self.size.0, self.size.1);
        }
        tx.focus(self.focused);
        tx.input(&std::mem::take(&mut self.pending));
        self.tx = Some(tx);
        self.state = State::Live;
        Ok(())
    }

    /// Output from the program.
    /// Output the emulator has had, owed back to the core as credit (sent in batches).
    pub fn used(&mut self, n: usize) {
        self.owed = self.owed.saturating_add(n as u32);
    }

    /// Give the core the credit owed: it may send that much more.
    pub fn credit(&mut self) {
        if self.owed > 0
            && let Some(tx) = &self.tx
        {
            tx.credit(std::mem::take(&mut self.owed));
        }
    }

    pub fn output(&mut self, bytes: &[u8]) {
        record(&self.addr, self.epoch, bytes);
        self.vt.write(bytes);
        self.flush_replies();
        if let Some(t) = self.vt.take_title() {
            self.title = Some(t);
        }
    }

    pub fn closed(&mut self, reason: Option<CloseReason>) {
        self.tx = None;
        self.pending.clear();
        self.task = None;
        self.state = match reason {
            Some(r) if r.is_final() => State::Exited(r),
            Some(r) => State::Recovering(r.reason.unwrap_or_default()),
            None => State::Offline,
        };
    }

    pub fn resize(&mut self, size: (u16, u16)) -> anyhow::Result<()> {
        if size == self.size {
            return Ok(());
        }
        self.size = size;
        self.vt.resize(size.0, size.1)?;
        if let Some(tx) = &self.tx {
            tx.resize(size.0, size.1);
        }
        self.flush_replies();
        Ok(())
    }

    pub fn focus(&mut self, focused: bool) {
        self.focused = focused;
        if let Some(tx) = &self.tx {
            tx.focus(focused);
        }
    }

    /// Typed keys or a paste.
    pub fn input(&mut self, bytes: &[u8]) {
        if bytes.is_empty() {
            return;
        }
        self.vt.scroll_bottom();
        match (&self.tx, &self.state) {
            (Some(tx), _) => tx.input(bytes),
            (None, State::Attaching) => self.pending.extend_from_slice(bytes),
            _ => {}
        }
    }

    /// Emulator-generated answers go back as automatic input, never as typing.
    fn flush_replies(&mut self) {
        let r = self.vt.take_replies();
        if let Some(tx) = &self.tx {
            tx.automatic_input(&r);
        }
    }

    /// What to call this pane in the tab bar.
    pub fn name(&self) -> String {
        let fg = self
            .record
            .as_ref()
            .and_then(|r| r.foreground.as_ref())
            .map(|f| f.name.clone());
        let argv0 = self.record.as_ref().and_then(|r| r.argv.first().cloned());
        self.title
            .clone()
            .filter(|t| !t.is_empty())
            .or(fg)
            .or(argv0)
            .unwrap_or_else(|| self.addr.clone())
    }
}

impl Drop for Pane {
    fn drop(&mut self) {
        self.detach();
    }
}

/// Debugging aid: with `CORGI_RECORD=<dir>`, every byte a pane receives is appended to
/// `<dir>/<addr>-<attach>.bin`, for replaying through `corgi-vt`'s `dump` example.
fn record(addr: &str, epoch: u64, bytes: &[u8]) {
    use std::io::Write;
    let Some(dir) = std::env::var_os("CORGI_RECORD") else {
        return;
    };
    let path = std::path::Path::new(&dir).join(format!("{}-{epoch}.bin", addr.replace('/', "_")));
    let file = std::fs::OpenOptions::new().create(true).append(true).open(&path);
    if let Ok(mut f) = file {
        let _ = f.write_all(bytes);
    }
}
