//! The app loop. One task owns all state; everything else (input thread,
//! attach streams, events, spawns, saves, reconnects) sends it a [`Msg`].
//! Draws are coalesced to at most one per frame interval.
//!
//! The layout is the `corgi` document in core ([`Doc`]). Panes of the
//! visible window are attached; others are not, and attach when shown.

use crate::commands::Action;
use crate::config::{self, Config, SidebarMode};
use crate::keys::Keymap;
use crate::layout::{Dir, Preset};
use crate::model::{self, Doc, Reconciled, Window};
use crate::pane::{Pane, State};
use crate::theme::Theme;
use crate::{input, persist, status, term};
use crossterm::event::{Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseButton, MouseEventKind};
use hc_proto::{Applied, AttachTx, Client, Display, Error, StreamFrame, Terminal, Transport};
use ratatui::layout::Rect;
use serde_json::{Value, json};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::time::{Duration, Instant};
use tokio::sync::mpsc::{self, UnboundedSender};

const FRAME: Duration = Duration::from_millis(8);
/// Core sends a heartbeat every 15 s; three missed means the line is dead.
const SILENCE: Duration = Duration::from_secs(45);
const NOTE_FOR: Duration = Duration::from_secs(3);
/// How often slow-changing things (git branches) are looked up.
const TICK: Duration = Duration::from_secs(10);

/// How the panes tile (see `App::tiling`).
pub(super) struct Tiling {
    pub area: Rect,
    /// Cells between panes.
    pub sep: u16,
    /// Each pane in its own box.
    pub boxes: bool,
}

/// A click target in the status line.
#[derive(Clone, Debug, PartialEq)]
enum TabHit {
    Window(String),
    NewWindow,
    /// The usage segments: their details.
    Usage,
    /// The narrow layout's "switch" button: the chooser.
    Switch,
}

/// Where clickable things were last drawn.
#[derive(Default)]
struct Hits {
    tabs: Vec<(Rect, TabHit)>,
    sidebar: Vec<(Rect, crate::sidebar::Hit)>,
    /// Toasts, and the pane each is about.
    toasts: Vec<(Rect, Option<String>)>,
}

/// What a new terminal is for, decided before it exists.
#[derive(Clone, Debug)]
pub enum Purpose {
    Workspace {
        ws: String,
        win: String,
        folder: String,
        name: Option<String>,
    },
    Window {
        ws: String,
        win: String,
        name: Option<String>,
    },
    Split {
        ws: String,
        win: String,
        target: String,
        dir: Dir,
        pct: u8,
    },
}

impl Purpose {
    fn ids(&self) -> (&str, &str) {
        match self {
            Purpose::Workspace { ws, win, .. } | Purpose::Window { ws, win, .. } | Purpose::Split { ws, win, .. } => {
                (ws, win)
            }
        }
    }
}

pub enum Msg {
    Input(Event),
    Attached {
        pane: String,
        epoch: u64,
        result: Result<(AttachTx, Terminal, (u16, u16)), Error>,
    },
    /// `frame: None`: the connection dropped under the stream.
    Stream {
        pane: String,
        epoch: u64,
        frame: Option<StreamFrame>,
    },
    Snapshot(BTreeMap<String, Terminal>),
    Event(Applied, Option<Terminal>),
    DocLoaded(Result<(Value, u64), Error>),
    Saved(Result<u64, Error>),
    Spawned(Purpose, Result<Terminal, Error>),
    /// Folder → git branch, → its repository's own checkout, → the checkout it's in.
    Branches(
        HashMap<String, String>,
        HashMap<String, String>,
        HashMap<String, String>,
    ),
    /// No event or heartbeat for too long.
    Silent,
    /// A popup's terminal was started (or not), for what was asked.
    PopupStarted(Result<Terminal, Error>, popup::Ask),
    /// Something to say in the status line (a background job's result).
    Note(String),
    /// A repository's worktrees (or why not), for the folder and machine asked about.
    Worktrees(Result<Vec<crate::worktrees::Worktree>, String>, String, Option<String>),
    Disconnected,
    Connected(Result<Client, Error>),
    /// Machines and templates, for an open dialog.
    Lists(dialogs::Lists),
    /// A worktree for a new workspace was made (or not).
    Worktree(dialogs::Made),
    /// A usage segment's latest reading: (generation, segment, target revision, reading).
    Usage(u64, usize, u64, Option<crate::usage::Reading>),
    /// Core's other machines, as they are now.
    Machines(Vec<crate::machines::Machine>),
    /// A machine call (add, connect, rename, remove) finished.
    Machine(machines::Done),
    /// An image was put beside a pane's terminal (where it landed), or not (why).
    ImagePut(String, Result<String, String>),
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Conn {
    Up,
    Reconnecting,
}

enum Mode {
    Normal,
    /// Since when: the key-hints dialog may wait (`keys.hints = "delay"`).
    Prefix(Instant),
    /// After a repeatable binding: more repeatable keys work without the prefix until then.
    Repeat(Instant),
    /// The tree chooser dialog.
    Chooser(crate::chooser::Chooser),
    /// A key table other than the prefix's (`switch-client -T resize`), until Esc.
    Table(String),
    /// Every key binding (`C-b ?`).
    Keys(crate::keylist::KeyList),
    /// Pane numbers on screen (`C-b q`) until then; a digit picks one.
    Panes(Instant),
    Prompt(Prompt),
    /// A dialog (New Workspace, New Window).
    Dialog(dialogs::Dialog),
    /// An action menu, and what it's for (a sidebar row, a tab), if not the focused pane.
    Menu(crate::menu::Menu, Option<crate::tree::Target>),
    /// Copy mode in a pane.
    Copy(Box<crate::copy::Copy>, String),
    /// A popup has the keys.
    Popup,
    /// The Worktrees dialog.
    Worktrees(Box<worktrees::Shown>),
    /// The Machines dialog.
    Machines(Box<crate::machines::View>),
}

/// What a new terminal runs instead of the machine's shell.
#[derive(Clone, Debug, PartialEq)]
pub enum Run {
    /// A command line, run by the shell.
    Line(String),
    /// A core template (`templates.list`).
    Template(String),
}

struct Prompt {
    label: String,
    text: String,
    then: Then,
    /// Which history entry Up/Down is on (command prompts).
    hist: Option<usize>,
}

enum Then {
    /// y/n question: on `y`, run this.
    Confirm(Box<Action>),
    /// The `:` prompt: run what's typed, put into `template`'s `%%` if there is one.
    Command { template: Option<String> },
}

pub struct Options {
    pub transport: Transport,
    pub session: String,
    /// Show this terminal (putting it in the layout if it isn't).
    pub attach: Option<String>,
    pub config: config::Loaded,
    pub config_path: std::path::PathBuf,
}

struct App {
    transport: Transport,
    session: String,
    client: Client,
    conn: Conn,
    out: UnboundedSender<Msg>,
    core_version: String,
    display: Option<Display>,
    keymap: Keymap,
    /// Commands run at the `:` prompt, oldest first.
    history: Vec<String>,

