//! Copy mode (`C-b [`): move through a pane's scrollback with vi keys, search it, select and
//! copy. Pure: it works on the pane's rows as text (`corgi_vt::Vt::screen_lines`, history first,
//! one line per row); the app scrolls the pane to `top`, so the pane keeps its colours, and draws
//! the cursor and selection over it.
//!
//! Keys: `h j k l` and the arrows, `w b e` (words), `W B E` (WORDs: anything but spaces),
//! `0 ^ $` (line), `f F t T` a character on the line (`; ,` again), `{ }` (paragraphs: to the
//! blank line before or after), `gg G` (all), `H M L` (screen), `C-u C-d` (half a screen),
//! `C-b C-f` / PageUp PageDown (a screen; `C-b` only when it isn't the prefix: the prefix
//! leaves copy mode and starts, as in tmux); a count before a motion repeats it (`5j`). `/` and `?`
//! search down and up (lowercase: any case), `n N` again. `v` selects, `V` selects lines, `C-v`
//! a block (a rectangle), `o` swaps the selection's ends; `y` or Enter copies it and leaves; Esc
//! clears the selection, then leaves; `q` leaves. The mouse: drag to select (copied when you
//! let go), double-click a word, triple-click a line.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

#[derive(Clone, Debug)]
pub struct Copy {
    pub lines: Vec<String>,
    /// The pane's height.
    pub height: usize,
    /// The cursor: a row of `lines`, a char in it.
    pub row: usize,
    pub col: usize,
    /// The column up and down keep to (vi's "curswant").
    want: usize,
    /// The first row on screen.
    pub top: usize,
    /// Where the selection started, and its shape.
    pub anchor: Option<(usize, usize)>,
    pub shape: Shape,
    /// The last search, and its direction (`true`: down).
    search: Option<(String, bool)>,
    /// A search being typed: its text and direction.
    pub typing: Option<(String, bool)>,
    /// A count being typed (`5` of `5j`), and a pending `g`.
    count: Option<usize>,
    g: bool,
    /// `f F t T` waiting for their character; the last one done, for `;` and `,`.
    pending: Option<char>,
    last_find: Option<(char, char)>,
    /// Said in the status: "not found", "copied 3 lines".
    pub note: Option<String>,
    /// The pane's text revision `lines` was read at (`corgi_vt::Vt::revision`).
    pub revision: u64,
}

/// What a selection takes in.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Shape {
    /// From one character to another, through the lines between (`v`).
    #[default]
    Chars,
    /// Whole lines (`V`).
    Lines,
    /// A rectangle: the same columns on each line (`C-v`).
    Block,
}

#[derive(Debug, PartialEq)]
pub enum Outcome {
    Stay,
    Exit,
    /// Copy this, and leave.
    Yank(String),
}

