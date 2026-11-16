//! One framed connection to herdr-core. A reader task routes frames by
//! channel: replies on channel 0 to their callers, stream bytes to the
//! attach or events handle that owns the channel. A writer task sends.

use crate::error::Error;
use crate::events::{Applied, Lines, Notification, Snapshot};
use crate::types::{AttachResult, CloseReason, Display, Doc, HelloResult, SubscribeResult, Terminal};
use crate::wire::{self, AUTOMATIC_INPUT, CLOSE, CREDIT, DATA, FACTS, FOCUS, RESIZE};
use serde::de::DeserializeOwned;
use serde_json::{Value, json};
use std::collections::{HashMap, VecDeque};
use std::ffi::OsString;
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt, BufReader};
use tokio::sync::{mpsc, oneshot, watch};

const HELLO_TIMEOUT: Duration = Duration::from_secs(15);
/// Extra credit for the events channel on top of the initial 256 KiB, as herdr-app does.
const EVENTS_CREDIT: u32 = 1 << 20;

/// How to reach the core.
#[derive(Clone, Debug)]
pub enum Transport {
    /// Run `hc connect`: it starts the core if needed and relays bytes unchanged.
    /// `ssh host hc connect` works the same way, with `program`/`args` set accordingly.
    Spawn {
        program: PathBuf,
        args: Vec<OsString>,
        cwd: Option<PathBuf>,
        env: Vec<(OsString, OsString)>,
    },
    /// The core's Unix socket directly.
    #[cfg(unix)]
    Socket(PathBuf),
}

impl Transport {
    /// `hc connect` from PATH, for the given core session.
    pub fn hc(session: Option<&str>) -> Transport {
        let env = session
            .map(|s| vec![("HC_SESSION".into(), s.into())])
            .unwrap_or_default();
        Transport::Spawn {
            program: "hc".into(),
            args: vec!["connect".into()],
            cwd: None,
            env,
        }
    }
}

/// Something that arrived on an attach or events channel.
#[derive(Clone, Debug, PartialEq)]
pub enum StreamFrame {
    Data(Vec<u8>),
    /// Grid size, for observers that asked for resize notifications.
    Resize(u16, u16),
    /// The stream is over. `None` when we detached (the core's empty CLOSE).
    Close(Option<CloseReason>),
}

type Reply = oneshot::Sender<Result<Value, Error>>;

#[derive(Default)]
struct State {
    open: bool,
    pending: HashMap<u64, Reply>,
    routes: HashMap<u32, mpsc::UnboundedSender<StreamFrame>>,
    /// Frames for a channel whose owner hasn't registered yet (attach reply still in flight to the caller).
    early: HashMap<u32, Vec<StreamFrame>>,
    /// Channels we let go of: drop their remaining frames until the core's CLOSE.
    dropped: std::collections::HashSet<u32>,
}

struct Inner {
    hello: HelloResult,
    out: mpsc::UnboundedSender<Vec<u8>>,
    state: Mutex<State>,
    next_id: AtomicU64,
    closed: watch::Sender<bool>,
    child: Mutex<Option<tokio::process::Child>>,
    tasks: Mutex<Vec<tokio::task::AbortHandle>>,
}

impl Inner {
    /// The line is gone: fail pending calls, end every stream, wake `closed()`.
    fn finish(&self) {
        let mut st = self.state.lock().unwrap();
        if !st.open {
            return;
        }
        st.open = false;
        for (_, reply) in st.pending.drain() {
            let _ = reply.send(Err(Error::Closed));
        }
        st.routes.clear();
        st.early.clear();
        drop(st);
        self.closed.send_replace(true);
        tracing::info!("herdr-core connection closed");
    }
}

/// A connection. Cheap to clone; all clones share it.
#[derive(Clone)]
pub struct Client {
    inner: Arc<Inner>,
}

type Reader = Box<dyn AsyncRead + Unpin + Send>;
type Writer = Box<dyn AsyncWrite + Unpin + Send>;

