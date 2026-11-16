//! Popups (`display-popup`): a terminal floating over everything, its own core terminal (not in
//! the layout: it has no workspace labels), taking every key until its program exits. `C-b x`
//! closes it; `C-b C-b` sends the prefix through. Also `run-shell` (a line in the background)
//! and `edit-scrollback` (`C-b e`: the pane's history in `$EDITOR`, in a popup).

use super::*;
use crate::commands::Size;
use ratatui::style::{Modifier, Style};
use ratatui::widgets::{Block, Borders, Clear};

/// The label that marks a terminal as a popup's.
pub const L_POPUP: &str = "corgi.popup";

#[derive(Debug)]
pub(super) struct Popup {
    pub addr: String,
    pub width: Size,
    pub height: Size,
    pub title: String,
    /// Close when the program exits (`-E`); else say so and wait for a key.
    pub close: bool,
    pub exited: Option<String>,
    /// The prefix was pressed: the next key is corgi's (`x` closes, the prefix again sends it).
    pub prefix: bool,
    /// A file to remove when it closes (`edit-scrollback`'s).
    pub tmp: Option<std::path::PathBuf>,
}

/// What `display-popup` asked for, before its terminal exists.
#[derive(Debug)]
pub struct Ask {
    pub cmd: Option<String>,
    pub cwd: Option<String>,
    pub machine: Option<String>,
    pub width: Size,
    pub height: Size,
    pub title: String,
    pub close: bool,
    pub tmp: Option<std::path::PathBuf>,
}

impl App {
    /// The popup's box and what's inside it, centred on the screen.
    fn popup_rects(&self, width: Size, height: Size) -> (Rect, Rect) {
        let s = self.screen;
        let w = width.of(s.width).clamp(10.min(s.width), s.width);
        let h = height.of(s.height).clamp(5.min(s.height), s.height);
        let outer = crate::ui::centered(s, w, h);
        let inner = Rect::new(
            outer.x + 1,
            outer.y + 1,
            outer.width.saturating_sub(2),
            outer.height.saturating_sub(2),
        );
        (outer, inner)
    }

    pub(super) fn open_popup(&mut self, ask: Ask) {
        if self.popup.is_some() {
            self.notify("a popup is open already".into());
            return;
        }
        let (_, inner) = self.popup_rects(ask.width, ask.height);
        let mut params = json!({
            "cwd": ask.cwd,
            "on": ask.machine,
            "keep": false,
            "labels": {L_POPUP: "1"},
            "size": {"cols": inner.width.max(1), "rows": inner.height.max(1)},
            "env": {"TERM": "xterm-256color", "COLORTERM": "truecolor", "TERM_PROGRAM": "corgi",
                    "TERM_PROGRAM_VERSION": env!("CARGO_PKG_VERSION")},
        });
        if let Some(line) = &ask.cmd {
            params["line"] = json!(line);
        }
        let (c, out) = (self.client.clone(), self.out.clone());
        tokio::spawn(async move {
            let r = c.call_as::<Terminal>("terminal.run", params).await;
            let _ = out.send(Msg::PopupStarted(r, ask));
        });
    }

    /// Its terminal exists: show it and give it the keys.
    pub(super) fn popup_started(&mut self, r: Result<Terminal, hc_proto::Error>, ask: Ask) {
        let t = match r {
            Ok(t) => t,
            Err(e) => {
                self.notify(format!("no popup: {e}"));
                return;
            }
        };
        let (_, inner) = self.popup_rects(ask.width, ask.height);
        let addr = t.address.clone();
        // Its events may have come first (a program that ends at once): theirs is newer.
        self.terminals.entry(addr.clone()).or_insert(t);
        match Pane::new(addr.clone(), (inner.width.max(1), inner.height.max(1))) {
            Ok(mut p) => {
                p.attach(&self.client, self.display.as_ref(), false, self.out.clone());
                p.focus(true);
                self.panes.insert(addr.clone(), p);
            }
            Err(e) => {
                self.notify(format!("no popup: {e}"));
                return;
            }
        }
        self.popup = Some(Popup {
            addr: addr.clone(),
            width: ask.width,
            height: ask.height,
            title: ask.title,
            close: ask.close,
            exited: None,
            prefix: false,
            tmp: ask.tmp,
        });
        self.mode = Mode::Popup;
        // It ended before it was shown.
        if let Some(t) = self.terminals.get(&addr).filter(|t| !t.is_live()) {
            let code = t.exit_code;
            self.popup_exited(code);
        }
    }

    /// Its program ended: close, or say so and wait for a key.
    pub(super) fn popup_exited(&mut self, code: Option<i64>) {
        let Some(p) = &mut self.popup else { return };
        if p.close {
            self.close_popup();
        } else {
            p.exited = Some(match code {
                Some(c) => format!("exited ({c}): any key closes"),
                None => "ended: any key closes".into(),
            });
        }
    }

    pub(super) fn close_popup(&mut self) {
        let Some(p) = self.popup.take() else { return };
        self.panes.remove(&p.addr);
        let live = self.terminals.get(&p.addr).is_some_and(|t| t.is_live());
        let (c, addr) = (self.client.clone(), p.addr.clone());
        tokio::spawn(async move {
            if live {
                let _ = c.call("terminal.kill", json!({"terminal": addr})).await;
            }
            let _ = c.call("terminal.rm", json!({"terminal": addr})).await;
        });
        if let Some(f) = p.tmp {
            let _ = std::fs::remove_file(f);
        }
        if matches!(self.mode, Mode::Popup) {
            self.mode = Mode::Normal;
        }
        self.sync();
    }