    doc: Doc,
    doc_version: u64,
    loaded: bool,
    dirty: bool,
    saving: bool,
    /// Had a layout at some point: an empty one now means everything exited.
    booted: bool,
    spawning: usize,
    /// New windows still getting the panes of their initial layout: panes to come, the
    /// arrangement once they're there, and their folder.
    layouts: HashMap<String, (u8, crate::layout::Preset, String)>,
    attach_target: Option<String>,

    /// Which workspace this client shows.
    view: String,
    terminals: BTreeMap<String, Terminal>,
    panes: HashMap<String, Pane>,
    screen: Rect,
    host_focus: bool,
    /// The host has reported focus at least once, so `host_focus` means something.
    focus_known: bool,
    mode: Mode,
    note: Option<(String, Instant)>,
    quit: bool,
    /// Keys pressed before there was a window to take them (startup); replayed once there is.
    early: Vec<KeyEvent>,
    /// The core terminal corgi itself runs in, if any: never attached (it would show itself).
    myself: Option<String>,

    /// What's drawn with: `wide`, or its narrowed copy on a narrow screen.
    config: Config,
    /// The config as loaded.
    wide: Config,
    /// The screen is at most `ui.narrow_width` wide: the narrow layout.
    narrow: bool,
    config_path: std::path::PathBuf,
    theme: Theme,