impl Client {
    /// Connect, say hello, switch to framed mode, and check the core can serve corgi.
    pub async fn connect(transport: &Transport) -> Result<Client, Error> {
        let (reader, mut writer, child, stderr) = open(transport).await?;
        let mut reader = BufReader::new(reader);
        let hello = json!({"jsonrpc": "2.0", "id": 0, "method": "hello",
                           "params": {"framed": true, "generation": crate::GENERATION}});
        let mut line = String::new();
        let handshake = async {
            writer.write_all(format!("{hello}\n").as_bytes()).await?;
            writer.flush().await?;
            reader.read_line(&mut line).await
        };
        let n = match tokio::time::timeout(HELLO_TIMEOUT, handshake).await {
            Err(_) => return Err(Error::Timeout),
            Ok(r) => r.unwrap_or(0),
        };
        if n == 0 {
            // `hc connect` exited: its stderr says why (bad session, core won't start, ...).
            let why = match stderr {
                Some(e) => tokio::time::timeout(Duration::from_secs(2), e)
                    .await
                    .ok()
                    .and_then(Result::ok),
                None => None,
            };
            return Err(Error::Connect(
                describe(why.as_deref().unwrap_or("")).unwrap_or("no answer to hello".into()),
            ));
        }
        let reply: Value = serde_json::from_str(&line)?;
        let hello: HelloResult = match reply_result(reply) {
            Ok(v) => serde_json::from_value(v)?,
            Err(e) => return Err(e),
        };
        crate::check(&hello)?;
        if !hello.framed {
            return Err(Error::Protocol("core did not switch to framed mode".into()));
        }
        tracing::info!(version = %hello.version, core = %hello.core.name, "connected to herdr-core");

        let (out, out_rx) = mpsc::unbounded_channel();

        let inner = Arc::new(Inner {
            hello,
            out,
            state: Mutex::new(State {
                open: true,
                ..State::default()
            }),
            next_id: AtomicU64::new(1),
            closed: watch::channel(false).0,
            child: Mutex::new(child),
            tasks: Mutex::new(Vec::new()),
        });
        let w = tokio::spawn(write_loop(writer, out_rx));
        let r = tokio::spawn(read_loop(reader, Arc::downgrade(&inner)));
        inner.tasks.lock().unwrap().extend([w.abort_handle(), r.abort_handle()]);
        Ok(Client { inner })
    }

    /// Connect, retrying every `every` while the core is unreachable (restarting,
    /// updating, not started yet). Gives up only on errors retrying can't fix:
    /// a core that can't serve this client, or a missing `hc`.
    pub async fn reconnect(transport: &Transport, every: Duration) -> Result<Client, Error> {
        loop {
            match Client::connect(transport).await {
                Ok(c) => return Ok(c),
                Err(e @ Error::Mismatch(_)) => return Err(e),
                Err(Error::Connect(m)) if m.contains("not found") => return Err(Error::Connect(m)),
                Err(e) => {
                    tracing::info!("core unreachable, retrying: {e}");
                    tokio::time::sleep(every).await;
                }
            }
        }
    }

    pub fn hello(&self) -> &HelloResult {
        &self.inner.hello
    }

    pub fn is_open(&self) -> bool {
        self.inner.state.lock().unwrap().open
    }

    /// Resolves when the connection ends (core stopped or restarted, `hc connect` died).
    pub async fn closed(&self) {
        let mut rx = self.inner.closed.subscribe();
        let _ = rx.wait_for(|c| *c).await;
    }

    /// Drop the line now (e.g. a heartbeat watchdog decided it's dead). Terminals keep running.
    pub fn close(&self) {
        for t in self.inner.tasks.lock().unwrap().drain(..) {
            t.abort();
        }
        if let Some(c) = self.inner.child.lock().unwrap().as_mut() {
            let _ = c.start_kill();
        }
        self.inner.finish();
    }

    fn send(&self, channel: u32, kind: u8, payload: &[u8]) {
        let _ = self.inner.out.send(wire::encode(channel, kind, payload));
    }

