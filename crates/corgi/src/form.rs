//! Modal forms (the New Workspace / New Window dialogs, …): fields, editing, focus,
//! validation messages, drawing. Pure apart from listing directories for path completion.
//!
//! Keys: Tab / S-Tab (or ↓ / ↑) move between fields, Enter submits, Esc cancels. Text and
//! path fields edit emacs-style (C-a C-e C-b C-f C-d C-k C-u C-w, arrows, Home/End). In a
//! path field Tab completes first: to the only match, or the matches' common start, and
//! moves on only when there's nothing to add. Selects take ← / → (or Space); toggles Space.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Widget};
use unicode_width::UnicodeWidthStr;

use crate::theme::Theme;

#[derive(Clone, Debug, PartialEq)]
pub enum Kind {
    Text,
    /// A folder: Tab completes against the directories on disk (when `local`).
    Path {
        local: bool,
    },
    /// One of `options` (value, label).
    Select {
        options: Vec<(String, String)>,
        at: usize,
    },
    /// On or off.
    Toggle(bool),
}

#[derive(Clone, Debug)]
pub struct Field {
    pub key: &'static str,
    pub label: String,
    pub kind: Kind,
    /// Text and path fields: the value, and the cursor in it (in chars).
    pub text: String,
    pub cursor: usize,
    /// Shown dimmed in an empty text field.
    pub placeholder: String,
    /// Under the field: what Tab could complete to, or a note.
    pub note: Option<String>,
    /// Not shown, and skipped by Tab (a field only some choices need).
    pub hidden: bool,
}

impl Field {
    pub fn text(key: &'static str, label: &str, value: &str, placeholder: &str) -> Field {
        Field {
            key,
            label: label.into(),
            kind: Kind::Text,
            text: value.into(),
            cursor: value.chars().count(),
            placeholder: placeholder.into(),
            note: None,
            hidden: false,
        }
    }

    pub fn path(key: &'static str, label: &str, value: &str, local: bool) -> Field {
        Field {
            kind: Kind::Path { local },
            ..Field::text(key, label, value, "")
        }
    }

    pub fn select(key: &'static str, label: &str, options: Vec<(String, String)>, at: usize) -> Field {
        let at = at.min(options.len().saturating_sub(1));
        Field {
            kind: Kind::Select { options, at },
            ..Field::text(key, label, "", "")
        }
    }

    pub fn toggle(key: &'static str, label: &str, on: bool) -> Field {
        Field {
            kind: Kind::Toggle(on),
            ..Field::text(key, label, "", "")
        }
    }

    /// The value as a string: text, the selected option's value, or `"true"`/`"false"`.
    pub fn value(&self) -> String {
        match &self.kind {
            Kind::Text | Kind::Path { .. } => self.text.clone(),
            Kind::Select { options, at } => options.get(*at).map(|o| o.0.clone()).unwrap_or_default(),
            Kind::Toggle(on) => on.to_string(),
        }
    }

    fn byte(&self, char_at: usize) -> usize {
        self.text
            .char_indices()
            .nth(char_at)
            .map_or(self.text.len(), |(i, _)| i)
    }

    fn insert(&mut self, s: &str) {
        let at = self.byte(self.cursor);
        self.text.insert_str(at, s);
        self.cursor += s.chars().count();
    }

    /// Delete chars `from..to` (char positions); the cursor goes to `from`.
    fn delete(&mut self, from: usize, to: usize) {
        let (a, b) = (self.byte(from), self.byte(to));
        self.text.replace_range(a..b, "");
        self.cursor = from;
    }