    /// `sidebar.mode`, toggled at runtime with C-b b.
    sidebar_mode: SidebarMode,
    /// Workspace folder (its `git_key`) → git branch.
    branches: HashMap<String, String>,
    /// Workspace folder → its git repository's own checkout (both `git_key`s).
    repos: HashMap<String, String>,
    /// Folder (its `git_key`) → the git checkout it's in (its top-level folder).
    toplevels: HashMap<String, String>,
    /// A git scan is running (since when); another was asked for meanwhile (it runs when this
    /// one's done).
    scanning: Option<Instant>,
    rescan: bool,
    hits: Hits,
    /// The host window title last set.
    title: String,
    /// How the workspace tree is folded (sidebar and chooser share it).
    fold: crate::tree::Fold,
    /// Width set by dragging or resizing the sidebar (overrides the config).
    sidebar_width: Option<u16>,
    /// The sidebar has the keyboard (entered with C-b ← from the left-most pane).
    sidebar_focus: Option<crate::chooser::Chooser>,
    /// Dragging the sidebar's edge with the mouse.
    dragging: bool,
    toasts: crate::toast::Toasts,
    /// The tab bar's usage segments: what each last read, and when.
    usage: usage::State,
    /// What copy mode copied last, for `C-b ]`.
    paste: Option<String>,
    /// The popup on screen, if any.
    popup: Option<popup::Popup>,
    host_out: notify::HostOut,
    /// The mouse pressed in a pane (a selection, if it drags), and the last click (when, where,
    /// how many in a row), for double and triple clicks.
    press: Option<copymode::Press>,
    last_click: Option<(Instant, (u16, u16), u8)>,
    /// The pane a button was pressed in for its program (its drag and release go there too).
    grab: Option<String>,
    /// An image paste is on its way to core.
    uploading: bool,
    /// Core's other machines and their lines (polled: core sends no event when one drops).
    machines: Vec<crate::machines::Machine>,
    /// This machine's name, for its group in the tree.
    hostname: String,
}

pub fn run(opts: Options) -> anyhow::Result<()> {
    let rt = tokio::runtime::Builder::new_current_thread().enable_all().build()?;
    let exited = rt.block_on(main(opts))?;
    if exited {
        println!("[exited]");
    }
    Ok(())
}

/// `Ok(true)` when every terminal exited (tmux prints `[exited]`), `Ok(false)` on detach.
async fn main(opts: Options) -> anyhow::Result<bool> {
    // Connect before taking over the screen, so errors print plainly.
    let client = Client::connect(&opts.transport).await?;
    if let Some(a) = &opts.attach {
        client.get(a).await?;
    }
    let myself = crate::nesting::own_terminal(&client).await;
    if let Some(me) = &myself {
        let rec = client.get(me).await?;
        let corgi_pane = crate::nesting::is_corgi_pane(&rec);
        if (corgi_pane || opts.attach.as_deref() == Some(me.as_str())) && std::env::var_os("CORGI_NESTED").is_none() {
            anyhow::bail!(
                "refusing to run inside {me}, which corgi itself shows as a pane: it would draw itself forever.\n\
                 Leave this corgi (C-b d in the outer one), or run corgi from a terminal it doesn't manage.\n\
                 Set CORGI_NESTED=1 to force (the pane {me} then shows a placeholder)."
            );
        }
    }
    let (out, mut rx) = mpsc::unbounded_channel();
    let (_guard, mut tui, display) = term::Guard::enter(opts.config.config.ui.mouse)?;
    let cfg = opts.config.config;
    let rounded = cfg.ui.corners == crate::config::Corners::Rounded;
    let theme = Theme::resolve(&cfg.theme, display.as_ref().and_then(|d| d.bg.as_deref())).unwrap_or_else(|e| {
        tracing::warn!("{e}");
        crate::theme::builtin("terminal").unwrap()
    });
    let theme = Theme { rounded, ..theme };
    let cfg_host = cfg.notifications.host;
    spawn_input(out.clone());
    let size = tui.size()?;
    let mut app = App {
        core_version: client.hello().version.clone(),
        transport: opts.transport,
        view: String::new(),
        session: opts.session,
        client,
        conn: Conn::Up,
        out,
        display,
        doc: Doc::new(),
        doc_version: 0,
        loaded: false,
        dirty: false,
        saving: false,
        booted: false,
        spawning: 0,
        layouts: HashMap::new(),
        attach_target: opts.attach,
        terminals: BTreeMap::new(),
        panes: HashMap::new(),
        screen: Rect::new(0, 0, size.width, size.height),
        host_focus: true,
        focus_known: false,
        mode: Mode::Normal,
        note: None,
        quit: false,
        early: Vec::new(),
        myself,
        keymap: Keymap::build(&cfg.keys).unwrap_or_else(|e| {
            tracing::warn!("{e}");
            Keymap::build(&Default::default()).unwrap()
        }),
        history: Vec::new(),
        sidebar_mode: cfg.sidebar.mode,
        wide: cfg.clone(),
        narrow: false,
        config: cfg,
        config_path: opts.config_path,
        theme,
        branches: HashMap::new(),
        repos: HashMap::new(),
        toplevels: HashMap::new(),
        scanning: None,
        rescan: false,
        hits: Hits::default(),
        title: String::new(),
        fold: Default::default(),
        sidebar_width: None,
        sidebar_focus: None,
        dragging: false,
        toasts: Default::default(),
        usage: Default::default(),
        paste: None,
        popup: None,
        host_out: notify::HostOut::new(cfg_host),
        machines: Vec::new(),
        hostname: hostname(),
        press: None,
        last_click: None,
        grab: None,
        uploading: false,
    };
    app.fit_width();
    app.start_usage();
    let saved = persist::load_view(&app.session);
    app.view = saved.workspace;
    app.fold = saved.fold;
    app.sidebar_width = saved.sidebar_width;
    if !opts.config.warnings.is_empty() {
        app.notify(format!("config: {}", opts.config.warnings.join("; ")));
    }
    app.connected();

    let mut dirty = true;
    let mut last_draw = Instant::now() - FRAME;
    // The slow lookups (git) run every TICK, kept across iterations so a busy loop doesn't
    // put them off; also at once when something they depend on changes.
    let mut last_tick = Instant::now();
    while !app.quit {
        if dirty && last_draw.elapsed() >= FRAME {
            term::draw(&mut tui, |f| app.draw(f))?;
            if app.panes.values_mut().any(|p| p.vt.take_bell()) {
                term::bell();
            }
            app.update_title();
            last_draw = Instant::now();
            dirty = false;
        }
        let now = Instant::now();
        let mut wait = if dirty {
            FRAME.saturating_sub(last_draw.elapsed())
        } else {
            TICK.saturating_sub(last_tick.elapsed())
        };
        // The key hints, when they're to open a moment after the prefix (and haven't yet).
        let hints_at = app.hint_deadline();
        if let Some(d) = hints_at {
            wait = wait.min(d.saturating_duration_since(now) + Duration::from_millis(1));
        }
        // The status line's message goes when it's had its time.
        let note_until = app.note.as_ref().map(|(_, at)| *at + NOTE_FOR);
        if let Some(d) = note_until {
            wait = wait.min(d.saturating_duration_since(now) + Duration::from_millis(1));
        }
        // The Machines dialog says how long a line has been down.
        if matches!(app.mode, Mode::Machines(_)) {
            wait = wait.min(Duration::from_secs(1));
        }
        // Wake when a pane's hold on its screen (synchronized output) runs out, to draw it anyway.
        if let Some(d) = app.panes.values().filter_map(|p| p.vt.hold_deadline()).min() {
            wait = wait.min(d.saturating_duration_since(Instant::now()) + Duration::from_millis(1));
        }
        if let Some(d) = app.toasts.deadline() {
            wait = wait.min(d.saturating_duration_since(Instant::now()) + Duration::from_millis(5));
        }
        // A working agent's circle spins.
        let spinning = app.spinning();
        if spinning {
            wait = wait.min(draw::SPIN_STEP);
        }
        // Wake when the repeat window ends, so the badge goes back from PREFIX.
        if let Mode::Repeat(until) | Mode::Panes(until) = app.mode
            && Instant::now() < until
        {
            wait = wait.min(until.saturating_duration_since(Instant::now()) + Duration::from_millis(5));
        }
        let repeat_until = match app.mode {
            Mode::Repeat(until) | Mode::Panes(until) => Some(until),
            _ => None,
        };
        tokio::select! {
            m = rx.recv() => {
                let Some(m) = m else { break };
                dirty |= app.handle(m)?;
                // What else is waiting, for a frame's time at most: then draw, and take keys,
                // before the rest (a flooding pane can't hold the screen up).
                let until = Instant::now() + FRAME;
                while Instant::now() < until {
                    let Ok(m) = rx.try_recv() else { break };
                    dirty |= app.handle(m)?;
                }
                // The output the panes used: the core may send that much more.
                for p in app.panes.values_mut() {
                    p.credit();
                }
                app.save();
            }
            _ = tokio::time::sleep(wait) => {
                let now = Instant::now();
                dirty |= app.toasts.expire(now) || spinning;
                // Only what changed: the hints opening, the repeat window ending, a hold
                // running out, the Machines dialog's clock.
                dirty |= hints_at.is_some_and(|d| now >= d)
                    || repeat_until.is_some_and(|d| now >= d)
                    || matches!(app.mode, Mode::Machines(_))
                    || app.panes.values().any(|p| p.vt.hold_deadline().is_some_and(|d| d <= now));
            }
        }
        // A message past its time goes, wherever it's shown (the tab bar, the narrow header).
        if note_until.is_some_and(|d| Instant::now() >= d) {
            app.note = None;
            dirty = true;
        }
        // Whatever woke us: events come often enough that waiting for a quiet moment would
        // put the slow things (git branches) off indefinitely. Missed ticks aren't made up.
        if last_tick.elapsed() >= TICK {
            app.tick();
            last_tick = Instant::now();
        }
    }
    // Let a last layout write go out before the connection closes with us.
    if app.saving || app.dirty {
        app.save();
        let _ = tokio::time::timeout(Duration::from_secs(1), async {
            while let Some(m) = rx.recv().await {
                if matches!(m, Msg::Saved(_)) {
                    break;
                }
            }
        })
        .await;
    }
    // The view and backups still on their way to disk.
    persist::flush(Duration::from_secs(1));
    Ok(app.loaded && app.doc.workspaces.is_empty() && app.booted)
}

/// Host input comes from a blocking thread.
fn spawn_input(out: UnboundedSender<Msg>) {
    std::thread::spawn(move || {
        while let Ok(ev) = crossterm::event::read() {
            if out.send(Msg::Input(ev)).is_err() {
                break;
            }
        }
    });
}

impl App {
    // ----- connection -----