    /// A JSON-RPC call on channel 0. Calls run concurrently.
    pub async fn call(&self, method: &str, params: Value) -> Result<Value, Error> {
        let id = self.inner.next_id.fetch_add(1, Ordering::Relaxed);
        let (tx, rx) = oneshot::channel();
        {
            let mut st = self.inner.state.lock().unwrap();
            if !st.open {
                return Err(Error::Closed);
            }
            st.pending.insert(id, tx);
        }
        let msg = json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params});
        self.send(0, DATA, msg.to_string().as_bytes());
        rx.await.unwrap_or(Err(Error::Closed))
    }

    pub async fn call_as<T: DeserializeOwned>(&self, method: &str, params: Value) -> Result<T, Error> {
        Ok(serde_json::from_value(self.call(method, params).await?)?)
    }

    /// `terminal.run`. `params` are merged over `{argv}`; see the schema for the options.
    pub async fn run(&self, argv: &[&str], mut params: Value) -> Result<Terminal, Error> {
        params["argv"] = json!(argv);
        self.call_as("terminal.run", params).await
    }

    pub async fn get(&self, terminal: &str) -> Result<Terminal, Error> {
        self.call_as("terminal.get", json!({"terminal": terminal})).await
    }

    pub async fn doc_get(&self, name: &str, on: Option<&str>) -> Result<Doc, Error> {
        self.call_as("doc.get", json!({"name": name, "on": on})).await
    }

    /// Compare-and-swap write; a stale `version` fails with kind `conflict`. Returns the new version.
    pub async fn doc_set(&self, name: &str, value: &Value, version: u64, on: Option<&str>) -> Result<u64, Error> {
        let r = self
            .call(
                "doc.set",
                json!({"name": name, "value": value, "version": version, "on": on}),
            )
            .await?;
        r.get("version")
            .and_then(Value::as_u64)
            .ok_or_else(|| Error::Protocol(format!("doc.set reply {r}")))
    }

    /// Claim a channel: frames that arrived before this call are delivered first.
    fn route(&self, channel: u32) -> mpsc::UnboundedReceiver<StreamFrame> {
        let (tx, rx) = mpsc::unbounded_channel();
        let mut st = self.inner.state.lock().unwrap();
        for f in st.early.remove(&channel).unwrap_or_default() {
            let _ = tx.send(f);
        }
        if st.open {
            st.routes.insert(channel, tx);
        }
        rx
    }

    fn unroute(&self, channel: u32) {
        let mut st = self.inner.state.lock().unwrap();
        st.routes.remove(&channel);
        st.early.remove(&channel);
        st.dropped.insert(channel);
    }

    /// Attach to a terminal's stream. The first frames are its first paint (starting with CAN).
    pub async fn attach(&self, terminal: &str, cols: u16, rows: u16, opts: Value) -> Result<Attach, Error> {
        let mut params = json!({"terminal": terminal, "cols": cols, "rows": rows});
        if let (Value::Object(p), Value::Object(o)) = (&mut params, opts) {
            p.extend(o);
        }
        let r: AttachResult = self.call_as("terminal.attach", params).await?;
        let rx = self.route(r.channel);
        Ok(Attach {
            channel: r.channel,
            automatic_input: r.automatic_input && self.inner.hello.has("automatic_input"),
            terminal: r.terminal,
            rx,
            client: self.clone(),
            done: false,
        })
    }

    /// Subscribe to events; `filter` is `{terminal?, children_of?, labels?}`.
    pub async fn subscribe(&self, filter: Value) -> Result<Events, Error> {
        let r: SubscribeResult = self.call_as("events.subscribe", filter).await?;
        let rx = self.route(r.channel);
        self.send(r.channel, CREDIT, &EVENTS_CREDIT.to_le_bytes());
        Ok(Events {
            snapshot: Snapshot::new(r.seq, r.terminals),
            channel: r.channel,
            rx,
            lines: Lines::default(),
            queue: VecDeque::new(),
            client: self.clone(),
        })
    }
}