    /// Edit with `k`. True if it was an editing key.
    fn edit(&mut self, k: &KeyEvent) -> bool {
        let ctrl = k.modifiers.contains(KeyModifiers::CONTROL);
        let alt = k.modifiers.contains(KeyModifiers::ALT);
        let n = self.text.chars().count();
        match k.code {
            KeyCode::Left => self.cursor = self.cursor.saturating_sub(1),
            KeyCode::Char('b') if ctrl => self.cursor = self.cursor.saturating_sub(1),
            KeyCode::Right => self.cursor = (self.cursor + 1).min(n),
            KeyCode::Char('f') if ctrl => self.cursor = (self.cursor + 1).min(n),
            KeyCode::Home => self.cursor = 0,
            KeyCode::Char('a') if ctrl => self.cursor = 0,
            KeyCode::End => self.cursor = n,
            KeyCode::Char('e') if ctrl => self.cursor = n,
            KeyCode::Backspace if self.cursor > 0 => self.delete(self.cursor - 1, self.cursor),
            KeyCode::Backspace => {}
            KeyCode::Delete if self.cursor < n => self.delete(self.cursor, self.cursor + 1),
            KeyCode::Char('d') if ctrl && self.cursor < n => self.delete(self.cursor, self.cursor + 1),
            KeyCode::Delete => {}
            KeyCode::Char('d') if ctrl => {}
            KeyCode::Char('k') if ctrl => self.delete(self.cursor, n),
            KeyCode::Char('u') if ctrl => self.delete(0, self.cursor),
            KeyCode::Char('w') if ctrl => {
                // The word before the cursor (and the separators after it), shell-style.
                let chars: Vec<char> = self.text.chars().collect();
                let mut i = self.cursor;
                while i > 0 && matches!(chars[i - 1], ' ' | '/') {
                    i -= 1;
                }
                while i > 0 && !matches!(chars[i - 1], ' ' | '/') {
                    i -= 1;
                }
                self.delete(i, self.cursor);
            }
            KeyCode::Char(c) if !ctrl && !alt => self.insert(&c.to_string()),
            _ => return false,
        }
        true
    }

    /// Complete a path field. True if it changed the text.
    fn complete(&mut self, home: &str) -> bool {
        let Kind::Path { local: true } = self.kind else {
            return false;
        };
        let names = complete_dir(&self.text, home);
        self.note = None;
        match names.as_slice() {
            // Nothing below a folder that's already complete isn't a mistake.
            [] if std::path::Path::new(&expand_home(&self.text, home)).is_dir() => false,
            [] => {
                self.note = Some("no folder starts like that".into());
                false
            }
            [one] => {
                let before = self.text.clone();
                self.text = format!("{}/", one.trim_end_matches('/'));
                self.cursor = self.text.chars().count();
                self.text != before
            }
            many => {
                let common = common_prefix(many);
                let shown: Vec<&str> = many
                    .iter()
                    .take(6)
                    .map(|m| m.trim_end_matches('/').rsplit('/').next().unwrap_or(m))
                    .collect();
                let more = if many.len() > 6 {
                    format!(" +{}", many.len() - 6)
                } else {
                    String::new()
                };
                self.note = Some(format!("{}{more}", shown.join("  ")));
                if common.chars().count() > self.text.chars().count() {
                    self.text = common;
                    self.cursor = self.text.chars().count();
                    true
                } else {
                    false
                }
            }
        }
    }
}

/// The folders `text` could be completing to, as `text` would read with each (so `~/` stays).
/// `~` is `home`. Hidden folders only when the part being typed starts with `.`.
pub fn complete_dir(text: &str, home: &str) -> Vec<String> {
    let expanded = expand_home(text, home);
    let (dir, part) = match expanded.rfind('/') {
        Some(i) => (&expanded[..=i], &expanded[i + 1..]),
        None => ("./", expanded.as_str()),
    };
    let shown_dir = match text.rfind('/') {
        Some(i) => &text[..=i],
        None => "",
    };
    let Ok(entries) = std::fs::read_dir(if dir.is_empty() { "/" } else { dir }) else {
        return Vec::new();
    };
    let mut out: Vec<String> = entries
        .filter_map(Result::ok)
        .filter(|e| e.path().is_dir())
        .filter_map(|e| e.file_name().into_string().ok())
        .filter(|n| n.starts_with(part) && (part.starts_with('.') || !n.starts_with('.')))
        .map(|n| format!("{shown_dir}{n}/"))
        .collect();
    out.sort();
    out
}

/// `~` and `~/…` as `home`.
pub fn expand_home(text: &str, home: &str) -> String {
    if text == "~" {
        home.to_string()
    } else if let Some(rest) = text.strip_prefix("~/") {
        format!("{}/{rest}", home.trim_end_matches('/'))
    } else {
        text.to_string()
    }
}