    /// A (new) connection is up: watch it, subscribe, load the layout.
    fn connected(&mut self) {
        self.conn = Conn::Up;
        self.core_version = self.client.hello().version.clone();
        let (c, out) = (self.client.clone(), self.out.clone());
        tokio::spawn(async move {
            c.closed().await;
            let _ = out.send(Msg::Disconnected);
        });
        self.poll_machines();
        let (c, out) = (self.client.clone(), self.out.clone());
        tokio::spawn(async move {
            // Subscribe before reading the document (core's rule), so no change slips between.
            let Ok(mut ev) = c.subscribe(json!({})).await else {
                return;
            };
            let _ = out.send(Msg::Snapshot(ev.snapshot.terminals.clone()));
            let _ = out.send(Msg::DocLoaded(
                c.doc_get(model::DOC, None).await.map(|d| (d.value, d.version)),
            ));
            loop {
                match tokio::time::timeout(SILENCE, ev.next()).await {
                    Err(_) => {
                        let _ = out.send(Msg::Silent);
                        return;
                    }
                    Ok(None) => return,
                    Ok(Some(Applied::Resynced)) => {
                        let _ = out.send(Msg::Snapshot(ev.snapshot.terminals.clone()));
                        let doc = c.doc_get(model::DOC, None).await.map(|d| (d.value, d.version));
                        let _ = out.send(Msg::DocLoaded(doc));
                    }
                    Ok(Some(a)) => {
                        let rec = match &a {
                            Applied::Changed(addr) | Applied::Restarted(addr) => {
                                ev.snapshot.terminals.get(addr).cloned()
                            }
                            _ => None,
                        };
                        if out.send(Msg::Event(a, rec)).is_err() {
                            return;
                        }
                    }
                }
            }
        });
    }

    fn fetch_doc(&self) {
        let (c, out) = (self.client.clone(), self.out.clone());
        tokio::spawn(async move {
            let _ = out.send(Msg::DocLoaded(
                c.doc_get(model::DOC, None).await.map(|d| (d.value, d.version)),
            ));
        });
    }

    /// Write the layout if it changed. One write in flight; changes meanwhile go in the next.
    fn save(&mut self) {
        if !self.loaded || !self.dirty || self.saving || self.conn != Conn::Up {
            return;
        }
        self.dirty = false;
        self.saving = true;
        let value = serde_json::to_value(&self.doc).unwrap_or(Value::Null);
        let (c, out, version) = (self.client.clone(), self.out.clone(), self.doc_version);
        tokio::spawn(async move {
            let r = c.doc_set(model::DOC, &value, version, None).await;
            if let Ok(v) = &r {
                persist::backup(&value, *v);
            }
            let _ = out.send(Msg::Saved(r));
        });
    }

    // ----- messages -----

    /// Act on a message; false when nothing on screen can have changed (so no redraw).
    fn handle(&mut self, m: Msg) -> anyhow::Result<bool> {
        match m {
            Msg::Branches(b, r, t) if b == self.branches && r == self.repos && t == self.toplevels => {
                self.scan_done();
                return Ok(false);
            }
            Msg::Machines(list) if crate::machines::same(&list, &self.machines) => return Ok(false),
            // A heartbeat (or an event already applied) changes nothing on screen.
            Msg::Event(Applied::Heartbeat | Applied::Stale, _) => return Ok(false),
            Msg::Input(ev) => self.input(ev),
            Msg::Attached { pane, epoch, result } => {
                let Some(p) = self.panes.get_mut(&pane).filter(|p| p.is_current(epoch)) else {
                    return Ok(false);
                };
                match result {
                    Ok((tx, rec, asked)) => p.attached(tx, rec, asked)?,
                    Err(e) if e.kind() == Some("unavailable") => p.state = State::Recovering(e.to_string()),
                    Err(e) => {
                        tracing::warn!("attach {pane}: {e}");
                        p.closed(None);
                        self.notify(format!("can't attach {pane}: {e}"));
                    }
                }
            }
            Msg::Stream { pane, epoch, frame } => {
                let Some(p) = self.panes.get_mut(&pane).filter(|p| p.is_current(epoch)) else {
                    return Ok(false);
                };
                match frame {
                    Some(StreamFrame::Data(b)) => {
                        p.output(&b);
                        p.used(b.len());
                        let notes = p.vt.take_notifications();
                        if !notes.is_empty() {
                            self.pane_notifications(&pane, notes);
                        }
                    }
                    Some(StreamFrame::Resize(..)) => {}
                    Some(StreamFrame::Close(r)) => p.closed(r),
                    None => p.closed(None),
                }
            }
            Msg::Snapshot(ts) => {
                self.terminals = ts;
                if self.loaded {
                    self.reconcile();
                }
            }
            Msg::Event(a, rec) => self.event(a, rec),
            Msg::DocLoaded(Ok((value, version))) => self.doc_loaded(value, version),
            Msg::DocLoaded(Err(e)) => self.notify(format!("can't read the layout: {e}")),
            Msg::Saved(Ok(v)) => {
                self.saving = false;
                self.doc_version = v;
            }
            Msg::Saved(Err(e)) => {
                self.saving = false;
                if e.kind() == Some("conflict") {
                    // Another client wrote first: take theirs; our terminals come back through their labels.
                    self.fetch_doc();
                } else {
                    self.dirty = true;
                    tracing::warn!("saving the layout: {e}");
                }
            }
            Msg::Spawned(purpose, result) => {
                self.spawning = self.spawning.saturating_sub(1);
                match result {
                    Ok(t) => self.place(purpose, t),
                    Err(e) => self.notify(format!("can't start a shell: {e}")),
                }
            }
            Msg::Branches(b, r, t) => {
                self.branches = b;
                self.repos = r;
                self.toplevels = t;
                self.scan_done();
            }
            Msg::Lists(l) => self.lists_arrived(l),
            Msg::Worktree(m) => self.worktree_made(m),
            Msg::Usage(generation, i, rev, r) => self.usage_arrived(generation, i, rev, r),
            Msg::PopupStarted(r, ask) => self.popup_started(r, ask),
            Msg::Note(s) => self.notify(s),
            Msg::Machines(list) => self.machines_arrived(list),
            Msg::Machine(done) => self.machine_done(done),
            Msg::ImagePut(addr, r) => self.image_put(addr, r),
            Msg::Worktrees(r, folder, machine) => self.worktrees_listed(r, folder, machine),
            Msg::Silent => {
                tracing::warn!("no events or heartbeat for {SILENCE:?}; dropping the connection");
                self.client.close();
            }
            Msg::Disconnected if self.conn == Conn::Up => {
                self.conn = Conn::Reconnecting;
                self.saving = false;
                for p in self.panes.values_mut() {
                    p.detach();
                    p.state = State::Offline;
                }
                let (t, out) = (self.transport.clone(), self.out.clone());
                tokio::spawn(async move {
                    let _ = out.send(Msg::Connected(Client::reconnect(&t, Duration::from_secs(1)).await));
                });
            }
            Msg::Disconnected => {}
            Msg::Connected(Ok(c)) => {
                self.client = c;
                self.connected();
            }
            Msg::Connected(Err(e)) => self.notify(format!("can't reconnect: {e}")),
        }
        Ok(true)
    }