/// The sending half of an attach: cheap to clone, usable while another task reads the stream.
#[derive(Clone)]
pub struct AttachTx {
    pub channel: u32,
    /// Both the core and this path take AUTOMATIC_INPUT frames.
    pub automatic_input: bool,
    client: Client,
}

impl AttachTx {
    /// Keys and pastes typed by the person.
    pub fn input(&self, bytes: &[u8]) {
        if !bytes.is_empty() {
            self.client.send(self.channel, DATA, bytes);
        }
    }

    /// Replies the viewer's emulator generated (size reports, graphics answers): they don't take size ownership.
    pub fn automatic_input(&self, bytes: &[u8]) {
        if bytes.is_empty() {
            return;
        }
        let kind = if self.automatic_input { AUTOMATIC_INPUT } else { DATA };
        self.client.send(self.channel, kind, bytes);
    }

    pub fn resize(&self, cols: u16, rows: u16) {
        self.client
            .send(self.channel, RESIZE, &wire::resize_payload(cols, rows));
    }

    pub fn focus(&self, focused: bool) {
        self.client.send(self.channel, FOCUS, &[focused as u8]);
    }

    /// Let the core send `bytes` more output (after `Attach::recv_uncredited` handed them over
    /// and they've been used): what it may have in flight stays within its credit.
    pub fn credit(&self, bytes: u32) {
        if bytes > 0 {
            self.client.send(self.channel, CREDIT, &bytes.to_le_bytes());
        }
    }

    pub fn facts(&self, display: &Display) {
        self.client.send(
            self.channel,
            FACTS,
            serde_json::to_vec(display).unwrap_or_default().as_slice(),
        );
    }
}

/// A terminal's live stream.
pub struct Attach {
    pub channel: u32,
    /// The record as of attaching.
    pub terminal: Terminal,
    /// Both the core and this path take AUTOMATIC_INPUT frames.
    pub automatic_input: bool,
    rx: mpsc::UnboundedReceiver<StreamFrame>,
    client: Client,
    done: bool,
}

impl Attach {
    /// Next frame; output bytes are credited as they're handed over. `None` once the connection is gone.
    pub async fn recv(&mut self) -> Option<StreamFrame> {
        let f = self.rx.recv().await?;
        match &f {
            StreamFrame::Data(b) => self.client.send(self.channel, CREDIT, &(b.len() as u32).to_le_bytes()),
            StreamFrame::Close(_) => self.done = true,
            StreamFrame::Resize(..) => {}
        }
        Some(f)
    }

    /// Next frame, without crediting its output: the caller credits it (`AttachTx::credit`)
    /// once it's used, so output waiting anywhere on the way stays within the credit.
    pub async fn recv_uncredited(&mut self) -> Option<StreamFrame> {
        let f = self.rx.recv().await?;
        if let StreamFrame::Close(_) = &f {
            self.done = true;
        }
        Some(f)
    }

    /// A sender for this stream, for another task to type, resize, focus with.
    pub fn tx(&self) -> AttachTx {
        AttachTx {
            channel: self.channel,
            automatic_input: self.automatic_input,
            client: self.client.clone(),
        }
    }

    pub fn input(&self, bytes: &[u8]) {
        self.tx().input(bytes)
    }

    pub fn automatic_input(&self, bytes: &[u8]) {
        self.tx().automatic_input(bytes)
    }

    pub fn resize(&self, cols: u16, rows: u16) {
        self.tx().resize(cols, rows)
    }

    pub fn focus(&self, focused: bool) {
        self.tx().focus(focused)
    }

    pub fn facts(&self, display: &Display) {
        self.tx().facts(display)
    }

