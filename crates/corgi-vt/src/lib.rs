//! A pane's terminal emulator. Feed it the bytes herdr-core relays, read back
//! a [`Frame`] of styled cells to draw. Wraps libghostty-vt's render-state API.

#[allow(non_camel_case_types, non_upper_case_globals, non_snake_case, dead_code, clippy::all)]
mod sys {
    include!(concat!(env!("OUT_DIR"), "/sys.rs"));
}

mod keys;
mod mouse;

pub use keys::{Key, Mods, Named};
pub use mouse::{MouseAction, MouseButton};

use std::ptr;
use std::time::{Duration, Instant};

/// How long a program may hold the screen (synchronized output) before we draw anyway.
pub const HOLD_LIMIT: Duration = Duration::from_secs(1);

/// An RGB colour; `None` in a [`Cell`] means "the terminal default".
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rgb(pub u8, pub u8, pub u8);

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Wide {
    #[default]
    Narrow,
    /// First column of a double-width character.
    Wide,
    /// Second column of a double-width character: draw nothing.
    Tail,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Cell {
    /// The grapheme cluster; empty for a blank cell.
    pub text: String,
    pub fg: Option<Rgb>,
    pub bg: Option<Rgb>,
    pub bold: bool,
    pub italic: bool,
    pub faint: bool,
    pub underline: bool,
    pub inverse: bool,
    pub strike: bool,
    pub invisible: bool,
    pub wide: Wide,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CursorStyle {
    Block,
    Bar,
    Underline,
    HollowBlock,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Cursor {
    pub x: u16,
    pub y: u16,
    pub style: CursorStyle,
    pub blinking: bool,
}

/// Everything needed to draw one pane, row-major `rows × cols`.
#[derive(Clone, Debug)]
pub struct Frame {
    pub cols: u16,
    pub rows: u16,
    pub cells: Vec<Cell>,
    /// `None` when hidden or outside the viewport.
    pub cursor: Option<Cursor>,
    pub fg: Rgb,
    pub bg: Rgb,
}

impl Frame {
    pub fn cell(&self, x: u16, y: u16) -> &Cell {
        &self.cells[y as usize * self.cols as usize + x as usize]
    }
}

#[derive(Debug)]
pub struct Error(&'static str, i32);

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "libghostty-vt: {} failed ({})", self.0, self.1)
    }
}

impl std::error::Error for Error {}

fn check(what: &'static str, rc: sys::GhosttyResult) -> Result<(), Error> {
    if rc == sys::GHOSTTY_SUCCESS {
        Ok(())
    } else {
        Err(Error(what, rc))
    }
}

/// A sized struct, zeroed with its `size` field set (C's `GHOSTTY_INIT_SIZED`).
macro_rules! sized {
    ($t:ty) => {{
        // SAFETY: plain C structs of integers/bools/unions; all-zero is valid.
        #[allow(unused_unsafe)]
        let mut v: $t = unsafe { std::mem::zeroed() };
        v.size = std::mem::size_of::<$t>();
        v
    }};
}

/// A desktop notification a program asked for. `title` is empty when the protocol has none (OSC 9).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Notification {
    pub title: String,
    pub body: String,
}

/// What callbacks collected during `write`. Boxed so its address is stable for libghostty's userdata.
struct Sink {
    /// Bytes the emulator wants to send back to the program (query answers, size reports).
    replies: Vec<u8>,
    bell: bool,
    title: bool,
    /// Desktop notifications asked for (OSC 9, OSC 777), oldest first.
    notes: Vec<Notification>,
    size: sys::GhosttySizeReportSize,
    /// Our render state, so the hold callback can capture the frame the program wants shown.
    state: sys::GhosttyRenderState,
    /// Since when the program holds the screen (mode 2026), if it does.
    hold: Option<Instant>,
}

impl Default for Sink {
    fn default() -> Sink {
        Sink {
            replies: Vec::new(),
            bell: false,
            title: false,
            notes: Vec::new(),
            size: sys::GhosttySizeReportSize::default(),
            state: ptr::null_mut(),
            hold: None,
        }
    }
}

pub struct Vt {
    term: sys::GhosttyTerminal,
    state: sys::GhosttyRenderState,
    rows_it: sys::GhosttyRenderStateRowIterator,
    cells: sys::GhosttyRenderStateRowCells,
    keys: sys::GhosttyKeyEncoder,
    key_event: sys::GhosttyKeyEvent,
    mouse: sys::GhosttyMouseEncoder,
    mouse_event: sys::GhosttyMouseEvent,
    sink: Box<Sink>,
    utf8: Vec<u8>,
    /// The last capture; only rows ghostty marks dirty are read again.
    frame: Frame,
    /// Capture even during a hold: the person scrolled, show it now.
    live: bool,
    /// Rises with every write and resize: the text may have changed since.
    revision: u64,
}

unsafe extern "C" fn on_write(_t: sys::GhosttyTerminal, ud: *mut std::ffi::c_void, data: *const u8, len: usize) {
    // SAFETY: `ud` is the Box<Sink> owned by the Vt whose write is running; data/len come from libghostty.
    unsafe {
        (*(ud as *mut Sink))
            .replies
            .extend_from_slice(std::slice::from_raw_parts(data, len))
    }
}