    fn event(&mut self, a: Applied, rec: Option<Terminal>) {
        match a {
            Applied::Changed(addr) => {
                let Some(t) = rec else { return };
                let finished = !t.is_live();
                if let Some(p) = self.panes.get_mut(&addr) {
                    p.record = Some(t.clone());
                }
                let before = self.terminals.insert(addr.clone(), t);
                if self.host_focus && self.window().is_some_and(|w| w.focus == addr) {
                    self.mark_seen(&addr);
                }
                if let Some(t) = self.terminals.get(&addr).cloned() {
                    self.agent_changed(before.as_ref(), &t);
                }
                // A workspace's folder moved (its first pane's): its branch and checkout (the
                // workspace's name) now. Other panes' folders don't matter to them.
                let cwd = |t: &Terminal| t.foreground.as_ref().map(|f| f.cwd.clone());
                if before.as_ref().map(cwd) != self.terminals.get(&addr).map(cwd) && self.gives_folder(&addr) {
                    self.tick();
                }
                if finished && self.popup.as_ref().is_some_and(|p| p.addr == addr) {
                    let code = self.terminals.get(&addr).and_then(|t| t.exit_code);
                    self.popup_exited(code);
                }
                if finished && self.doc.locate(&addr).is_some() {
                    // tmux: a pane closes when its program exits.
                    self.close_pane(&addr);
                }
            }
            Applied::Removed(addr) => {
                self.terminals.remove(&addr);
                if self.doc.remove_pane(&addr) {
                    self.dirty = true;
                    self.sync();
                }
            }
            Applied::Restarted(addr) => {
                if let Some(t) = rec {
                    self.terminals.insert(addr.clone(), t);
                }
                // A kept terminal has its new shell: view it.
                if let Some(p) = self.panes.get_mut(&addr) {
                    p.attach(&self.client, self.display.as_ref(), false, self.out.clone());
                }
            }
            Applied::Doc {
                name,
                version,
                machine: None,
            } if name == model::DOC && !self.saving && version != self.doc_version => self.fetch_doc(),
            _ => {}
        }
    }

    fn doc_loaded(&mut self, value: Value, version: u64) {
        let doc = match serde_json::from_value::<Doc>(value.clone()) {
            Ok(d) => d,
            Err(_) if value.is_null() => Doc::new(),
            Err(e) => {
                // Never overwrite a layout we can't read: show it, keep it.
                self.notify(format!("layout document unreadable ({e}); not saving over it"));
                tracing::error!("doc {}: {e}: {value}", model::DOC);
                return;
            }
        };
        // Ids handed out before core's copy came back stay taken: a workspace still being
        // made (its terminal starting) would otherwise share its id with the next one.
        self.doc = doc.replacing(&self.doc);
        self.doc_version = version;
        self.loaded = true;
        self.reconcile();
        if let Some(addr) = self.attach_target.take() {
            self.show_terminal(&addr);
        }
        // Its folders' branches and checkouts, now there are folders to look at.
        self.tick();
    }

    /// Make the layout agree with core's terminals, then with the screen.
    fn reconcile(&mut self) {
        let Reconciled {
            removed,
            adopted,
            relabel,
        } = self.doc.reconcile(&self.terminals);
        if !removed.is_empty() || !adopted.is_empty() {
            self.dirty = true;
            tracing::info!(?removed, ?adopted, "reconciled layout");
        }
        for addr in &removed {
            self.rm_if_finished(addr);
        }
        for (addr, ws, win) in relabel {
            self.label(&addr, &ws, &win);
        }
        if !self.doc.workspaces.is_empty() {
            self.booted = true;
        } else if self.booted {
            self.quit = true; // everything exited
        } else if self.spawning == 0 && self.attach_target.is_none() {
            let folder = std::env::current_dir()
                .map(|p| p.display().to_string())
                .unwrap_or_default();
            self.new_workspace(folder);
        }
        self.sync();
    }

    // ----- layout changes -----

    fn active(&self) -> Option<(String, String)> {
        let ws = self.doc.workspace(&self.view).or(self.doc.workspaces.first())?;
        Some((ws.id.clone(), ws.active_window()?.id.clone()))
    }

    fn window(&self) -> Option<&Window> {
        let (ws, win) = self.active()?;
        self.doc.workspace(&ws)?.window(&win)
    }

    fn window_mut(&mut self) -> Option<&mut Window> {
        let (ws, win) = self.active()?;
        self.doc.workspace_mut(&ws)?.window_mut(&win)
    }

    /// How the panes tile: the area they share, the cells between them, and whether each
    /// has its own box (`panes.separate` with the box style) or they share dividers.
    pub(super) fn tiling(&self) -> Tiling {
        let a = self.areas();
        let p = &self.config.panes;
        let boxed = a.body != a.panes;
        match (boxed, p.separate) {
            (true, true) => Tiling {
                area: a.body,
                sep: p.padding,
                boxes: true,
            },
            // Without boxes, separate panes still need a blank cell between them.
            (false, true) => Tiling {
                area: a.body,
                sep: p.padding.max(1),
                boxes: false,
            },
            (true, false) => Tiling {
                area: a.panes,
                sep: 1,
                boxes: false,
            },
            (false, false) => Tiling {
                area: a.body,
                sep: 1,
                boxes: false,
            },
        }
    }

    /// Each visible pane's whole cell in the tiling: its box, when it has one.
    pub(super) fn frames(&self) -> Vec<(String, Rect)> {
        let Some(w) = self.window() else { return Vec::new() };
        let t = self.tiling();
        // Narrow: one pane at a time, as if zoomed.
        if w.zoomed || self.narrow {
            vec![(w.focus.clone(), t.area)]
        } else {
            w.root.geometry_with(t.area, t.sep).0
        }
    }