fn common_prefix(items: &[String]) -> String {
    let first = &items[0];
    let mut n = first.chars().count();
    for s in &items[1..] {
        n = n.min(first.chars().zip(s.chars()).take_while(|(a, b)| a == b).count());
    }
    first.chars().take(n).collect()
}

#[derive(Clone, Debug)]
pub struct Form {
    pub title: String,
    pub fields: Vec<Field>,
    pub focus: usize,
    /// What's wrong, after a submit that didn't validate.
    pub error: Option<String>,
    /// What Enter does, for the footer ("create").
    pub action: String,
    /// Where each field's row landed, for the mouse.
    pub hits: Vec<(Rect, usize)>,
    /// Lines above the fields; `true` for a warning.
    pub message: Vec<(String, bool)>,
}

#[derive(Debug, PartialEq)]
pub enum Outcome {
    Stay,
    Cancel,
    Submit,
}

impl Form {
    pub fn new(title: &str, action: &str, fields: Vec<Field>) -> Form {
        Form {
            title: title.into(),
            fields,
            focus: 0,
            error: None,
            action: action.into(),
            hits: Vec::new(),
            message: Vec::new(),
        }
    }

    /// A question with no fields: Enter (or `y`) goes ahead, Esc (or `n`) doesn't.
    pub fn confirm(title: &str, action: &str, message: Vec<(String, bool)>) -> Form {
        Form {
            message,
            ..Form::new(title, action, Vec::new())
        }
    }

    pub fn get(&self, key: &str) -> Option<&Field> {
        self.fields.iter().find(|f| f.key == key)
    }

    pub fn get_mut(&mut self, key: &str) -> Option<&mut Field> {
        self.fields.iter_mut().find(|f| f.key == key)
    }

    /// A field's value (`""` if there's no such field).
    pub fn value(&self, key: &str) -> String {
        self.get(key).map(Field::value).unwrap_or_default()
    }

    /// Show or hide a field (hiding the focused one moves the focus on).
    pub fn show(&mut self, key: &str, shown: bool) {
        if let Some(f) = self.get_mut(key) {
            f.hidden = !shown;
        }
        if self.fields.get(self.focus).is_some_and(|f| f.hidden) {
            self.step(1);
        }
    }

    fn step(&mut self, by: isize) {
        let n = self.fields.len() as isize;
        for _ in 0..n {
            self.focus = (self.focus as isize + by).rem_euclid(n) as usize;
            if !self.fields[self.focus].hidden {
                break;
            }
        }
    }

    pub fn key(&mut self, k: &KeyEvent, home: &str) -> Outcome {
        let ctrl = k.modifiers.contains(KeyModifiers::CONTROL);
        let Some(f) = self.fields.get_mut(self.focus) else {
            return match k.code {
                KeyCode::Enter | KeyCode::Char('y' | 'Y') if !ctrl => Outcome::Submit,
                KeyCode::Esc | KeyCode::Char('n' | 'N' | 'q') if !ctrl => Outcome::Cancel,
                KeyCode::Char('c' | 'g') if ctrl => Outcome::Cancel,
                _ => Outcome::Stay,
            };
        };
        match k.code {
            KeyCode::Esc => return Outcome::Cancel,
            KeyCode::Char('c' | 'g') if ctrl => return Outcome::Cancel,
            KeyCode::Enter => return Outcome::Submit,
            KeyCode::Tab => {
                if !f.complete(home) {
                    self.step(1);
                }
            }
            KeyCode::BackTab | KeyCode::Up => self.step(-1),
            KeyCode::Char('p') if ctrl => self.step(-1),
            KeyCode::Down => self.step(1),
            KeyCode::Char('n') if ctrl => self.step(1),
            _ => match &mut f.kind {
                Kind::Select { options, at } => match k.code {
                    KeyCode::Left => *at = (*at + options.len().max(1) - 1) % options.len().max(1),
                    KeyCode::Right | KeyCode::Char(' ') => *at = (*at + 1) % options.len().max(1),
                    _ => {}
                },
                Kind::Toggle(on) => {
                    if matches!(k.code, KeyCode::Char(' ') | KeyCode::Left | KeyCode::Right) {
                        *on = !*on;
                    }
                }
                Kind::Text | Kind::Path { .. } => {
                    if f.edit(k) {
                        f.note = None;
                        self.error = None;
                    }
                }
            },
        }
        Outcome::Stay
    }

