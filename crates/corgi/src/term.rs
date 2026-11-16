//! Puts the host terminal into TUI mode and always puts it back: on return,
//! on panic, and on SIGTERM/SIGHUP. (SIGKILL can't be caught; `reset` fixes that.)

use crossterm::event::{
    DisableBracketedPaste, DisableFocusChange, DisableMouseCapture, EnableBracketedPaste, EnableFocusChange,
    EnableMouseCapture, KeyboardEnhancementFlags, PopKeyboardEnhancementFlags, PushKeyboardEnhancementFlags,
};
use crossterm::terminal::{self, EnterAlternateScreen, LeaveAlternateScreen};
use crossterm::{cursor, execute};
use ratatui::{Terminal, backend::CrosstermBackend};
use std::io::{BufWriter, Stdout, Write, stdout};
use std::sync::atomic::{AtomicBool, Ordering};

/// Buffered, so a frame leaves in one write instead of stdout's 1 KiB line-buffer chunks.
pub type Tui = Terminal<CrosstermBackend<BufWriter<Stdout>>>;

static ACTIVE: AtomicBool = AtomicBool::new(false);
static KITTY: AtomicBool = AtomicBool::new(false);

pub struct Guard;

impl Guard {
    /// Also returns the host's colours and cell size, asked once before input reading starts.
    /// `mouse`: capture it (click, wheel) or leave it to the host terminal.
    pub fn enter(mouse: bool) -> anyhow::Result<(Guard, Tui, Option<hc_proto::Display>)> {
        install_hooks();
        terminal::enable_raw_mode()?;
        ACTIVE.store(true, Ordering::SeqCst);
        let display = crate::display::query();
        let mut out = stdout();
        execute!(out, EnterAlternateScreen, EnableBracketedPaste, EnableFocusChange)?;
        if mouse {
            execute!(out, EnableMouseCapture)?;
        }
        // Kitty keyboard protocol where the host has it: unambiguous Esc, Alt, C-i vs Tab.
        if terminal::supports_keyboard_enhancement().unwrap_or(false) {
            execute!(
                out,
                PushKeyboardEnhancementFlags(
                    KeyboardEnhancementFlags::DISAMBIGUATE_ESCAPE_CODES
                        | KeyboardEnhancementFlags::REPORT_ALTERNATE_KEYS
                )
            )?;
            KITTY.store(true, Ordering::SeqCst);
        }
        tracing::info!(kitty_keyboard = KITTY.load(Ordering::SeqCst), "terminal entered");
        let out = BufWriter::with_capacity(1 << 18, stdout());
        Ok((Guard, Terminal::new(CrosstermBackend::new(out))?, display))
    }
}

impl Drop for Guard {
    fn drop(&mut self) {
        restore();
    }
}

/// Draw one frame inside a synchronized update (mode 2026), so the host terminal shows
/// it whole instead of painting it while it arrives. Hosts without 2026 ignore the brackets.
pub fn draw(tui: &mut Tui, render: impl FnOnce(&mut ratatui::Frame)) -> std::io::Result<()> {
    use crossterm::terminal::{BeginSynchronizedUpdate, EndSynchronizedUpdate};
    crossterm::queue!(tui.backend_mut(), BeginSynchronizedUpdate)?;
    tui.draw(render)?;
    crossterm::queue!(tui.backend_mut(), EndSynchronizedUpdate)?;
    tui.backend_mut().flush()
}

/// Ring the host terminal's bell (a pane rang its own).
pub fn bell() {
    use std::io::Write;
    let mut out = stdout();
    let _ = out.write_all(b"\x07");
    let _ = out.flush();
}

/// Send the host terminal a notification sequence (see `host_notify`). Between frames, so
/// it can't land inside one.
pub fn host_write(bytes: &[u8]) {
    use std::io::Write;
    if bytes.is_empty() {
        return;
    }
    let mut out = stdout();
    let _ = out.write_all(bytes);
    let _ = out.flush();
}

/// Idempotent: safe from Drop, the panic hook and the signal thread.
pub fn restore() {
    if !ACTIVE.swap(false, Ordering::SeqCst) {
        return;
    }
    let mut out = stdout();
    if KITTY.swap(false, Ordering::SeqCst) {
        let _ = execute!(out, PopKeyboardEnhancementFlags);
    }
    let _ = execute!(
        out,
        DisableFocusChange,
        DisableBracketedPaste,
        DisableMouseCapture,
        LeaveAlternateScreen,
        cursor::Show
    );
    let _ = terminal::disable_raw_mode();
}

/// Take the mouse from the host terminal (corgi's), or give it back (`ui.mouse`).
pub fn mouse(on: bool) {
    let mut out = std::io::stdout();
    let _ = if on {
        execute!(out, EnableMouseCapture)
    } else {
        execute!(out, DisableMouseCapture)
    };
}

fn install_hooks() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        let prev = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            restore();
            tracing::error!("panic: {info}");
            prev(info);
        }));
        #[cfg(unix)]
        {
            use signal_hook::consts::{SIGHUP, SIGTERM};
            if let Ok(mut signals) = signal_hook::iterator::Signals::new([SIGTERM, SIGHUP]) {
                std::thread::spawn(move || {
                    if let Some(sig) = signals.forever().next() {
                        tracing::info!(sig, "signal: restoring terminal");
                        restore();
                        std::process::exit(128 + sig);
                    }
                });
            }
        }
    });
}