unsafe extern "C" fn on_bell(_t: sys::GhosttyTerminal, ud: *mut std::ffi::c_void) {
    // SAFETY: as above.
    unsafe { (*(ud as *mut Sink)).bell = true }
}

/// At most this many notifications wait to be taken; a program spamming them can't grow memory.
const NOTES_MAX: usize = 16;

unsafe extern "C" fn on_notify(
    _t: sys::GhosttyTerminal,
    ud: *mut std::ffi::c_void,
    n: *const sys::GhosttyTerminalDesktopNotification,
) {
    // SAFETY: `ud` is our boxed Sink; `n` and its strings are borrowed for this call only, so copied.
    unsafe {
        let sink = &mut *(ud as *mut Sink);
        let n = &*n;
        let text = |s: &sys::GhosttyString| {
            if s.ptr.is_null() || s.len == 0 {
                String::new()
            } else {
                String::from_utf8_lossy(std::slice::from_raw_parts(s.ptr, s.len)).into_owned()
            }
        };
        if sink.notes.len() >= NOTES_MAX {
            sink.notes.remove(0);
        }
        sink.notes.push(Notification {
            title: text(&n.title),
            body: text(&n.body),
        });
    }
}

unsafe extern "C" fn on_size(
    _t: sys::GhosttyTerminal,
    ud: *mut std::ffi::c_void,
    out: *mut sys::GhosttySizeReportSize,
) -> bool {
    // SAFETY: as above; `out` is a valid out-pointer.
    unsafe { *out = (*(ud as *mut Sink)).size };
    true
}

/// Synchronized output: capture the finished frame as the hold begins, then keep showing it.
unsafe extern "C" fn on_hold(t: sys::GhosttyTerminal, ud: *mut std::ffi::c_void, held: bool) {
    // SAFETY: `ud` is our boxed Sink; `state` is valid for the terminal's whole life.
    unsafe {
        let sink = &mut *(ud as *mut Sink);
        if held {
            sys::ghostty_render_state_update(sink.state, t);
            sink.hold = Some(Instant::now());
        } else {
            sink.hold = None;
        }
    }
}

unsafe extern "C" fn on_title(_t: sys::GhosttyTerminal, ud: *mut std::ffi::c_void) {
    // SAFETY: as above.
    unsafe { (*(ud as *mut Sink)).title = true }
}

// SAFETY: every handle is owned by this value and only used through &mut self.
unsafe impl Send for Vt {}

impl Vt {
    pub fn new(cols: u16, rows: u16) -> Result<Vt, Error> {
        let mut vt = Vt {
            term: ptr::null_mut(),
            state: ptr::null_mut(),
            rows_it: ptr::null_mut(),
            cells: ptr::null_mut(),
            keys: ptr::null_mut(),
            key_event: ptr::null_mut(),
            mouse: ptr::null_mut(),
            mouse_event: ptr::null_mut(),
            sink: Box::new(Sink {
                size: sys::GhosttySizeReportSize {
                    rows,
                    columns: cols,
                    cell_width: 0,
                    cell_height: 0,
                },
                ..Sink::default()
            }),
            utf8: vec![0; 64],
            frame: Frame {
                cols: 0,
                rows: 0,
                cells: Vec::new(),
                cursor: None,
                fg: Rgb(0, 0, 0),
                bg: Rgb(0, 0, 0),
            },
            live: false,
            revision: 0,
        };
        // SAFETY: out-pointers to null handles; a null allocator means the default one.
        unsafe {
            check(
                "terminal_new",
                sys::ghostty_terminal_new(ptr::null(), &mut vt.term, cols, rows),
            )?;
            check(
                "render_state_new",
                sys::ghostty_render_state_new(ptr::null(), &mut vt.state),
            )?;
            check(
                "row_iterator_new",
                sys::ghostty_render_state_row_iterator_new(ptr::null(), &mut vt.rows_it),
            )?;
            check(
                "row_cells_new",
                sys::ghostty_render_state_row_cells_new(ptr::null(), &mut vt.cells),
            )?;
            check(
                "key_encoder_new",
                sys::ghostty_key_encoder_new(ptr::null(), &mut vt.keys),
            )?;
            check(
                "key_event_new",
                sys::ghostty_key_event_new(ptr::null(), &mut vt.key_event),
            )?;
            check(
                "mouse_encoder_new",
                sys::ghostty_mouse_encoder_new(ptr::null(), &mut vt.mouse),
            )?;
            check(
                "mouse_event_new",
                sys::ghostty_mouse_event_new(ptr::null(), &mut vt.mouse_event),
            )?;
            vt.sink.state = vt.state;
            let ud: *mut Sink = &mut *vt.sink;
            check(
                "set userdata",
                sys::ghostty_terminal_set(vt.term, sys::GHOSTTY_TERMINAL_OPT_USERDATA, ud.cast()),
            )?;
            let write: sys::GhosttyTerminalWritePtyFn = Some(on_write);
            let bell: sys::GhosttyTerminalBellFn = Some(on_bell);
            let title: sys::GhosttyTerminalTitleChangedFn = Some(on_title);
            let size: sys::GhosttyTerminalSizeFn = Some(on_size);
            let hold: sys::GhosttyTerminalRenderHoldFn = Some(on_hold);
            let notify: sys::GhosttyTerminalDesktopNotificationFn = Some(on_notify);
            // Function-pointer options take the pointer value itself.
            for (opt, f) in [
                (
                    sys::GHOSTTY_TERMINAL_OPT_WRITE_PTY,
                    write.map(|f| f as *const std::ffi::c_void),
                ),
                (
                    sys::GHOSTTY_TERMINAL_OPT_BELL,
                    bell.map(|f| f as *const std::ffi::c_void),
                ),
                (
                    sys::GHOSTTY_TERMINAL_OPT_TITLE_CHANGED,
                    title.map(|f| f as *const std::ffi::c_void),
                ),
                (
                    sys::GHOSTTY_TERMINAL_OPT_SIZE,
                    size.map(|f| f as *const std::ffi::c_void),
                ),
                (
                    sys::GHOSTTY_TERMINAL_OPT_RENDER_HOLD,
                    hold.map(|f| f as *const std::ffi::c_void),
                ),
                (
                    sys::GHOSTTY_TERMINAL_OPT_DESKTOP_NOTIFICATION,
                    notify.map(|f| f as *const std::ffi::c_void),
                ),
            ] {
                check("set callback", sys::ghostty_terminal_set(vt.term, opt, f.unwrap()))?;
            }
        }
        Ok(vt)
    }