    /// A click: focus the field under it. True if it hit one.
    pub fn click(&mut self, x: u16, y: u16) -> bool {
        let at = ratatui::layout::Position::new(x, y);
        match self.hits.iter().find(|(r, _)| r.contains(at)) {
            Some(&(_, i)) => {
                self.focus = i;
                if let Some(Kind::Toggle(on)) = self.fields.get_mut(i).map(|f| &mut f.kind) {
                    *on = !*on;
                }
                true
            }
            None => false,
        }
    }

    /// Draw centred in `area`, over a dimmed background. Returns where the text cursor goes.
    pub fn render(&mut self, area: Rect, buf: &mut Buffer, t: &Theme) -> Option<(u16, u16)> {
        // Everything behind the dialog steps back.
        for y in area.top()..area.bottom() {
            for x in area.left()..area.right() {
                if let Some(c) = buf.cell_mut((x, y)) {
                    c.modifier.insert(Modifier::DIM);
                }
            }
        }
        let label_w = self.fields.iter().map(|f| f.label.width()).max().unwrap_or(4) as u16;
        // Wide enough for its longest message line (within the screen), else up to 72.
        let longest = self.message.iter().map(|(l, _)| l.width()).max().unwrap_or(0) as u16 + 6;
        let w = area
            .width
            .saturating_sub(4)
            .clamp(30, 72)
            .max(longest.min(area.width.saturating_sub(4)));
        let notes = self.fields.iter().filter(|f| f.note.is_some() && !f.hidden).count() as u16;
        let shown = self.fields.iter().filter(|f| !f.hidden).count() as u16;
        // Border, a gap, the fields (and their notes), a gap, the error, the footer.
        let message = self.message.len() as u16 + u16::from(!self.message.is_empty() && !self.fields.is_empty());
        let h = (message + shown + notes + 2 + 1 + u16::from(self.error.is_some()) + 2).min(area.height);
        let rect = crate::ui::centered(area, w, h);
        Clear.render(rect, buf);
        let block = Block::default()
            .borders(Borders::ALL)
            .border_type(t.border_type())
            .border_style(Style::new().fg(t.prefix))
            .style(Style::new().bg(t.surface).fg(t.fg))
            .title(Line::from(vec![
                Span::raw(" "),
                Span::styled(
                    self.title.clone(),
                    Style::new().fg(t.prefix).add_modifier(Modifier::BOLD),
                ),
                Span::raw(" "),
            ]));
        let inner = block.inner(rect);
        block.render(rect, buf);
        if inner.height == 0 || inner.width < label_w + 6 {
            return None;
        }
        self.hits.clear();
        let mut cursor = None;
        let mut y = inner.y + 1;
        for (line, warn) in &self.message {
            if y >= inner.bottom() {
                break;
            }
            let style = if *warn {
                Style::new().fg(t.blocked).bg(t.surface).add_modifier(Modifier::BOLD)
            } else {
                Style::new().fg(t.fg).bg(t.surface)
            };
            buf.set_stringn(inner.x + 2, y, line, inner.width.saturating_sub(4) as usize, style);
            y += 1;
        }
        if !self.message.is_empty() && !self.fields.is_empty() {
            y += 1;
        }
        let vx = inner.x + 2 + label_w + 2;
        let vw = inner.right().saturating_sub(vx + 2);
        for (i, f) in self.fields.iter().enumerate() {
            if f.hidden {
                continue;
            }
            if y >= inner.bottom() {
                break;
            }
            let focused = i == self.focus;
            let row = Rect::new(inner.x, y, inner.width, 1);
            self.hits.push((row, i));
            let label = Style::new().fg(if focused { t.prefix } else { t.muted });
            let label = if focused {
                label.add_modifier(Modifier::BOLD)
            } else {
                label
            };
            buf.set_stringn(
                inner.x + 2,
                y,
                format!("{:>w$}", f.label, w = label_w as usize),
                label_w as usize,
                label,
            );
            let field_bg = if focused { t.accent } else { t.border };
            let field = Style::new().bg(field_bg).fg(if focused { t.accent_fg } else { t.fg });
            let value_rect = Rect::new(vx, y, vw, 1);
            buf.set_style(value_rect, field);
            match &f.kind {
                Kind::Text | Kind::Path { .. } => {
                    let shown_w = vw.saturating_sub(2) as usize;
                    // Keep the cursor in view: scroll the text left when it's long.
                    let chars: Vec<char> = f.text.chars().collect();
                    let start = f.cursor.saturating_sub(shown_w.saturating_sub(1));
                    let visible: String = chars[start.min(chars.len())..].iter().collect();
                    if f.text.is_empty() && !f.placeholder.is_empty() {
                        buf.set_stringn(
                            vx + 1,
                            y,
                            &f.placeholder,
                            shown_w,
                            field.add_modifier(Modifier::DIM | Modifier::ITALIC),
                        );
                    } else {
                        buf.set_stringn(vx + 1, y, &visible, shown_w, field);
                    }
                    if focused {
                        let before: String = chars[start.min(chars.len())..f.cursor.min(chars.len())]
                            .iter()
                            .collect();
                        cursor = Some((vx + 1 + (before.width() as u16).min(shown_w as u16), y));
                    }
                }
                Kind::Select { options, at } => {
                    let label = options.get(*at).map(|o| o.1.as_str()).unwrap_or("");
                    let arrows = if options.len() > 1 {
                        ("‹ ", " ›")
                    } else {
                        ("  ", "  ")
                    };
                    let s = format!("{}{label}{}", arrows.0, arrows.1);
                    buf.set_stringn(vx, y, s, vw as usize, field);
                }
                Kind::Toggle(on) => {
                    let s = if *on { " [x] yes" } else { " [ ] no" };
                    buf.set_stringn(vx, y, s, vw as usize, field);
                }
            }
            y += 1;
            if let Some(note) = &f.note
                && y < inner.bottom()
            {
                buf.set_stringn(vx + 1, y, note, vw as usize, Style::new().fg(t.muted).bg(t.surface));
                y += 1;
            }
        }
        y += 1;
        if let Some(e) = &self.error
            && y < inner.bottom()
        {
            buf.set_stringn(
                inner.x + 2,
                y,
                format!("✗ {e}"),
                inner.width.saturating_sub(4) as usize,
                Style::new().fg(t.blocked).bg(t.surface).add_modifier(Modifier::BOLD),
            );
        }
        let footer = if self.fields.is_empty() {
            format!("Enter or y: {} · Esc or n: cancel", self.action)
        } else if self.fields.len() == 1 {
            format!("Enter {} · Esc cancel", self.action)
        } else {
            format!("Tab next · Enter {} · Esc cancel", self.action)
        };
        let fw = (footer.width() as u16).min(inner.width);
        buf.set_stringn(
            inner.x + (inner.width - fw) / 2,
            inner.bottom() - 1,
            &footer,
            fw as usize,
            Style::new().fg(t.muted).bg(t.surface),
        );
        cursor
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(c: KeyCode) -> KeyEvent {
        KeyEvent::new(c, KeyModifiers::NONE)
    }
    fn ctrl(c: char) -> KeyEvent {
        KeyEvent::new(KeyCode::Char(c), KeyModifiers::CONTROL)
    }
    fn typ(f: &mut Form, s: &str) {
        for c in s.chars() {
            f.key(&key(KeyCode::Char(c)), "/home");
        }
    }

    fn form() -> Form {
        Form::new(
            "New Window",
            "create",
            vec![
                Field::text("name", "Name", "", "auto"),
                Field::select(
                    "prog",
                    "Program",
                    vec![("".into(), "shell".into()), ("claude".into(), "claude".into())],
                    0,
                ),
                Field::toggle("wt", "Worktree", false),
            ],
        )
    }

    #[test]
    fn editing_emacs_style() {
        let mut f = form();
        typ(&mut f, "hello world");
        assert_eq!(f.value("name"), "hello world");
        f.key(&ctrl('w'), "/home");
        assert_eq!(f.value("name"), "hello ");
        f.key(&ctrl('a'), "/home");
        typ(&mut f, ">");
        f.key(&ctrl('e'), "/home");
        typ(&mut f, "x");
        assert_eq!(f.value("name"), ">hello x");
        f.key(&key(KeyCode::Left), "/home");
        f.key(&key(KeyCode::Left), "/home");
        f.key(&ctrl('k'), "/home");
        assert_eq!(f.value("name"), ">hello");
        f.key(&ctrl('u'), "/home");
        assert_eq!(f.value("name"), "");
        typ(&mut f, "é—x");
        f.key(&key(KeyCode::Backspace), "/home");
        assert_eq!(f.value("name"), "é—", "by characters, not bytes");
    }

    #[test]
    fn moving_selecting_toggling_submitting() {
        let mut f = form();
        assert_eq!(f.key(&key(KeyCode::Tab), "/home"), Outcome::Stay);
        assert_eq!(f.focus, 1);
        f.key(&key(KeyCode::Right), "/home");
        assert_eq!(f.value("prog"), "claude");
        f.key(&key(KeyCode::Right), "/home");
        assert_eq!(f.value("prog"), "", "wraps");
        f.key(&key(KeyCode::Down), "/home");
        f.key(&key(KeyCode::Char(' ')), "/home");
        assert_eq!(f.value("wt"), "true");
        f.key(&key(KeyCode::Tab), "/home");
        assert_eq!(f.focus, 0, "wraps to the first");
        f.key(&key(KeyCode::BackTab), "/home");
        assert_eq!(f.focus, 2);
        assert_eq!(f.key(&key(KeyCode::Enter), "/home"), Outcome::Submit);
        assert_eq!(f.key(&key(KeyCode::Esc), "/home"), Outcome::Cancel);
    }

    #[test]
    fn path_completion() {
        let root = std::env::temp_dir().join(format!("corgi-form-{}", std::process::id()));
        for d in ["projects/hermit", "projects/herdr", "projects/api", "notes", ".hidden"] {
            std::fs::create_dir_all(root.join(d)).unwrap();
        }
        std::fs::write(root.join("projects/file.txt"), "").unwrap();
        let home = root.display().to_string();
        assert_eq!(complete_dir("~/zz", &home), Vec::<String>::new());
        assert_eq!(
            complete_dir("~/", &home),
            vec!["~/notes/", "~/projects/"],
            "no hidden ones, no files"
        );
        assert_eq!(complete_dir("~/.h", &home), vec!["~/.hidden/"]);
        assert_eq!(
            complete_dir("~/projects/her", &home),
            vec!["~/projects/herdr/", "~/projects/hermit/"]
        );

        let mut f = Form::new(
            "x",
            "go",
            vec![
                Field::path("dir", "Folder", "~/pro", true),
                Field::text("n", "Name", "", ""),
            ],
        );
        f.key(&key(KeyCode::Tab), &home);
        assert_eq!(f.value("dir"), "~/projects/", "the only match");
        typ(&mut f, "h");
        f.key(&key(KeyCode::Tab), &home);
        assert_eq!(f.value("dir"), "~/projects/her", "the common start");
        assert_eq!(f.fields[0].note.as_deref(), Some("herdr  hermit"));
        assert_eq!(f.focus, 0);
        f.key(&key(KeyCode::Tab), &home);
        assert_eq!(f.focus, 1, "nothing more to add: the next field");
        let mut g = Form::new(
            "x",
            "go",
            vec![
                Field::path("dir", "Folder", "~/notes/", true),
                Field::text("n", "Name", "", ""),
            ],
        );
        g.key(&key(KeyCode::Tab), &home);
        assert_eq!(
            (g.focus, g.fields[0].note.clone()),
            (1, None),
            "a folder with none inside: on, no complaint"
        );
        let mut g = Form::new("x", "go", vec![Field::path("dir", "Folder", "~/zz", true)]);
        g.key(&key(KeyCode::Tab), &home);
        assert_eq!(g.fields[0].note.as_deref(), Some("no folder starts like that"));
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn hidden_fields_are_skipped() {
        let mut f = form();
        f.show("prog", false);
        f.key(&key(KeyCode::Tab), "/");
        assert_eq!(f.focus, 2, "over the hidden one");
        f.focus = 1;
        f.show("prog", false);
        assert_eq!(f.focus, 2, "hiding the focused field moves on");
        let theme = crate::theme::builtin("terminal").unwrap();
        let area = Rect::new(0, 0, 60, 14);
        let mut buf = Buffer::empty(area);
        f.render(area, &mut buf, &theme);
        let all: String = (0..14)
            .map(|y| (0..60).map(|x| buf[(x, y)].symbol().to_string()).collect::<String>() + "\n")
            .collect();
        assert!(!all.contains("Program") && all.contains("Worktree"), "{all}");
    }

    #[test]
    fn confirm_questions() {
        let mut f = Form::confirm(
            "Close Pane",
            "close",
            vec![("Close t3?".into(), false), ("⚠ claude is working".into(), true)],
        );
        assert_eq!(f.key(&key(KeyCode::Char('x')), "/"), Outcome::Stay);
        assert_eq!(f.key(&key(KeyCode::Char('y')), "/"), Outcome::Submit);
        assert_eq!(f.key(&key(KeyCode::Enter), "/"), Outcome::Submit);
        assert_eq!(f.key(&key(KeyCode::Char('n')), "/"), Outcome::Cancel);
        assert_eq!(f.key(&key(KeyCode::Esc), "/"), Outcome::Cancel);
        let theme = crate::theme::builtin("terminal").unwrap();
        let area = Rect::new(0, 0, 60, 14);
        let mut buf = Buffer::empty(area);
        f.render(area, &mut buf, &theme);
        let text: Vec<String> = (0..14)
            .map(|y| (0..60).map(|x| buf[(x, y)].symbol().to_string()).collect())
            .collect();
        let at = text
            .iter()
            .position(|l| l.contains("⚠ claude is working"))
            .expect("the warning");
        assert_eq!(
            buf[(
                text[at].find('⚠').map(|b| text[at][..b].chars().count()).unwrap() as u16,
                at as u16
            )]
                .fg,
            theme.blocked
        );
        assert!(text.iter().any(|l| l.contains("Enter or y: close")), "{text:#?}");
    }

    #[test]
    fn renders_fields_error_and_footer() {
        let theme = crate::theme::builtin("terminal").unwrap();
        let area = Rect::new(0, 0, 80, 20);
        let mut buf = Buffer::empty(area);
        let mut f = form();
        typ(&mut f, "logs");
        f.error = Some("no such folder".into());
        let cursor = f.render(area, &mut buf, &theme);
        let text: Vec<String> = (0..20)
            .map(|y| (0..80).map(|x| buf[(x, y)].symbol().to_string()).collect())
            .collect();
        let all = text.join("\n");
        assert!(all.contains("New Window"), "{all}");
        assert!(text.iter().any(|l| l.contains("Name") && l.contains("logs")), "{all}");
        assert!(
            text.iter().any(|l| l.contains("Program") && l.contains("‹ shell ›")),
            "{all}"
        );
        assert!(
            text.iter().any(|l| l.contains("Worktree") && l.contains("[ ] no")),
            "{all}"
        );
        assert!(all.contains("✗ no such folder"));
        assert!(all.contains("Enter create"));
        let (cx, cy) = cursor.unwrap();
        assert!(text[cy as usize].contains("logs"));
        assert_eq!(
            &text[cy as usize][..].chars().nth(cx as usize - 1),
            &Some('s'),
            "after the text"
        );
        // Clicking a row focuses it.
        let (r, i) = f.hits[2];
        assert!(f.click(r.x + 3, r.y));
        assert_eq!(f.focus, i);
        assert_eq!(f.value("wt"), "true", "and flips a toggle");
    }
}