    /// Where the visible panes go.
    fn rects(&self) -> Vec<(String, Rect)> {
        self.inside(self.frames())
    }

    /// The panes' contents within their frames: inside their boxes, below their tabs.
    fn inside(&self, frames: Vec<(String, Rect)>) -> Vec<(String, Rect)> {
        // Then the inset: blank cells between the border (and the title) and the terminal.
        let (ix, iy) = (self.config.panes.inset, self.config.panes.inset_rows);
        self.titled(frames)
            .into_iter()
            .map(|(id, mut r)| {
                if r.width > 2 * ix + 2 {
                    r = Rect::new(r.x + ix, r.y, r.width - 2 * ix, r.height);
                }
                if r.height > 2 * iy + 2 {
                    r = Rect::new(r.x, r.y + iy, r.width, r.height - 2 * iy);
                }
                (id, r)
            })
            .collect()
    }

    /// Each pane's room inside its frame, after its box and its title, before the inset.
    pub(super) fn titled(&self, frames: Vec<(String, Rect)>) -> Vec<(String, Rect)> {
        let t = self.tiling();
        let p = &self.config.panes;
        let crumbs =
            p.title_shape == crate::config::TitleShape::Breadcrumbs && p.title != crate::config::PaneTitle::Off;
        let rects = if t.boxes {
            frames
                .into_iter()
                .map(|(id, r)| match r.width >= 3 && r.height >= 3 {
                    true => (id, Rect::new(r.x + 1, r.y + 1, r.width - 2, r.height - 2)),
                    false => (id, r),
                })
                .collect()
        } else if crumbs {
            frames
        } else {
            return self.title_room(t.area, frames);
        };
        if !crumbs {
            return rects;
        }
        // Breadcrumbs: every pane gives its first row to them and the next to the line under
        // them (fewer, when it's short).
        rects
            .into_iter()
            .map(|(id, r)| {
                let take = match r.height {
                    0..=2 => 0,
                    3..=4 => 1,
                    _ => 2,
                };
                (id, Rect::new(r.x, r.y + take, r.width, r.height - take))
            })
            .collect()
    }

    /// Pane tabs go on the border line above each pane. Panes with none above them (the top
    /// row, without the panes' box) give up their first row for it.
    fn title_room(&self, area: Rect, rects: Vec<(String, Rect)>) -> Vec<(String, Rect)> {
        let a = self.areas();
        if self.config.panes.title == crate::config::PaneTitle::Off || a.body != a.panes {
            return rects;
        }
        rects
            .into_iter()
            .map(|(id, r)| {
                if r.y == area.y && r.height > 2 {
                    (id, Rect::new(r.x, r.y + 1, r.width, r.height - 1))
                } else {
                    (id, r)
                }
            })
            .collect()
    }

    /// Attach what's visible, at its size; let go of the rest.
    fn sync(&mut self) {
        if let Some((ws, _)) = self.active()
            && ws != self.view
        {
            self.view = ws;
            self.save_view();
        }
        self.usage_target();
        let rects = self.rects();
        let focus = self.window().map(|w| w.focus.clone());
        let visible: HashSet<&String> = rects.iter().map(|(id, _)| id).collect();
        // Panes on a machine whose line is down keep what they showed; they attach when it's back.
        let down: HashSet<String> = rects
            .iter()
            .filter(|(id, _)| self.down_machine(id).is_some())
            .map(|(id, _)| id.clone())
            .collect();
        // A popup's pane isn't in the layout, but stays while it's up.
        let popup = self.popup.as_ref().map(|p| p.addr.clone());
        self.panes
            .retain(|id, _| visible.contains(id) || popup.as_ref() == Some(id));
        for (id, r) in &rects {
            let size = (r.width.max(1), r.height.max(1));
            if !self.panes.contains_key(id) {
                match Pane::new(id.clone(), size) {
                    Ok(p) => {
                        self.panes.insert(id.clone(), p);
                    }
                    Err(e) => {
                        tracing::error!("pane {id}: {e}");
                        continue;
                    }
                }
            }
            let p = self.panes.get_mut(id).unwrap();
            if let Err(e) = p.resize(size) {
                tracing::warn!("resize {id}: {e}");
            }
            if p.record.is_none() {
                p.record = self.terminals.get(id).cloned();
            }
            let looking = self.host_focus && self.sidebar_focus.is_none() && focus.as_deref() == Some(id);
            p.focus(looking);
            if self.myself.as_deref() == Some(id.as_str()) {
                p.state = State::Myself;
                continue;
            }
            if p.state == State::Offline && self.conn == Conn::Up && !down.contains(id) {
                let resumed = p.record.is_some();
                p.attach(&self.client, self.display.as_ref(), resumed, self.out.clone());
            }
        }
        if self.host_focus
            && let Some(f) = &focus
        {
            self.mark_seen(&f.clone());
            if self.sidebar_focus.is_none() {
                self.toasts.dismiss_pane(f);
            }
        }
        if !self.early.is_empty() && !rects.is_empty() {
            for k in std::mem::take(&mut self.early) {
                self.key(k);
            }
        }
    }

    /// Start a terminal for `purpose`: the machine's shell, or `cmd` run by it.
    fn spawn(&mut self, purpose: Purpose, cwd: Option<String>, machine: Option<String>, run: Option<Run>) {
        self.spawning += 1;
        let (ws, win) = purpose.ids();
        // Start it at the size it will have, so the shell doesn't redraw for a resize at once.
        let area = self.tiling().area;
        let mut body = self.inside(vec![(String::new(), area)])[0].1;
        if let Purpose::Split { target, dir, pct, .. } = &purpose
            && let Some((_, r)) = self.rects().into_iter().find(|(id, _)| id == target)
        {
            let share = |n: u16| (n.saturating_sub(1) as u32 * *pct as u32 / 100) as u16;
            body = match dir {
                Dir::Horizontal => Rect::new(0, 0, share(r.width), r.height),
                Dir::Vertical => Rect::new(0, 0, r.width, share(r.height)),
            };
        }
        let mut params = json!({
            "cwd": cwd,
            "on": machine,
            "keep": true,
            "labels": {model::L_WORKSPACE: ws, model::L_WINDOW: win},
            "size": {"cols": body.width.max(1), "rows": body.height.max(1)},
            "env": {"TERM": "xterm-256color", "COLORTERM": "truecolor", "TERM_PROGRAM": "corgi",
                    "TERM_PROGRAM_VERSION": env!("CARGO_PKG_VERSION")},
        });
        match run {
            Some(Run::Line(line)) => params["line"] = json!(line),
            Some(Run::Template(name)) => params["argv"] = json!([name]),
            None => {}
        }
        let (c, out) = (self.client.clone(), self.out.clone());
        tokio::spawn(async move {
            let r = c.call_as::<Terminal>("terminal.run", params).await;
            let _ = out.send(Msg::Spawned(purpose, r));
        });
    }