    /// Rises whenever the text may have changed (output, a resize): a copy of it taken at
    /// one revision is still good while this stays the same.
    pub fn revision(&self) -> u64 {
        self.revision
    }

    /// Program output, exactly as relayed.
    pub fn write(&mut self, bytes: &[u8]) {
        self.revision += 1;
        // SAFETY: valid terminal; the slice outlives the call.
        unsafe { sys::ghostty_terminal_vt_write(self.term, bytes.as_ptr(), bytes.len()) }
    }

    /// Bytes the emulator generated for the program since the last call (e.g. a
    /// mode-2048 size report). Send them as automatic input, never as typing.
    pub fn take_replies(&mut self) -> Vec<u8> {
        std::mem::take(&mut self.sink.replies)
    }

    /// When the program's hold on the screen (synchronized output) runs out, if it has one.
    /// Draw again then: [`Vt::frame`] ends an overlong hold itself.
    pub fn hold_deadline(&self) -> Option<Instant> {
        self.sink.hold.map(|t| t + HOLD_LIMIT)
    }

    /// Give up on the program's hold: it is taking too long, or it no longer matters.
    fn release_hold(&mut self) {
        let mut m = sys::GhosttyTerminalModeConfig {
            mode: 2026,
            value: false,
        };
        // SAFETY: MODE takes a GhosttyTerminalModeConfig; setting it never calls back.
        unsafe {
            sys::ghostty_terminal_set(
                self.term,
                sys::GHOSTTY_TERMINAL_OPT_MODE,
                (&mut m as *mut sys::GhosttyTerminalModeConfig).cast(),
            );
        }
        self.sink.hold = None;
    }

    /// Whether a BEL arrived since the last call.
    pub fn take_bell(&mut self) -> bool {
        std::mem::take(&mut self.sink.bell)
    }

    /// Desktop notifications the program asked for (OSC 9, OSC 777) since the last call.
    pub fn take_notifications(&mut self) -> Vec<Notification> {
        std::mem::take(&mut self.sink.notes)
    }

    /// The window title, if it changed since the last call.
    pub fn take_title(&mut self) -> Option<String> {
        if !std::mem::take(&mut self.sink.title) {
            return None;
        }
        let mut s = sys::GhosttyString {
            ptr: ptr::null(),
            len: 0,
        };
        // SAFETY: TITLE yields a GhosttyString borrowed from the terminal; copied before any other call.
        let rc = unsafe {
            sys::ghostty_terminal_get(
                self.term,
                sys::GHOSTTY_TERMINAL_DATA_TITLE,
                (&mut s as *mut sys::GhosttyString).cast(),
            )
        };
        if rc != sys::GHOSTTY_SUCCESS || s.ptr.is_null() {
            return Some(String::new());
        }
        // SAFETY: ptr/len describe valid bytes owned by the terminal.
        Some(String::from_utf8_lossy(unsafe { std::slice::from_raw_parts(s.ptr, s.len) }).into_owned())
    }

    /// A terminal mode: `ansi` false for DEC private modes (`?2004`), true for ANSI ones.
    pub fn mode(&self, value: u16, ansi: bool) -> bool {
        let mut m = sys::GhosttyTerminalModeConfig {
            mode: (value & 0x7fff) | ((ansi as u16) << 15),
            value: false,
        };
        // SAFETY: MODE takes an initialised GhosttyTerminalModeConfig.
        let rc = unsafe {
            sys::ghostty_terminal_get(
                self.term,
                sys::GHOSTTY_TERMINAL_DATA_MODE,
                (&mut m as *mut sys::GhosttyTerminalModeConfig).cast(),
            )
        };
        rc == sys::GHOSTTY_SUCCESS && m.value
    }

    /// Encode a key press for the program's current modes (cursor keys, kitty flags, ...).
    pub fn encode_key(&mut self, key: &Key) -> Vec<u8> {
        keys::encode(self, key)
    }