    /// Leave the terminal running and stop viewing it. Waits briefly for the core's CLOSE.
    pub async fn detach(mut self) {
        if self.done {
            return;
        }
        self.client.send(self.channel, CLOSE, b"");
        let wait = async {
            while let Some(f) = self.rx.recv().await {
                if matches!(f, StreamFrame::Close(_)) {
                    break;
                }
            }
        };
        let _ = tokio::time::timeout(Duration::from_secs(2), wait).await;
        self.done = true;
    }
}

impl Drop for Attach {
    fn drop(&mut self) {
        if !self.done {
            self.client.send(self.channel, CLOSE, b"");
        }
        self.client.unroute(self.channel);
    }
}

/// The event stream with its snapshot kept current.
pub struct Events {
    pub snapshot: Snapshot,
    pub channel: u32,
    rx: mpsc::UnboundedReceiver<StreamFrame>,
    lines: Lines,
    queue: VecDeque<Notification>,
    client: Client,
}

impl Events {
    /// Next notification, already applied to `snapshot`. `None` when the stream or connection ends.
    pub async fn next(&mut self) -> Option<Applied> {
        loop {
            if let Some(n) = self.queue.pop_front() {
                return Some(self.snapshot.apply(n));
            }
            match self.rx.recv().await? {
                StreamFrame::Data(b) => {
                    self.client.send(self.channel, CREDIT, &(b.len() as u32).to_le_bytes());
                    let queue = &mut self.queue;
                    self.lines.push(&b, |line| match Notification::parse(line) {
                        Some(n) => queue.push_back(n),
                        None => tracing::debug!(line = %String::from_utf8_lossy(line), "unknown notification"),
                    });
                }
                StreamFrame::Close(_) => return None,
                StreamFrame::Resize(..) => {}
            }
        }
    }
}

impl Drop for Events {
    fn drop(&mut self) {
        self.client.send(self.channel, CLOSE, b"");
        self.client.unroute(self.channel);
    }
}

async fn open(t: &Transport) -> Result<(Reader, Writer, Option<tokio::process::Child>, Option<StderrTask>), Error> {
    match t {
        Transport::Spawn {
            program,
            args,
            cwd,
            env,
        } => {
            let mut cmd = tokio::process::Command::new(program);
            cmd.args(args)
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .kill_on_drop(true);
            if let Some(dir) = cwd {
                cmd.current_dir(dir);
            }
            cmd.envs(env.iter().map(|(k, v)| (k, v)));
            let mut child = cmd.spawn().map_err(|e| {
                Error::Connect(if e.kind() == std::io::ErrorKind::NotFound {
                    format!("`{}` not found; install herdr-core: curl -fsSL https://releases.herdr.dev/core/install.sh | sh", program.display())
                } else {
                    format!("starting {}: {e}", program.display())
                })
            })?;
            let stdin = child.stdin.take().unwrap();
            let stdout = child.stdout.take().unwrap();
            let mut stderr = child.stderr.take().unwrap();
            let task = tokio::spawn(async move {
                let mut s = String::new();
                let _ = stderr.read_to_string(&mut s).await;
                s
            });
            Ok((Box::new(stdout), Box::new(stdin), Some(child), Some(task)))
        }
        #[cfg(unix)]
        Transport::Socket(path) => {
            let s = tokio::net::UnixStream::connect(path)
                .await
                .map_err(|e| Error::Connect(format!("{}: {e}", path.display())))?;
            let (r, w) = s.into_split();
            Ok((Box::new(r), Box::new(w), None, None))
        }
    }
}

type StderrTask = tokio::task::JoinHandle<String>;

/// `hc` prints `{"error":{"message":..,"hint":..}}` when it can't connect; prefer that over raw text.
fn describe(stderr: &str) -> Option<String> {
    let text = stderr.trim();
    if text.is_empty() {
        return None;
    }
    let err = serde_json::from_str::<Value>(text)
        .ok()
        .and_then(|v| v.get("error").cloned());
    Some(match err {
        Some(e) => {
            let msg = e.get("message").and_then(Value::as_str).unwrap_or(text);
            match e.get("hint").and_then(Value::as_str) {
                Some(h) => format!("{msg} ({h})"),
                None => msg.to_string(),
            }
        }
        None => text.to_string(),
    })
}