    pub(super) fn popup_key(&mut self, k: KeyEvent) {
        self.mode = Mode::Popup;
        let prefix = self.keymap.prefix.matches(&k);
        let Some(p) = &mut self.popup else {
            self.mode = Mode::Normal;
            return;
        };
        if p.exited.is_some() {
            return self.close_popup();
        }
        if std::mem::take(&mut p.prefix) {
            match k.code {
                KeyCode::Char('x') => return self.close_popup(),
                _ if prefix => {}
                _ => return,
            }
        } else if prefix {
            p.prefix = true;
            return;
        }
        let addr = p.addr.clone();
        if let (Some(key), Some(pane)) = (input::to_vt(&k), self.panes.get_mut(&addr)) {
            let bytes = pane.vt.encode_key(&key);
            pane.input(&bytes);
        }
    }

    /// A paste while it's up: into it.
    pub(super) fn popup_paste(&mut self, text: &str) {
        let Some(addr) = self.popup.as_ref().map(|p| p.addr.clone()) else {
            return;
        };
        if let Some(p) = self.panes.get_mut(&addr) {
            let bytes = p.vt.encode_paste(text);
            p.input(&bytes);
        }
    }

    pub(super) fn draw_popup(&mut self, f: &mut ratatui::Frame) {
        let Some(pop) = &self.popup else { return };
        let (outer, inner) = self.popup_rects(pop.width, pop.height);
        let t = self.theme;
        let mut title = format!(" {} ", pop.title);
        if let Some(e) = &pop.exited {
            title.push_str(&format!("· {e} "));
        } else if pop.prefix {
            title.push_str("· x closes, C-b sends C-b ");
        }
        let addr = pop.addr.clone();
        f.render_widget(Clear, outer);
        f.render_widget(
            Block::default()
                .borders(Borders::ALL)
                .border_type(t.border_type())
                .border_style(Style::new().fg(t.prefix))
                .title(ratatui::text::Span::styled(
                    title,
                    Style::new().fg(t.prefix).add_modifier(Modifier::BOLD),
                )),
            outer,
        );
        let Some(p) = self.panes.get_mut(&addr) else { return };
        let _ = p.resize((inner.width.max(1), inner.height.max(1)));
        if let Ok(frame) = p.vt.frame()
            && let Some(c) = crate::ui::draw_frame(f.buffer_mut(), inner, frame)
        {
            f.set_cursor_position(c);
        }
    }

    /// `run-shell`: a line in the background, on the workspace's machine, in the focused pane's
    /// folder; what it prints first (or why it failed) is shown.
    pub(super) fn run_shell(&mut self, line: String) {
        let machine = self
            .active()
            .and_then(|(ws, _)| self.doc.workspace(&ws).and_then(|s| s.machine.clone()));
        let params = json!({"line": line, "on": machine, "cwd": self.focused_cwd(), "timeout": 60});
        let (c, out) = (self.client.clone(), self.out.clone());
        tokio::spawn(async move {
            let text = match c.call("machine.exec", params).await {
                Err(e) => format!("run-shell: {e}"),
                Ok(v) => {
                    let text = |k: &str| {
                        let raw = crate::worktree::unbase64(v[k].as_str().unwrap_or_default()).unwrap_or_default();
                        crate::toast::clean(
                            String::from_utf8_lossy(&raw)
                                .lines()
                                .find(|l| !l.trim().is_empty())
                                .unwrap_or(""),
                        )
                    };
                    match v["code"].as_i64() {
                        Some(0) => {
                            let o = text("stdout");
                            if o.is_empty() { format!("{line}: done") } else { o }
                        }
                        code => format!(
                            "{line}: exit {} {}",
                            code.map_or("?".into(), |c| c.to_string()),
                            text("stderr")
                        ),
                    }
                }
            };
            let _ = out.send(Msg::Note(text));
        });
    }

    /// `C-b e`: the focused pane's scrollback, in `$VISUAL` / `$EDITOR` (else vi), at its end, in
    /// a popup; the file goes when the popup does.
    pub(super) fn edit_scrollback(&mut self) {
        let Some(w) = self.window() else { return };
        let pane = w.focus.clone();
        let Some(p) = self.panes.get_mut(&pane) else { return };
        let mut lines = p.vt.screen_lines();
        while lines.last().is_some_and(|l| l.is_empty()) {
            lines.pop();
        }
        let file = std::env::temp_dir().join(format!(
            "corgi-scrollback-{}-{}.txt",
            std::process::id(),
            pane.replace('/', "-")
        ));
        if let Err(e) = std::fs::write(&file, lines.join("\n") + "\n") {
            self.notify(format!("can't write the scrollback: {e}"));
            return;
        }
        let editor = ["VISUAL", "EDITOR"]
            .iter()
            .find_map(|v| std::env::var(v).ok().filter(|e| !e.trim().is_empty()))
            .unwrap_or_else(|| "vi".into());
        let path = crate::worktree::quote(&file.display().to_string());
        self.open_popup(Ask {
            cmd: Some(format!("{editor} +{} {path}", lines.len().max(1))),
            cwd: None,
            machine: None,
            width: Size::Percent(90),
            height: Size::Percent(90),
            title: format!("scrollback of {}", self.pane_name(&pane)),
            close: true,
            tmp: Some(file),
        });
    }
}