    /// Encode a paste: bracketed if the program asked (mode 2004), unsafe control bytes stripped.
    pub fn encode_paste(&self, text: &str) -> Vec<u8> {
        let bracketed = self.mode(2004, false);
        let mut out = vec![0u8; text.len() + 16];
        loop {
            let mut data = text.as_bytes().to_vec();
            let mut n = 0usize;
            // SAFETY: data and out are valid for their lengths; libghostty edits `data` in place.
            let rc = unsafe {
                sys::ghostty_paste_encode(
                    data.as_mut_ptr().cast(),
                    data.len(),
                    bracketed,
                    out.as_mut_ptr().cast(),
                    out.len(),
                    &mut n,
                )
            };
            if rc == sys::GHOSTTY_OUT_OF_SPACE {
                out.resize(n, 0);
                continue;
            }
            out.truncate(if rc == sys::GHOSTTY_SUCCESS { n } else { 0 });
            return out;
        }
    }

    /// Scroll the view into history (negative = up). New output doesn't move it; see `scroll_bottom`.
    pub fn scroll(&mut self, delta: isize) {
        self.live = true;
        // SAFETY: zeroed value union plus the tag; DELTA reads `delta`.
        unsafe {
            let mut b: sys::GhosttyTerminalScrollViewport = std::mem::zeroed();
            b.tag = sys::GHOSTTY_SCROLL_VIEWPORT_DELTA;
            b.value.delta = delta;
            sys::ghostty_terminal_scroll_viewport(self.term, b);
        }
    }

    /// Scroll so `row` (0: the oldest row of history) is the viewport's first row; clamped.
    pub fn scroll_to(&mut self, row: usize) {
        self.live = true;
        // SAFETY: zeroed value union plus the tag; ROW reads `row`.
        unsafe {
            let mut b: sys::GhosttyTerminalScrollViewport = std::mem::zeroed();
            b.tag = sys::GHOSTTY_SCROLL_VIEWPORT_ROW;
            b.value.row = row;
            sys::ghostty_terminal_scroll_viewport(self.term, b);
        }
    }

    /// (rows in all, the viewport's first row, rows in the viewport), in `scroll_to`'s row space.
    pub fn scrollbar(&self) -> (usize, usize, usize) {
        let mut sb = sys::GhosttyTerminalScrollbar::default();
        // SAFETY: SCROLLBAR writes a GhosttyTerminalScrollbar.
        let rc = unsafe {
            sys::ghostty_terminal_get(
                self.term,
                sys::GHOSTTY_TERMINAL_DATA_SCROLLBAR,
                (&mut sb as *mut sys::GhosttyTerminalScrollbar).cast(),
            )
        };
        if rc != sys::GHOSTTY_SUCCESS {
            return (0, 0, 0);
        }
        (sb.total as usize, sb.offset as usize, sb.len as usize)
    }

    /// Every row's text, history first, one line per row (soft-wrapped rows stay separate, so
    /// line `i` is row `i` of `scroll_to`'s row space), trailing blanks trimmed. Plain text, for
    /// moving, searching and copying in it.
    pub fn screen_lines(&mut self) -> Vec<String> {
        let (total, ..) = self.scrollbar();
        let mut opts = sized!(sys::GhosttyFormatterTerminalOptions);
        opts.emit = sys::GHOSTTY_FORMATTER_FORMAT_PLAIN;
        opts.unwrap = false;
        opts.trim = true;
        opts.extra = sized!(sys::GhosttyFormatterTerminalExtra);
        opts.extra.screen = sized!(sys::GhosttyFormatterScreenExtra);
        opts.selection = ptr::null();
        let mut text = String::new();
        // SAFETY: the formatter borrows our terminal only for this call; the buffer is libghostty's
        // own, copied and freed with the same (default) allocator.
        unsafe {
            let mut f: sys::GhosttyFormatter = ptr::null_mut();
            if sys::ghostty_formatter_terminal_new(ptr::null(), &mut f, self.term, opts) == sys::GHOSTTY_SUCCESS {
                let (mut out, mut len) = (ptr::null_mut::<u8>(), 0usize);
                if sys::ghostty_formatter_format_alloc(f, ptr::null(), &mut out, &mut len) == sys::GHOSTTY_SUCCESS
                    && !out.is_null()
                {
                    text = String::from_utf8_lossy(std::slice::from_raw_parts(out, len)).into_owned();
                    sys::ghostty_free(ptr::null(), out, len);
                }
                sys::ghostty_formatter_free(f);
            }
        }
        let mut lines: Vec<String> = text.split('\n').map(|l| l.trim_end().to_string()).collect();
        // Trailing blank rows aren't written: put them back, so rows line up.
        lines.resize(total.max(lines.len()), String::new());
        lines.truncate(total.max(1));
        lines
    }

    pub fn scroll_bottom(&mut self) {
        self.live = true;
        // SAFETY: BOTTOM ignores the value.
        unsafe {
            let mut b: sys::GhosttyTerminalScrollViewport = std::mem::zeroed();
            b.tag = sys::GHOSTTY_SCROLL_VIEWPORT_BOTTOM;
            sys::ghostty_terminal_scroll_viewport(self.term, b);
        }
    }