fn reply_result(reply: Value) -> Result<Value, Error> {
    if let Some(e) = reply.get("error") {
        return Err(Error::Rpc {
            code: e.get("code").and_then(Value::as_i64).unwrap_or(0),
            message: e.get("message").and_then(Value::as_str).unwrap_or("error").to_owned(),
            kind: e.pointer("/data/kind").and_then(Value::as_str).map(str::to_owned),
            hint: e.pointer("/data/hint").and_then(Value::as_str).map(str::to_owned),
        });
    }
    Ok(reply.get("result").cloned().unwrap_or(Value::Null))
}

async fn write_loop(mut w: Writer, mut rx: mpsc::UnboundedReceiver<Vec<u8>>) {
    while let Some(frame) = rx.recv().await {
        if w.write_all(&frame).await.is_err() {
            break;
        }
        // Batch whatever else is queued before flushing.
        while let Ok(more) = rx.try_recv() {
            if w.write_all(&more).await.is_err() {
                return;
            }
        }
        if w.flush().await.is_err() {
            break;
        }
    }
}

async fn read_loop(mut r: BufReader<Reader>, inner: std::sync::Weak<Inner>) {
    loop {
        let frame = wire::read_frame(&mut r).await;
        let Some(inner) = inner.upgrade() else { break };
        let (channel, kind, payload) = match frame {
            Ok(Some(f)) => f,
            Ok(None) => break,
            Err(e) => {
                tracing::warn!("core connection: {e}");
                break;
            }
        };
        if channel == 0 {
            if kind == DATA {
                deliver_reply(&inner, &payload);
            }
            continue;
        }
        let f = match kind {
            DATA => StreamFrame::Data(payload),
            RESIZE => match wire::parse_resize(&payload) {
                Some((c, r)) => StreamFrame::Resize(c, r),
                None => continue,
            },
            CLOSE => StreamFrame::Close(if payload.is_empty() {
                None
            } else {
                serde_json::from_slice(&payload).ok()
            }),
            _ => continue,
        };
        let close = matches!(f, StreamFrame::Close(_));
        let mut st = inner.state.lock().unwrap();
        if st.dropped.contains(&channel) {
            if close {
                st.dropped.remove(&channel);
            }
            continue;
        }
        match st.routes.get(&channel) {
            Some(tx) => {
                if tx.send(f).is_err() || close {
                    st.routes.remove(&channel);
                }
            }
            None => st.early.entry(channel).or_default().push(f),
        }
    }
    if let Some(inner) = inner.upgrade() {
        inner.finish();
    }
}

fn deliver_reply(inner: &Inner, payload: &[u8]) {
    let Ok(msg) = serde_json::from_slice::<Value>(payload) else {
        return;
    };
    let Some(id) = msg.get("id").and_then(Value::as_u64) else {
        tracing::debug!(%msg, "core message on channel 0 without id");
        return;
    };
    if let Some(reply) = inner.state.lock().unwrap().pending.remove(&id) {
        let _ = reply.send(reply_result(msg));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn connect_errors_prefer_the_cores_message_and_hint() {
        let e = r#"{"error":{"hint":"use a shorter HC_SESSION","kind":"usage","message":"socket path too long"}}"#;
        assert_eq!(describe(e).unwrap(), "socket path too long (use a shorter HC_SESSION)");
        assert_eq!(describe("plain failure\n").unwrap(), "plain failure");
        assert_eq!(describe("  "), None);
    }

    #[test]
    fn rpc_errors_carry_kind_and_hint() {
        let e = reply_result(
            json!({"error":{"code":-32000,"message":"no terminal t9","data":{"kind":"not_found","hint":"see hc ls"}}}),
        )
        .unwrap_err();
        assert_eq!(e.kind(), Some("not_found"));
        assert_eq!(e.to_string(), "no terminal t9 (see hc ls)");
    }
}