fn is_word(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// What a character is, for word motions: a space (0), a word character (1), punctuation (2).
/// For WORDs (`big`) everything but spaces is one kind.
fn kind(ch: Option<char>, big: bool) -> Option<u8> {
    ch.map(|ch| {
        if ch.is_whitespace() {
            0
        } else if big || is_word(ch) {
            1
        } else {
            2
        }
    })
}

impl Copy {
    /// Copy mode over `lines`, the pane `height` rows tall showing from `top`, the cursor at
    /// (`row`, `col`).
    pub fn new(lines: Vec<String>, height: usize, top: usize, row: usize, col: usize) -> Copy {
        let lines = if lines.is_empty() { vec![String::new()] } else { lines };
        let row = row.min(lines.len() - 1);
        let mut c = Copy {
            lines,
            height: height.max(1),
            row,
            col,
            want: col,
            top,
            anchor: None,
            shape: Shape::Chars,
            search: None,
            typing: None,
            count: None,
            g: false,
            pending: None,
            last_find: None,
            note: None,
            revision: 0,
        };
        c.clamp();
        c
    }

    /// The pane's text changed (new output): keep the cursor where it was, as far as possible.
    pub fn refresh(&mut self, lines: Vec<String>) {
        self.lines = if lines.is_empty() { vec![String::new()] } else { lines };
        self.clamp();
    }

    fn len(&self, row: usize) -> usize {
        self.lines.get(row).map_or(0, |l| l.chars().count())
    }

    fn char_at(&self, row: usize, col: usize) -> Option<char> {
        self.lines.get(row).and_then(|l| l.chars().nth(col))
    }

    fn last_row(&self) -> usize {
        self.lines.len() - 1
    }

    /// Keep the cursor on the text and on screen.
    fn clamp(&mut self) {
        self.row = self.row.min(self.last_row());
        self.col = self.col.min(self.len(self.row).saturating_sub(1));
        let max_top = self.lines.len().saturating_sub(self.height);
        if self.row < self.top {
            self.top = self.row;
        } else if self.row >= self.top + self.height {
            self.top = self.row + 1 - self.height;
        }
        self.top = self.top.min(max_top);
    }

    fn go(&mut self, row: usize, col: usize) {
        self.row = row;
        self.col = col;
        self.want = col;
        self.clamp();
    }

    fn vertical(&mut self, row: usize) {
        self.row = row.min(self.last_row());
        self.col = self.want.min(self.len(self.row).saturating_sub(1));
        self.clamp();
    }

    /// The next word's start after the cursor (across lines), or the end.
    fn word_forward(&self, big: bool) -> (usize, usize) {
        let (mut r, mut c) = (self.row, self.col);
        let kind = |ch: Option<char>| kind(ch, big);
        let start = kind(self.char_at(r, c));
        // Off the current word…
        while c < self.len(r) && kind(self.char_at(r, c)) == start && start != Some(0) {
            c += 1;
        }
        // …over spaces and line ends to the next word.
        loop {
            while c < self.len(r) && kind(self.char_at(r, c)) == Some(0) {
                c += 1;
            }
            if c < self.len(r) || r == self.last_row() {
                return (r, c.min(self.len(r).saturating_sub(1)));
            }
            r += 1;
            c = 0;
            if self.len(r) == 0 {
                return (r, 0);
            }
        }
    }

    /// The start of the word before the cursor (across lines).
    fn word_back(&self, big: bool) -> (usize, usize) {
        let (mut r, mut c) = (self.row, self.col);
        let kind = |ch: Option<char>| kind(ch, big);
        loop {
            if c == 0 {
                if r == 0 {
                    return (0, 0);
                }
                r -= 1;
                c = self.len(r);
                if c == 0 {
                    return (r, 0);
                }
                continue;
            }
            c -= 1;
            if kind(self.char_at(r, c)) != Some(0) {
                break;
            }
        }
        let k = kind(self.char_at(r, c));
        while c > 0 && kind(self.char_at(r, c - 1)) == k {
            c -= 1;
        }
        (r, c)
    }

    /// The end of the word at or after the cursor.
    fn word_end(&self, big: bool) -> (usize, usize) {
        let (mut r, mut c) = (self.row, self.col + 1);
        let kind = |ch: Option<char>| kind(ch, big);
        loop {
            while c < self.len(r) && kind(self.char_at(r, c)) == Some(0) {
                c += 1;
            }
            if c < self.len(r) {
                break;
            }
            if r == self.last_row() {
                return (r, self.len(r).saturating_sub(1));
            }
            r += 1;
            c = 0;
        }
        let k = kind(self.char_at(r, c));
        while c + 1 < self.len(r) && kind(self.char_at(r, c + 1)) == k {
            c += 1;
        }
        (r, c)
    }

    /// Find `needle` from the cursor (not at it), down or up, wrapping round. Lowercase needles
    /// match any case.
    fn find(&mut self, needle: &str, down: bool) {
        if needle.is_empty() {
            return;
        }
        let fold = !needle.chars().any(char::is_uppercase);
        let prep = |s: &str| if fold { s.to_lowercase() } else { s.to_string() };
        let needle = prep(needle);
        let n = self.lines.len();
        // Matches in a line as char columns.
        let cols = |line: &str| -> Vec<usize> {
            let hay = prep(line);
            hay.match_indices(&needle)
                .map(|(b, _)| hay[..b].chars().count())
                .collect()
        };
        for step in 0..=n {
            let r = if down {
                (self.row + step) % n
            } else {
                (self.row + n - step % n) % n
            };
            let found = cols(&self.lines[r]);
            let hit = if step == 0 {
                if down {
                    found.into_iter().find(|&c| c > self.col)
                } else {
                    found.into_iter().rev().find(|&c| c < self.col)
                }
            } else if step == n {
                // Round to the cursor's own line again: the other side of it.
                if down {
                    found.into_iter().find(|&c| c <= self.col)
                } else {
                    found.into_iter().rev().find(|&c| c >= self.col)
                }
            } else if down {
                found.into_iter().next()
            } else {
                found.into_iter().last()
            };
            if let Some(c) = hit {
                let wrapped = if down {
                    r < self.row || (r == self.row && c <= self.col)
                } else {
                    r > self.row
                };
                self.go(r, c);
                self.note = wrapped.then(|| {
                    if down {
                        "from the top".into()
                    } else {
                        "from the bottom".into()
                    }
                });
                return;
            }
        }
        self.note = Some(format!("not found: {needle}"));
    }

    /// The selection's ends, first to last.
    fn ends(&self) -> Option<((usize, usize), (usize, usize))> {
        let a = self.anchor?;
        let b = (self.row, self.col);
        Some(if a <= b { (a, b) } else { (b, a) })
    }

    /// A block's columns: from the leftmost end to the rightmost. The cursor's side is the
    /// column it keeps to (wider than a short line it's on; after `$`, every line's end).
    fn block_cols(&self) -> Option<(usize, usize)> {
        let (_, ac) = self.anchor?;
        let cur = self.want.max(self.col);
        Some((ac.min(cur), ac.max(cur)))
    }

    /// The selection, as text: from the anchor to the cursor (whole lines, or a block's columns).
    pub fn selected(&self) -> Option<String> {
        let ((r0, c0), (r1, c1)) = self.ends()?;
        let mut out: Vec<String> = Vec::new();
        for r in r0..=r1 {
            let line: Vec<char> = self.lines[r].chars().collect();
            let (from, to) = match self.shape {
                Shape::Lines => (0, line.len()),
                Shape::Block => {
                    let (a, b) = self.block_cols()?;
                    (a, b.saturating_add(1))
                }
                Shape::Chars => (if r == r0 { c0 } else { 0 }, if r == r1 { c1 + 1 } else { line.len() }),
            };
            let (from, to) = (from.min(line.len()), to.min(line.len()));
            let piece: String = line[from..to.max(from)].iter().collect();
            out.push(if self.shape == Shape::Block {
                piece.trim_end().to_string()
            } else {
                piece
            });
        }
        Some(out.join("\n"))
    }

    /// Whether the char at (`row`, `col`) is selected.
    pub fn is_selected(&self, row: usize, col: usize) -> bool {
        let Some((lo, hi)) = self.ends() else { return false };
        match self.shape {
            Shape::Lines => (lo.0..=hi.0).contains(&row),
            Shape::Block => {
                let (a, b) = self.block_cols().unwrap_or_default();
                (lo.0..=hi.0).contains(&row) && (a..=b).contains(&col)
            }
            Shape::Chars => (lo..=hi).contains(&(row, col)),
        }
    }

    /// Start or end a selection of this shape (`v`, `V`, `C-v`): the same shape again ends it,
    /// another changes it.
    fn toggle(&mut self, shape: Shape) {
        if self.anchor.is_some() && self.shape == shape {
            self.anchor = None;
        } else {
            self.anchor = Some(self.anchor.unwrap_or((self.row, self.col)));
            self.shape = shape;
        }
    }

    /// `{` and `}`: to the blank line before or after this paragraph (else the first or last).
    fn paragraph(&self, down: bool) -> usize {
        let blank = |r: usize| self.lines[r].trim().is_empty();
        let mut r = self.row;
        let step = |r: usize| {
            if down {
                (r < self.last_row()).then(|| r + 1)
            } else {
                r.checked_sub(1)
            }
        };
        // Off the blank lines it's on first, then through the paragraph to the blank after it.
        while blank(r) {
            match step(r) {
                Some(n) => r = n,
                None => return r,
            }
        }
        while let Some(n) = step(r) {
            r = n;
            if blank(r) {
                break;
            }
        }
        r
    }

    /// `f F t T`: the column of `ch` on this line, after (or before) the cursor; `t` and `T`
    /// stop just short of it (`again`, for `;` and `,`: past one they stopped short of).
    fn find_char(&self, how: char, ch: char, n: usize, again: bool) -> Option<usize> {
        let line: Vec<char> = self.lines[self.row].chars().collect();
        let forward = matches!(how, 'f' | 't');
        let mut col = self.col;
        for i in 0..n {
            // `t` again (`;`) from just before a match must look past it.
            let skip = again && i == 0 && matches!(how, 't' | 'T');
            col = if forward {
                let from = col + 1 + usize::from(skip && line.get(col + 1) == Some(&ch));
                (from..line.len()).find(|&c| line[c] == ch)?
            } else {
                let to = col.saturating_sub(usize::from(skip && col > 0 && line.get(col - 1) == Some(&ch)));
                (0..to).rev().find(|&c| line[c] == ch)?
            };
        }
        Some(match how {
            't' => col - 1,
            'T' => col + 1,
            _ => col,
        })
    }

    /// The mouse: put the cursor at (`row`, `col`), as far as the text goes.
    pub fn point(&mut self, row: usize, col: usize) {
        self.go(row.min(self.last_row()), col);
    }

    /// The mouse pressed here: a new selection starts (once it moves).
    pub fn press(&mut self, row: usize, col: usize) {
        self.point(row, col);
        self.anchor = Some((self.row, self.col));
        self.shape = Shape::Chars;
    }

    /// A double click: the word under the cursor, selected.
    pub fn select_word(&mut self) {
        let line: Vec<char> = self.lines[self.row].chars().collect();
        let Some(&ch) = line.get(self.col) else { return };
        let k = kind(Some(ch), false);
        let (mut a, mut b) = (self.col, self.col);
        while a > 0 && kind(line.get(a - 1).copied(), false) == k {
            a -= 1;
        }
        while b + 1 < line.len() && kind(line.get(b + 1).copied(), false) == k {
            b += 1;
        }
        self.anchor = Some((self.row, a));
        self.shape = Shape::Chars;
        self.go(self.row, b);
    }

    /// A triple click: the line.
    pub fn select_line(&mut self) {
        self.anchor = Some((self.row, 0));
        self.shape = Shape::Lines;
    }

    pub fn key(&mut self, k: &KeyEvent) -> Outcome {
        let ctrl = k.modifiers.contains(KeyModifiers::CONTROL);
        // Typing a search.
        if let Some((mut text, down)) = self.typing.take() {
            match k.code {
                KeyCode::Esc => {}
                KeyCode::Enter => {
                    self.find(&text, down);
                    self.search = Some((text, down));
                }
                KeyCode::Backspace => {
                    if text.pop().is_some() {
                        self.typing = Some((text, down));
                    }
                }
                KeyCode::Char('u') if ctrl => self.typing = Some((String::new(), down)),
                KeyCode::Char(c) if !ctrl => {
                    text.push(c);
                    self.typing = Some((text, down));
                }
                _ => self.typing = Some((text, down)),
            }
            return Outcome::Stay;
        }
        self.note = None;
        // `f F t T`: this is the character to find.
        if let Some(how) = self.pending.take() {
            if let KeyCode::Char(ch) = k.code {
                let n = self.count.take().unwrap_or(1).max(1);
                self.last_find = Some((how, ch));
                match self.find_char(how, ch, n, false) {
                    Some(c) => self.go(self.row, c),
                    None => self.note = Some(format!("no {ch} on this line")),
                }
            }
            self.count = None;
            return Outcome::Stay;
        }
        // A count before a motion.
        if let KeyCode::Char(d @ '0'..='9') = k.code
            && !ctrl
            && (d != '0' || self.count.is_some())
        {
            self.count = Some(
                self.count
                    .unwrap_or(0)
                    .saturating_mul(10)
                    .saturating_add(d as usize - '0' as usize),
            );
            return Outcome::Stay;
        }
        let n = self.count.take().unwrap_or(1).max(1);
        let g = std::mem::take(&mut self.g);
        let half = (self.height / 2).max(1);
        match (k.code, ctrl) {
            (KeyCode::Esc, _) if self.anchor.is_some() => self.anchor = None,
            (KeyCode::Esc | KeyCode::Char('q'), false) => return Outcome::Exit,
            (KeyCode::Char('c'), true) => return Outcome::Exit,
            (KeyCode::Char('y'), false) | (KeyCode::Enter, _) => {
                return match self.selected() {
                    Some(t) => Outcome::Yank(t),
                    // Nothing selected: Enter leaves, as in tmux; y copies the line.
                    None if k.code == KeyCode::Enter => Outcome::Exit,
                    None => Outcome::Yank(self.lines[self.row].clone()),
                };
            }
            (KeyCode::Char('v'), false) => self.toggle(Shape::Chars),
            (KeyCode::Char('V'), false) => self.toggle(Shape::Lines),
            (KeyCode::Char('v'), true) => self.toggle(Shape::Block),
            (KeyCode::Char(how @ ('f' | 'F' | 't' | 'T')), false) => {
                self.pending = Some(how);
                self.count = Some(n);
            }
            (KeyCode::Char(again @ (';' | ',')), false) => match self.last_find {
                Some((how, ch)) => {
                    // `,` the other way.
                    let how = if again == ',' {
                        match how {
                            'f' => 'F',
                            'F' => 'f',
                            't' => 'T',
                            _ => 't',
                        }
                    } else {
                        how
                    };
                    if let Some(c) = self.find_char(how, ch, n, true) {
                        self.go(self.row, c);
                    }
                }
                None => self.note = Some("no f, F, t or T yet".into()),
            },
            (KeyCode::Char('}'), false) => {
                for _ in 0..n {
                    let r = self.paragraph(true);
                    self.go(r, 0);
                }
            }
            (KeyCode::Char('{'), false) => {
                for _ in 0..n {
                    let r = self.paragraph(false);
                    self.go(r, 0);
                }
            }
            (KeyCode::Char('o'), false) => {
                if let Some((r, c)) = self.anchor {
                    self.anchor = Some((self.row, self.col));
                    self.go(r, c);
                }
            }
            (KeyCode::Char('h') | KeyCode::Left, false) => {
                let c = self.col.saturating_sub(n);
                self.go(self.row, c);
            }
            (KeyCode::Char('l') | KeyCode::Right, false) => {
                let c = (self.col + n).min(self.len(self.row).saturating_sub(1));
                self.go(self.row, c);
            }
            (KeyCode::Char('j') | KeyCode::Down, false) => self.vertical(self.row + n),
            (KeyCode::Char('k') | KeyCode::Up, false) => self.vertical(self.row.saturating_sub(n)),
            (KeyCode::Char('0') | KeyCode::Home, false) => self.go(self.row, 0),
            (KeyCode::Char('^'), false) => {
                let c = self.lines[self.row]
                    .chars()
                    .position(|c| !c.is_whitespace())
                    .unwrap_or(0);
                self.go(self.row, c);
            }
            (KeyCode::Char('$') | KeyCode::End, false) => {
                let c = self.len(self.row).saturating_sub(1);
                self.go(self.row, c);
                self.want = usize::MAX;
            }
            (KeyCode::Char(k @ ('w' | 'W')), false) => {
                for _ in 0..n {
                    let (r, c) = self.word_forward(k == 'W');
                    self.go(r, c);
                }
            }
            (KeyCode::Char(k @ ('b' | 'B')), false) => {
                for _ in 0..n {
                    let (r, c) = self.word_back(k == 'B');
                    self.go(r, c);
                }
            }
            (KeyCode::Char(k @ ('e' | 'E')), false) => {
                for _ in 0..n {
                    let (r, c) = self.word_end(k == 'E');
                    self.go(r, c);
                }
            }
            (KeyCode::Char('g'), false) if g => self.go(0, 0),
            (KeyCode::Char('g'), false) => self.g = true,
            (KeyCode::Char('G'), false) => {
                let r = self.last_row();
                self.go(r, 0);
            }
            (KeyCode::Char('H'), false) => self.vertical(self.top),
            (KeyCode::Char('M'), false) => self.vertical(self.top + self.height.min(self.lines.len() - self.top) / 2),
            (KeyCode::Char('L'), false) => self.vertical((self.top + self.height).saturating_sub(1)),
            (KeyCode::Char('d'), true) => self.page(half as isize * n as isize),
            (KeyCode::Char('u'), true) => self.page(-(half as isize) * n as isize),
            (KeyCode::Char('f'), true) | (KeyCode::PageDown, _) => self.page(self.height as isize * n as isize),
            (KeyCode::Char('b'), true) | (KeyCode::PageUp, _) => self.page(-(self.height as isize) * n as isize),
            (KeyCode::Char('/'), false) => self.typing = Some((String::new(), true)),
            (KeyCode::Char('?'), false) => self.typing = Some((String::new(), false)),
            (KeyCode::Char('n'), false) => self.again(true),
            (KeyCode::Char('N'), false) => self.again(false),
            _ => {}
        }
        Outcome::Stay
    }

    /// Scroll the view and the cursor by `by` rows.
    pub fn page(&mut self, by: isize) {
        let max_top = self.lines.len().saturating_sub(self.height) as isize;
        self.top = (self.top as isize + by).clamp(0, max_top) as usize;
        let row = (self.row as isize + by).clamp(0, self.last_row() as isize) as usize;
        self.vertical(row);
    }

    /// `n` (`same`) and `N`: the last search again, the same way or the other.
    fn again(&mut self, same: bool) {
        match self.search.clone() {
            Some((s, down)) => self.find(&s, down == same),
            None => self.note = Some("no search yet".into()),
        }
    }

    /// `[12/340]`: where the cursor is.
    pub fn position(&self) -> String {
        format!("[{}/{}]", self.row + 1, self.lines.len())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(c: char) -> KeyEvent {
        KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE)
    }
    fn ctrl(c: char) -> KeyEvent {
        KeyEvent::new(KeyCode::Char(c), KeyModifiers::CONTROL)
    }
    fn keys(c: &mut Copy, s: &str) {
        for ch in s.chars() {
            match ch {
                '\r' => c.key(&KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)),
                ch => c.key(&key(ch)),
            };
        }
    }
    fn copy() -> Copy {
        let lines: Vec<String> = (0..100).map(|i| format!("line {i} has words")).collect();
        Copy::new(lines, 10, 90, 99, 0)
    }

    #[test]
    fn moving() {
        let mut c = copy();
        keys(&mut c, "k");
        assert_eq!((c.row, c.top), (98, 90));
        keys(&mut c, "gg");
        assert_eq!((c.row, c.col, c.top), (0, 0, 0), "to the top, and the view with it");
        keys(&mut c, "5j");
        assert_eq!(c.row, 5);
        keys(&mut c, "$");
        assert_eq!(c.col, "line 5 has words".len() - 1);
        keys(&mut c, "j");
        assert_eq!(c.col, "line 6 has words".len() - 1, "$ sticks to the end");
        keys(&mut c, "0w");
        assert_eq!(c.col, 5, "the next word");
        keys(&mut c, "w");
        assert_eq!(c.col, 7);
        keys(&mut c, "e");
        assert_eq!(c.col, 9, "its end");
        keys(&mut c, "b");
        assert_eq!(c.col, 7);
        keys(&mut c, "G");
        assert_eq!((c.row, c.top), (99, 90));
        c.key(&ctrl('u'));
        assert_eq!((c.row, c.top), (94, 85), "half a screen");
        keys(&mut c, "H");
        assert_eq!(c.row, 85);
        keys(&mut c, "L");
        assert_eq!(c.row, 94);
    }

    #[test]
    fn words_cross_lines() {
        let mut c = Copy::new(vec!["one two".into(), "".into(), "  three".into()], 5, 0, 0, 4);
        keys(&mut c, "w");
        assert_eq!((c.row, c.col), (1, 0), "an empty line is a stop");
        keys(&mut c, "w");
        assert_eq!((c.row, c.col), (2, 2));
        keys(&mut c, "b");
        assert_eq!((c.row, c.col), (1, 0));
        keys(&mut c, "b");
        assert_eq!((c.row, c.col), (0, 4));
    }

    #[test]
    fn searching() {
        let mut c = copy();
        keys(&mut c, "?line 42\r");
        assert_eq!((c.row, c.col), (42, 0));
        keys(&mut c, "n");
        assert_eq!(c.row, 42, "no other match for line 42 but itself: round to it");
        keys(&mut c, "/WORDS\r");
        assert_eq!(
            c.note.as_deref(),
            Some("not found: WORDS"),
            "an uppercase needle matches case"
        );
        keys(&mut c, "/words\r");
        assert_eq!((c.row, c.col), (42, 12), "\"words\" in \"line 42 has words\"");
        keys(&mut c, "n");
        assert_eq!(c.row, 43);
        keys(&mut c, "N");
        assert_eq!(c.row, 42);
        keys(&mut c, "gg?line 9\r");
        assert_eq!(c.row, 99, "up from the top: round to the bottom");
        assert_eq!(c.note.as_deref(), Some("from the bottom"));
    }

    #[test]
    fn selecting_and_copying() {
        let mut c = Copy::new(vec!["hello world".into(), "second line".into()], 5, 0, 0, 6);
        keys(&mut c, "ve");
        assert!(c.is_selected(0, 8) && !c.is_selected(0, 5));
        assert_eq!(c.key(&key('y')), Outcome::Yank("world".into()));
        // Across lines, and whole lines.
        let mut c = Copy::new(vec!["hello world".into(), "second line".into()], 5, 0, 0, 6);
        keys(&mut c, "vj");
        assert_eq!(
            c.selected().unwrap(),
            "world\nsecond ",
            "up to and with the cursor's char"
        );
        keys(&mut c, "V");
        assert_eq!(c.selected().unwrap(), "hello world\nsecond line");
        keys(&mut c, "o");
        assert_eq!(c.row, 0, "the other end");
        // Esc: the selection first, then leave. Enter with nothing selected leaves.
        assert_eq!(c.key(&KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE)), Outcome::Stay);
        assert!(c.anchor.is_none());
        assert_eq!(c.key(&KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)), Outcome::Exit);
        // y with nothing selected: the line.
        assert_eq!(c.key(&key('y')), Outcome::Yank("hello world".into()));
    }

    #[test]
    fn short_and_changing_text() {
        let mut c = Copy::new(vec![], 5, 0, 3, 3);
        assert_eq!((c.row, c.col), (0, 0), "nothing to stand on: the start");
        c.refresh(vec!["a".into(), "b".into()]);
        keys(&mut c, "G");
        assert_eq!(c.row, 1);
        assert_eq!(c.position(), "[2/2]");
    }

    #[test]
    fn words_paragraphs_and_characters() {
        let mut c = Copy::new(vec!["a.b-c d,e  f".into()], 5, 0, 0, 0);
        keys(&mut c, "w");
        assert_eq!(c.col, 1, "punctuation is a word of its own");
        keys(&mut c, "0W");
        assert_eq!(c.col, 6, "a WORD: everything up to a space");
        keys(&mut c, "E");
        assert_eq!(c.col, 8);
        keys(&mut c, "B");
        assert_eq!(c.col, 6);
        keys(&mut c, "0fc");
        assert_eq!(c.col, 4);
        keys(&mut c, "0tc");
        assert_eq!(c.col, 3, "t stops short");
        keys(&mut c, "0t.");
        assert_eq!(c.col, 0, "short of a match next door: stays");
        keys(&mut c, "0f ;");
        assert_eq!(c.col, 9, "; again");
        keys(&mut c, ",");
        assert_eq!(c.col, 5, ", the other way");
        keys(&mut c, "Fa");
        assert_eq!(c.col, 0);
        keys(&mut c, "fz");
        assert_eq!(c.note.as_deref(), Some("no z on this line"));
        let text = ["one", "two", "", "", "three", "four", "", "five"];
        let mut c = Copy::new(text.iter().map(|s| s.to_string()).collect(), 10, 0, 0, 0);
        keys(&mut c, "}");
        assert_eq!(c.row, 2, "the blank line after");
        keys(&mut c, "}");
        assert_eq!(c.row, 6, "past blank lines, through the next paragraph");
        keys(&mut c, "}");
        assert_eq!(c.row, 7, "none after: the last line");
        keys(&mut c, "2{");
        assert_eq!(c.row, 3);
    }

    #[test]
    fn blocks_and_the_mouse() {
        let lines = vec!["abcdef".to_string(), "ghijkl".into(), "mn".into()];
        let mut c = Copy::new(lines.clone(), 5, 0, 0, 1);
        c.key(&ctrl('v'));
        keys(&mut c, "lljj");
        assert_eq!(c.shape, Shape::Block);
        assert_eq!(c.selected().unwrap(), "bcd\nhij\nn", "the same columns on each line");
        assert!(c.is_selected(1, 3) && !c.is_selected(1, 0) && !c.is_selected(1, 4));
        keys(&mut c, "$");
        assert_eq!(c.selected().unwrap(), "bcdef\nhijkl\nn", "$: to each line's end");
        keys(&mut c, "v");
        assert_eq!(c.shape, Shape::Chars, "another shape: changed, not ended");
        c.key(&ctrl('v'));
        c.key(&ctrl('v'));
        assert!(c.anchor.is_none(), "the same shape again: ended");
        let mut c = Copy::new(lines, 5, 0, 0, 0);
        c.press(0, 2);
        c.point(1, 1);
        assert_eq!(c.selected().unwrap(), "cdef\ngh");
        c.point(9, 0);
        assert_eq!(c.row, 2, "below the text: its last line");
        let mut c = Copy::new(vec!["say hello_there, ok".into()], 5, 0, 0, 7);
        c.select_word();
        assert_eq!(c.selected().unwrap(), "hello_there");
        c.select_line();
        assert_eq!(c.selected().unwrap(), "say hello_there, ok");
    }
}