    /// Resize the grid. `cell` is the host's cell size in pixels when known (for pixel size reports).
    /// If the program enabled in-band size reports (mode 2048), one is queued in `take_replies`.
    pub fn resize(&mut self, cols: u16, rows: u16) -> Result<(), Error> {
        self.revision += 1;
        let (cw, ch) = (self.sink.size.cell_width, self.sink.size.cell_height);
        // SAFETY: valid terminal.
        check("resize", unsafe {
            sys::ghostty_terminal_resize(self.term, cols, rows, cw, ch)
        })?;
        self.sink.size.columns = cols;
        self.sink.size.rows = rows;
        if self.mode(2048, false) {
            let mut buf = [0u8; 64];
            let mut n = 0usize;
            // SAFETY: buf is valid for its length.
            let rc = unsafe {
                sys::ghostty_size_report_encode(
                    sys::GHOSTTY_SIZE_REPORT_MODE_2048,
                    self.sink.size,
                    buf.as_mut_ptr().cast(),
                    buf.len(),
                    &mut n,
                )
            };
            if rc == sys::GHOSTTY_SUCCESS {
                self.sink.replies.extend_from_slice(&buf[..n]);
            }
        }
        Ok(())
    }

    /// The host terminal's cell size in pixels, used in pixel size reports.
    pub fn set_cell_pixels(&mut self, w: u32, h: u32) {
        self.sink.size.cell_width = w;
        self.sink.size.cell_height = h;
    }

    /// Capture the current screen. Rows that didn't change since the last capture are
    /// kept as they were. While the program holds the screen (synchronized output) this
    /// is the frame it last finished, until the hold ends or runs past [`HOLD_LIMIT`].
    pub fn frame(&mut self) -> Result<&Frame, Error> {
        if self.sink.hold.is_some_and(|t| t.elapsed() >= HOLD_LIMIT) {
            self.release_hold();
        }
        // SAFETY: all handles are valid and exclusively ours. Out-pointers match
        // the documented type for each data kind.
        unsafe {
            if self.sink.hold.is_none() || std::mem::take(&mut self.live) {
                check("update", sys::ghostty_render_state_update(self.state, self.term))?;
            }
            let mut dirty: sys::GhosttyRenderStateDirty = sys::GHOSTTY_RENDER_STATE_DIRTY_FULL;
            get(self.state, sys::GHOSTTY_RENDER_STATE_DATA_DIRTY, &mut dirty)?;
            let (mut cols, mut rows) = (0u16, 0u16);
            get(self.state, sys::GHOSTTY_RENDER_STATE_DATA_COLS, &mut cols)?;
            get(self.state, sys::GHOSTTY_RENDER_STATE_DATA_ROWS, &mut rows)?;
            let mut colors = sized!(sys::GhosttyRenderStateColors);
            get(self.state, sys::GHOSTTY_RENDER_STATE_DATA_COLORS, &mut colors)?;
            let mut cur = sized!(sys::GhosttyRenderStateCursor);
            get(self.state, sys::GHOSTTY_RENDER_STATE_DATA_CURSOR, &mut cur)?;

            let f = &mut self.frame;
            let full = dirty == sys::GHOSTTY_RENDER_STATE_DIRTY_FULL || (f.cols, f.rows) != (cols, rows);
            if full && (f.cols, f.rows) != (cols, rows) {
                f.cells.clear();
                f.cells.resize(cols as usize * rows as usize, Cell::default());
                (f.cols, f.rows) = (cols, rows);
            }
            if full || dirty != sys::GHOSTTY_RENDER_STATE_DIRTY_FALSE {
                get(
                    self.state,
                    sys::GHOSTTY_RENDER_STATE_DATA_ROW_ITERATOR,
                    &mut self.rows_it,
                )?;
                let mut y = 0usize;
                while y < rows as usize && sys::ghostty_render_state_row_iterator_next(self.rows_it) {
                    let mut row_dirty = true;
                    if !full {
                        check(
                            "row dirty",
                            sys::ghostty_render_state_row_get(
                                self.rows_it,
                                sys::GHOSTTY_RENDER_STATE_ROW_DATA_DIRTY,
                                (&mut row_dirty as *mut bool).cast(),
                            ),
                        )?;
                    }
                    if row_dirty {
                        check(
                            "row cells",
                            sys::ghostty_render_state_row_get(
                                self.rows_it,
                                sys::GHOSTTY_RENDER_STATE_ROW_DATA_CELLS,
                                (&mut self.cells as *mut sys::GhosttyRenderStateRowCells).cast(),
                            ),
                        )?;
                        let row = &mut f.cells[y * cols as usize..(y + 1) * cols as usize];
                        let mut x = 0;
                        while x < row.len() && sys::ghostty_render_state_row_cells_next(self.cells) {
                            read_cell(self.cells, &mut self.utf8, &mut row[x])?;
                            x += 1;
                        }
                        for c in &mut row[x..] {
                            c.blank();
                        }
                    }
                    y += 1;
                }
                for c in &mut f.cells[y * cols as usize..] {
                    c.blank();
                }
                check("clean", sys::ghostty_render_state_clean(self.state))?;
            }

            f.cursor = (cur.visible && cur.viewport_has_value).then_some(Cursor {
                x: cur.viewport_x,
                y: cur.viewport_y,
                style: match cur.visual_style {
                    sys::GHOSTTY_RENDER_STATE_CURSOR_VISUAL_STYLE_BAR => CursorStyle::Bar,
                    sys::GHOSTTY_RENDER_STATE_CURSOR_VISUAL_STYLE_UNDERLINE => CursorStyle::Underline,
                    sys::GHOSTTY_RENDER_STATE_CURSOR_VISUAL_STYLE_BLOCK_HOLLOW => CursorStyle::HollowBlock,
                    _ => CursorStyle::Block,
                },
                blinking: cur.blinking,
            });
            f.fg = rgb(colors.foreground);
            f.bg = rgb(colors.background);
            Ok(&self.frame)
        }
    }
}