    /// A terminal started for `purpose` exists: put it in the layout.
    fn place(&mut self, purpose: Purpose, t: Terminal) {
        let addr = t.address.clone();
        let (placed, at) = (purpose.clone(), addr.clone());
        // Known before its `created` event arrives, so reconciling doesn't drop it.
        // (If its events came first, theirs is newer.)
        self.terminals.entry(addr.clone()).or_insert(t);
        match purpose {
            Purpose::Workspace { ws, win, folder, name } => {
                let machine = addr.split_once('/').map(|(m, _)| m.to_string());
                self.doc
                    .add_workspace(ws.clone(), folder, machine, Window::new(win, addr));
                if let Some(s) = self.doc.workspace_mut(&ws) {
                    s.name = name;
                }
                // Its branch, and whether it's another's worktree, now rather than at the next tick.
                self.tick();
                self.view = ws;
                self.save_view();
                self.booted = true;
            }
            Purpose::Window { ws, win, name } => {
                let mut w = Window::new(win, addr);
                w.name = name;
                if !self.doc.add_window(&ws, w.clone()) {
                    self.doc.add_workspace(ws, String::new(), None, w);
                }
            }
            Purpose::Split {
                ws,
                win,
                target,
                dir,
                pct,
            } => {
                if !self.doc.split(&ws, &win, &target, addr.clone(), dir, pct) {
                    // Its window went away meanwhile: give it one of its own.
                    let w = self.doc.alloc('w');
                    self.label(&addr, &ws, &w);
                    if !self.doc.add_window(&ws, Window::new(w.clone(), addr.clone())) {
                        self.doc.add_workspace(ws, String::new(), None, Window::new(w, addr));
                    }
                }
            }
        }
        self.dirty = true;
        self.sync();
        self.after_place(&placed, &at);
        // Its program ended before it was placed (its event found no pane): close it now, as
        // its event would have.
        if self.terminals.get(&at).is_some_and(|t| !t.is_live()) {
            self.close_pane(&at);
        }
    }

    /// The folder a workspace is in now: its first pane's (the first window's first-made pane),
    /// as herdr has it; else where it started.
    pub(super) fn live_folder(&self, s: &crate::model::Workspace) -> String {
        s.windows
            .first()
            .and_then(|w| crate::tree::made_order(w.root.panes()).first().map(|p| p.to_string()))
            .and_then(|p| self.terminals.get(&p))
            .map(|t| {
                t.foreground
                    .as_ref()
                    .map(|f| f.cwd.clone())
                    .filter(|c| !c.is_empty())
                    .unwrap_or_else(|| t.cwd.clone())
            })
            .filter(|c| !c.is_empty())
            .unwrap_or_else(|| s.folder.clone())
    }

    /// A workspace's name: yours, if you gave it one; else the git checkout it's in now, else the
    /// folder (`~/src/corgi/crates` → `corgi`).
    pub(super) fn ws_label(&self, s: &crate::model::Workspace) -> String {
        if let Some(n) = &s.name {
            return n.clone();
        }
        let folder = self.live_folder(s);
        let key = git_key(s.machine.as_deref(), &folder);
        let named = self.toplevels.get(&key).map(String::as_str).unwrap_or(&folder);
        std::path::Path::new(named.trim_end_matches('/'))
            .file_name()
            .map(|f| f.to_string_lossy().into_owned())
            .unwrap_or_else(|| s.label())
    }

    /// The folder new terminals in the focused pane's place should start in.
    fn focused_cwd(&self) -> Option<String> {
        let w = self.window()?;
        let t = self.terminals.get(&w.focus)?;
        t.foreground
            .as_ref()
            .map(|f| f.cwd.clone())
            .filter(|c| !c.is_empty())
            .or(Some(t.cwd.clone()))
    }

    fn new_workspace(&mut self, folder: String) {
        let (ws, win) = (self.doc.alloc('s'), self.doc.alloc('w'));
        self.spawn(
            Purpose::Workspace {
                ws,
                win,
                folder: folder.clone(),
                name: None,
            },
            Some(folder),
            None,
            None,
        );
    }

    fn close_pane(&mut self, addr: &str) {
        if self.doc.remove_pane(addr) {
            self.dirty = true;
        }
        self.rm_if_finished(addr);
        if self.doc.workspaces.is_empty() && self.booted && self.spawning == 0 {
            self.quit = true;
        }
        self.sync();
    }

    /// Tidy core: a finished terminal we've closed doesn't need its record.
    fn rm_if_finished(&self, addr: &str) {
        if self.terminals.get(addr).is_some_and(|t| !t.is_live()) {
            let (c, a) = (self.client.clone(), addr.to_string());
            tokio::spawn(async move {
                let _ = c.call("terminal.rm", json!({"terminal": a})).await;
            });
        }
    }

    fn label(&self, addr: &str, ws: &str, win: &str) {
        let (c, a) = (self.client.clone(), addr.to_string());
        let labels = json!({model::L_WORKSPACE: ws, model::L_WINDOW: win});
        tokio::spawn(async move {
            if let Err(e) = c.call("terminal.label", json!({"terminal": a, "labels": labels})).await {
                tracing::warn!("label {a}: {e}");
            }
        });
    }

    fn kill(&self, addrs: Vec<String>) {
        let c = self.client.clone();
        tokio::spawn(async move {
            for a in addrs {
                let _ = c.call("terminal.kill", json!({"terminal": a})).await;
            }
        });
    }