impl Cell {
    /// Back to an empty, unstyled cell, keeping the text's allocation.
    fn blank(&mut self) {
        let mut text = std::mem::take(&mut self.text);
        text.clear();
        *self = Cell {
            text,
            ..Cell::default()
        };
    }
}

/// Read the cell under the row-cells iterator into `out`, reusing its text allocation.
///
/// # Safety
/// `c` is positioned on a cell.
unsafe fn read_cell(c: sys::GhosttyRenderStateRowCells, utf8: &mut Vec<u8>, out: &mut Cell) -> Result<(), Error> {
    out.blank();
    // SAFETY (whole fn): `c` is positioned on a cell; out-pointers match each kind.
    unsafe {
        let mut raw: sys::GhosttyCell = 0;
        cell_get(c, sys::GHOSTTY_RENDER_STATE_ROW_CELLS_DATA_RAW, &mut raw)?;
        let mut wide: sys::GhosttyCellWide = 0;
        check(
            "cell wide",
            sys::ghostty_cell_get(
                raw,
                sys::GHOSTTY_CELL_DATA_WIDE,
                (&mut wide as *mut sys::GhosttyCellWide).cast(),
            ),
        )?;
        out.wide = match wide {
            sys::GHOSTTY_CELL_WIDE_WIDE => Wide::Wide,
            sys::GHOSTTY_CELL_WIDE_SPACER_TAIL | sys::GHOSTTY_CELL_WIDE_SPACER_HEAD => Wide::Tail,
            _ => Wide::Narrow,
        };

        loop {
            let mut buf = sys::GhosttyBuffer {
                ptr: utf8.as_mut_ptr(),
                cap: utf8.len(),
                len: 0,
            };
            let rc = sys::ghostty_render_state_row_cells_get(
                c,
                sys::GHOSTTY_RENDER_STATE_ROW_CELLS_DATA_GRAPHEMES_UTF8,
                (&mut buf as *mut sys::GhosttyBuffer).cast(),
            );
            if rc == sys::GHOSTTY_OUT_OF_SPACE {
                utf8.resize(buf.len, 0);
                continue;
            }
            check("cell text", rc)?;
            out.text.push_str(&String::from_utf8_lossy(&utf8[..buf.len]));
            break;
        }

        let mut styled = false;
        cell_get(c, sys::GHOSTTY_RENDER_STATE_ROW_CELLS_DATA_HAS_STYLING, &mut styled)?;
        if styled {
            let mut s = sized!(sys::GhosttyStyle);
            cell_get(c, sys::GHOSTTY_RENDER_STATE_ROW_CELLS_DATA_STYLE, &mut s)?;
            out.bold = s.bold;
            out.italic = s.italic;
            out.faint = s.faint;
            out.inverse = s.inverse;
            out.strike = s.strikethrough;
            out.invisible = s.invisible;
            out.underline = s.underline != 0;
            let mut c_rgb: sys::GhosttyColorRgb = std::mem::zeroed();
            // INVALID_VALUE means "no explicit colour": keep None.
            if sys::ghostty_render_state_row_cells_get(
                c,
                sys::GHOSTTY_RENDER_STATE_ROW_CELLS_DATA_FG_COLOR,
                (&mut c_rgb as *mut sys::GhosttyColorRgb).cast(),
            ) == sys::GHOSTTY_SUCCESS
            {
                out.fg = Some(rgb(c_rgb));
            }
        }
        // Background even without a style: cells erased while a background colour was set
        // (ED/EL, how vim and nvim paint their background) carry only a colour.
        let mut bg: sys::GhosttyColorRgb = std::mem::zeroed();
        if sys::ghostty_render_state_row_cells_get(
            c,
            sys::GHOSTTY_RENDER_STATE_ROW_CELLS_DATA_BG_COLOR,
            (&mut bg as *mut sys::GhosttyColorRgb).cast(),
        ) == sys::GHOSTTY_SUCCESS
        {
            out.bg = Some(rgb(bg));
        }
    }
    Ok(())
}

impl Drop for Vt {
    fn drop(&mut self) {
        // SAFETY: handles were created in `new` and are freed once.
        unsafe {
            if !self.mouse_event.is_null() {
                sys::ghostty_mouse_event_free(self.mouse_event);
            }
            if !self.mouse.is_null() {
                sys::ghostty_mouse_encoder_free(self.mouse);
            }
            sys::ghostty_key_event_free(self.key_event);
            sys::ghostty_key_encoder_free(self.keys);
            sys::ghostty_render_state_row_cells_free(self.cells);
            sys::ghostty_render_state_row_iterator_free(self.rows_it);
            sys::ghostty_render_state_free(self.state);
            sys::ghostty_terminal_free(self.term);
        }
    }
}

unsafe fn get<T>(state: sys::GhosttyRenderState, kind: sys::GhosttyRenderStateData, out: &mut T) -> Result<(), Error> {
    // SAFETY: caller passes the type documented for `kind`.
    check("render_state_get", unsafe {
        sys::ghostty_render_state_get(state, kind, (out as *mut T).cast())
    })
}