    /// `corgi attach <addr>`: show that terminal, adding it to the layout if needed.
    fn show_terminal(&mut self, addr: &str) {
        if self.doc.locate(addr).is_none() {
            let win = self.doc.alloc('w');
            let ws = match self.active() {
                Some((ws, _)) => {
                    self.doc.add_window(&ws, Window::new(win.clone(), addr.into()));
                    ws
                }
                None => {
                    let ws = self.doc.alloc('s');
                    let cwd = self.terminals.get(addr).map(|t| t.cwd.clone()).unwrap_or_default();
                    self.doc
                        .add_workspace(ws.clone(), cwd, None, Window::new(win.clone(), addr.into()));
                    ws
                }
            };
            self.label(addr, &ws, &win);
            self.dirty = true;
        }
        if let Some((si, wi)) = self.doc.locate(addr) {
            let s = &mut self.doc.workspaces[si];
            let win = s.windows[wi].id.clone();
            s.select(&win);
            s.windows[wi].set_focus(addr);
            self.view = s.id.clone();
        }
        self.booted = true;
        self.sync();
    }

    fn window_index(&self) -> Option<usize> {
        let (ws, win) = self.active()?;
        self.doc.workspace(&ws)?.windows.iter().position(|w| w.id == win)
    }

    fn notify(&mut self, s: String) {
        self.note = Some((s, Instant::now()));
    }

    fn prompt(&mut self, label: &str, text: String, then: Then) {
        self.mode = Mode::Prompt(Prompt {
            label: label.into(),
            text,
            then,
            hist: None,
        });
    }

    // ----- input -----

    fn focused_pane(&mut self) -> Option<&mut Pane> {
        let f = self.window()?.focus.clone();
        self.panes.get_mut(&f)
    }

    fn type_key(&mut self, key: &corgi_vt::Key) {
        if let Some(p) = self.focused_pane() {
            let bytes = p.vt.encode_key(key);
            p.input(&bytes);
        }
    }

    fn input(&mut self, ev: Event) {
        match ev {
            Event::Key(k) if k.kind != KeyEventKind::Release => {
                if self.window().is_none() {
                    self.early.push(k);
                } else {
                    self.key(k);
                }
            }
            Event::Paste(text) => match &mut self.mode {
                Mode::Prompt(p) => p.text.push_str(text.lines().next().unwrap_or("")),
                Mode::Popup => self.popup_paste(&text),
                _ => self.paste_into_pane(text),
            },
            Event::Resize(w, h) => {
                self.screen = Rect::new(0, 0, w, h);
                self.fit_width();
                self.sync();
            }
            Event::FocusGained | Event::FocusLost => {
                self.host_focus = matches!(ev, Event::FocusGained);
                self.focus_known = true;
                self.sync();
            }
            Event::Mouse(m) => {
                let at = ratatui::layout::Position::new(m.column, m.row);
                // A program that asked for the mouse gets it (Shift held: corgi's).
                if matches!(self.mode, Mode::Normal) && self.sidebar_focus.is_none() && self.mouse_to_program(m) {
                    return;
                }
                // Right-click: the action menu for what's under it.
                if let MouseEventKind::Down(MouseButton::Right) = m.kind {
                    self.right_click(at);
                    return;
                }
                // A popup keeps the mouse from what's under it.
                if let Mode::Popup = self.mode {
                    return;
                }
                // Selecting with the mouse (in a pane, or in copy mode).
                if matches!(self.mode, Mode::Normal | Mode::Copy(..)) && self.copy_mouse(m) {
                    return;
                }
                // Copy mode: the wheel moves its view; a click leaves it there.
                if let Mode::Copy(c, _) = &mut self.mode {
                    match m.kind {
                        MouseEventKind::ScrollUp => c.page(-3),
                        MouseEventKind::ScrollDown => c.page(3),
                        _ => {}
                    }
                    return;
                }
                // A menu goes on any other click.
                if let Mode::Menu(..) = self.mode {
                    if let MouseEventKind::Down(_) = m.kind {
                        self.mode = Mode::Normal;
                    }
                    return;
                }
                // A dialog keeps the mouse to itself.
                if let Mode::Dialog(_) = self.mode {
                    if let MouseEventKind::Down(MouseButton::Left) = m.kind {
                        self.dialog_click(m.column, m.row);
                    }
                    return;
                }
                if self.drag_sidebar(m.kind, m.column, m.row) {
                    return;
                }
                if let MouseEventKind::Down(MouseButton::Left) = m.kind
                    && self.click(at)
                {
                    return;
                }
                // A pane, or its tab on the line above it.
                let hit = self
                    .rects()
                    .into_iter()
                    .find(|(_, r)| r.contains(at) || (at.y < r.y && at.y + 2 >= r.y && at.x >= r.x && at.x < r.right()))
                    .map(|(id, _)| id);
                let Some(id) = hit else { return };
                match m.kind {
                    MouseEventKind::Down(MouseButton::Left) => {
                        if let Some(w) = self.window_mut()
                            && w.focus != id
                        {
                            w.set_focus(&id);
                            self.dirty = true;
                        }
                        self.sync();
                    }
                    MouseEventKind::ScrollUp => self.panes.get_mut(&id).into_iter().for_each(|p| p.vt.scroll(-3)),
                    MouseEventKind::ScrollDown => self.panes.get_mut(&id).into_iter().for_each(|p| p.vt.scroll(3)),
                    _ => {}
                }
            }
            _ => {}
        }
    }
}

mod actions;
mod copymode;
mod dialogs;
mod draw;
mod keyboard;
mod machines;
mod menus;
mod mouse;
mod narrow;
mod notify;
mod paste;
mod popup;
mod usage;
mod worktrees;

impl App {
    /// The narrow layout on a narrow screen (`ui.narrow_width`), the configured one otherwise.
    fn fit_width(&mut self) {
        let n = self.wide.ui.narrow_width;
        self.narrow = n > 0 && self.screen.width <= n;
        self.config = if self.narrow {
            self.wide.narrowed()
        } else {
            self.wide.clone()
        };
    }
}

/// Between a machine's name and a folder in the keys git facts go by.
const KEY_SEP: char = '\u{1f}';

/// What a folder's git facts (branch, checkout, repository) are kept under: the folder on this
/// machine; on another, its name and the folder, so the same path on two machines doesn't mix.
fn git_key(machine: Option<&str>, folder: &str) -> String {
    match machine {
        None => folder.to_string(),
        Some(m) => format!("{m}{KEY_SEP}{folder}"),
    }
}

/// This machine's name (up to the first dot), for its group in the tree.
fn hostname() -> String {
    let name = crate::usage_sources::hostname();
    let short = name.split('.').next().unwrap_or("").trim().to_string();
    if short.is_empty() { "this machine".into() } else { short }
}