unsafe fn cell_get<T>(
    cells: sys::GhosttyRenderStateRowCells,
    kind: sys::GhosttyRenderStateRowCellsData,
    out: &mut T,
) -> Result<(), Error> {
    // SAFETY: caller passes the type documented for `kind`.
    check("row_cells_get", unsafe {
        sys::ghostty_render_state_row_cells_get(cells, kind, (out as *mut T).cast())
    })
}

fn rgb(c: sys::GhosttyColorRgb) -> Rgb {
    Rgb(c.r, c.g, c.b)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_styles_wide_and_cursor() {
        let mut vt = Vt::new(20, 4).unwrap();
        vt.write(b"Hi \x1b[1;31mred\x1b[0m\r\n\x1b[38;2;255;128;0morange\x1b[0m\r\n\xe4\xb8\xad!");
        let f = vt.frame().unwrap();
        assert_eq!((f.cols, f.rows), (20, 4));
        let row = |y: u16| (0..f.cols).map(|x| f.cell(x, y).text.clone()).collect::<String>();
        assert_eq!(row(0).trim_end(), "Hi red");
        let r = f.cell(3, 0);
        assert_eq!(r.text, "r");
        assert!(r.bold);
        assert!(r.fg.is_some());
        assert_eq!(f.cell(0, 1).fg, Some(Rgb(255, 128, 0)));
        assert_eq!(f.cell(0, 2).text, "中");
        assert_eq!(f.cell(0, 2).wide, Wide::Wide);
        assert_eq!(f.cell(1, 2).wide, Wide::Tail);
        assert_eq!(f.cell(2, 2).text, "!");
        let c = f.cursor.unwrap();
        assert_eq!((c.x, c.y), (3, 2));
    }

    #[test]
    fn background_colours_resolve_palette_and_rgb() {
        let mut vt = Vt::new(10, 1).unwrap();
        vt.write(b"\x1b[48;5;196mA\x1b[48;2;1;2;3mB\x1b[0mC");
        let f = vt.frame().unwrap();
        assert_eq!(f.cell(0, 0).bg, Some(Rgb(255, 0, 0)));
        assert_eq!(f.cell(1, 0).bg, Some(Rgb(1, 2, 3)));
        assert_eq!(f.cell(2, 0).bg, None);
    }

    #[test]
    fn replies_title_bell_and_modes() {
        let mut vt = Vt::new(20, 4).unwrap();
        vt.write(b"\x1b]2;my title\x07\x07\x1b[?2004h\x1b[?2048h");
        assert_eq!(vt.take_title().as_deref(), Some("my title"));
        assert_eq!(vt.take_title(), None, "only reported once");
        assert!(vt.take_bell());
        assert!(vt.mode(2004, false));
        // Enabling 2048 makes the emulator report its size in-band.
        let r = String::from_utf8(vt.take_replies()).unwrap();
        assert!(r.contains("\x1b[48;4;20;"), "{r:?}");
        vt.resize(30, 5).unwrap();
        let r = String::from_utf8(vt.take_replies()).unwrap();
        assert!(r.contains("\x1b[48;5;30;"), "size report after resize: {r:?}");
    }

    #[test]
    fn screen_lines_include_history_and_line_up_with_rows() {
        let mut vt = Vt::new(10, 4).unwrap();
        for i in 0..30 {
            vt.write(format!("line {i}\r\n").as_bytes());
        }
        // Soft-wrapped: 14 characters in a 10-column terminal take two rows.
        vt.write(b"abcdefghijklmn");
        let lines = vt.screen_lines();
        let (total, top, len) = vt.scrollbar();
        assert_eq!(lines.len(), total, "a line per row");
        assert_eq!(len, 4);
        assert_eq!(top, total - 4, "at the bottom");
        assert_eq!(lines[0], "line 0", "history first");
        assert_eq!(lines[29], "line 29");
        assert_eq!(lines[30], "abcdefghij", "a wrapped row stays a row");
        assert_eq!(lines[31], "klmn");
        // scroll_to puts a row at the top of the viewport.
        vt.scroll_to(5);
        assert_eq!(vt.scrollbar().1, 5);
        let f = vt.frame().unwrap();
        let row0: String = (0..6).map(|x| f.cell(x, 0).text.clone()).collect();
        assert_eq!(row0, "line 5");
        vt.scroll_bottom();
        assert_eq!(vt.scrollbar().1, total - 4);
    }

    #[test]
    fn desktop_notifications() {
        let mut vt = Vt::new(20, 4).unwrap();
        // OSC 9 (body only), OSC 777 (title and body), and OSC 9;4 progress, which isn't one.
        vt.write(b"\x1b]9;build done\x07\x1b]777;notify;Claude;needs you\x1b\\\x1b]9;4;1;50\x07");
        let n = vt.take_notifications();
        assert_eq!(
            n,
            vec![
                Notification {
                    title: String::new(),
                    body: "build done".into()
                },
                Notification {
                    title: "Claude".into(),
                    body: "needs you".into()
                },
            ]
        );
        assert!(vt.take_notifications().is_empty(), "only reported once");
        for _ in 0..40 {
            vt.write(b"\x1b]9;spam\x07");
        }
        assert_eq!(vt.take_notifications().len(), NOTES_MAX, "capped");
    }

    #[test]
    fn paste_follows_bracketed_mode() {
        let mut vt = Vt::new(20, 4).unwrap();
        assert_eq!(vt.encode_paste("a\nb"), b"a\rb");
        vt.write(b"\x1b[?2004h");
        assert_eq!(vt.encode_paste("a\nb"), b"\x1b[200~a\nb\x1b[201~");
    }

    #[test]
    fn scrolling_shows_history() {
        let mut vt = Vt::new(10, 2).unwrap();
        vt.write(b"one\r\ntwo\r\nthree");
        assert_eq!(vt.frame().unwrap().cell(0, 0).text, "t");
        vt.scroll(-1);
        assert_eq!(vt.frame().unwrap().cell(0, 0).text, "o");
        vt.scroll_bottom();
        assert_eq!(vt.frame().unwrap().cell(0, 1).text, "t");
    }

    #[test]
    fn erased_cells_keep_the_background_colour() {
        // nvim with a colour scheme: set a background, clear the screen, draw.
        let mut vt = Vt::new(10, 5).unwrap();
        vt.write(b"\x1b[48;2;1;2;3m\x1b[2J\x1b[1;1HX\x1b[K\x1b[0m");
        let f = vt.frame().unwrap();
        assert_eq!(f.cell(0, 0).bg, Some(Rgb(1, 2, 3)));
        assert_eq!(f.cell(5, 0).bg, Some(Rgb(1, 2, 3)), "erased to end of line");
        assert_eq!(
            f.cell(9, 4).bg,
            Some(Rgb(1, 2, 3)),
            "erased screen, bottom-right corner"
        );
        assert_eq!(f.cell(9, 4).text, "");
    }

    #[test]
    fn resize_changes_grid() {
        let mut vt = Vt::new(10, 3).unwrap();
        vt.resize(30, 8).unwrap();
        let f = vt.frame().unwrap();
        assert_eq!((f.cols, f.rows, f.cells.len()), (30, 8, 240));
    }

    /// Every capture, incremental or not, must equal a fresh full one of the same screen.
    #[test]
    fn incremental_frames_match_full_captures() {
        let mut vt = Vt::new(12, 5).unwrap();
        let steps: &[&[u8]] = &[
            b"one\r\ntwo\r\nthree",
            b"\x1b[2;1H\x1b[31mTWO\x1b[0m",
            b"\r\nfour\r\nfive\r\nsix",
            b"\x1b[2;4r\x1b[4;1H\nsc\x1b[r",
            b"\x1b]4;1;rgb:12/34/56\x07\x1b[31mp\x1b[0m",
            b"\x1b[?1049h\x1b[2Jalt",
            b"\x1b[?1049l",
            b"\x1b[3;3H\x1b[K",
            b"\x1b[2J\x1b[H\xe4\xb8\xad",
        ];
        for (i, s) in steps.iter().enumerate() {
            vt.write(s);
            let inc = vt.frame().unwrap().cells.clone();
            vt.frame.cols = 0; // forget the cache: next capture is a full one
            let full = vt.frame().unwrap().cells.clone();
            assert_eq!(inc, full, "after step {i}");
        }
        vt.resize(8, 3).unwrap();
        assert_eq!(vt.frame().unwrap().cells.len(), 24);
    }

    #[test]
    fn synchronized_output_shows_the_finished_frame() {
        let mut vt = Vt::new(10, 2).unwrap();
        vt.write(b"old");
        assert_eq!(vt.frame().unwrap().cell(0, 0).text, "o");
        // The program starts a frame and is half-way through drawing it.
        vt.write(b"\x1b[?2026h\x1b[2J\x1b[Hne");
        assert!(vt.hold_deadline().is_some());
        let row = |vt: &mut Vt| {
            let f = vt.frame().unwrap();
            (0..3).map(|x| f.cell(x, 0).text.clone()).collect::<String>()
        };
        assert_eq!(row(&mut vt), "old", "half-drawn frame not shown");
        vt.write(b"w\x1b[?2026l");
        assert!(vt.hold_deadline().is_none());
        assert_eq!(row(&mut vt), "new");
    }

    #[test]
    fn an_overlong_hold_is_cut_short() {
        let mut vt = Vt::new(10, 2).unwrap();
        vt.write(b"\x1b[?2026hstuck");
        assert_eq!(vt.frame().unwrap().cell(0, 0).text, "");
        vt.sink.hold = Some(Instant::now() - HOLD_LIMIT);
        assert_eq!(vt.frame().unwrap().cell(0, 0).text, "s");
        assert!(vt.hold_deadline().is_none());
        assert!(!vt.mode(2026, false));
    }

    #[test]
    fn scrolling_shows_through_a_hold() {
        let mut vt = Vt::new(10, 2).unwrap();
        vt.write(b"one\r\ntwo\r\nthree\x1b[?2026h");
        assert_eq!(vt.frame().unwrap().cell(0, 0).text, "t");
        vt.scroll(-1);
        assert_eq!(vt.frame().unwrap().cell(0, 0).text, "o");
    }

    #[test]
    fn the_revision_rises_with_output_and_resizes() {
        let mut vt = Vt::new(20, 5).unwrap();
        let r0 = vt.revision();
        let _ = vt.screen_lines();
        assert_eq!(vt.revision(), r0, "reading changes nothing");
        vt.write(b"hi");
        let r1 = vt.revision();
        assert!(r1 > r0);
        vt.resize(30, 5).unwrap();
        assert!(vt.revision() > r1);
    }
}
