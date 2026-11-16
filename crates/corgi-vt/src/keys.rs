//! Key presses → bytes for the program, through libghostty's encoder, which
//! follows the program's modes (DECCKM, keypad, kitty keyboard flags, ...).

use crate::{Vt, sys};

/// Modifier bits, as libghostty defines them.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Mods(pub u16);

impl Mods {
    pub const SHIFT: Mods = Mods(sys::GHOSTTY_MODS_SHIFT as u16);
    pub const CTRL: Mods = Mods(sys::GHOSTTY_MODS_CTRL as u16);
    pub const ALT: Mods = Mods(sys::GHOSTTY_MODS_ALT as u16);
    pub const SUPER: Mods = Mods(sys::GHOSTTY_MODS_SUPER as u16);

    pub fn contains(self, other: Mods) -> bool {
        self.0 & other.0 == other.0
    }
}

impl std::ops::BitOr for Mods {
    type Output = Mods;
    fn bitor(self, o: Mods) -> Mods {
        Mods(self.0 | o.0)
    }
}

/// Keys that aren't text.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Named {
    Enter,
    Tab,
    Backspace,
    Escape,
    Space,
    Up,
    Down,
    Left,
    Right,
    Home,
    End,
    PageUp,
    PageDown,
    Insert,
    Delete,
    F(u8),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Key {
    /// A character key: the text it types (before Ctrl/Alt), e.g. `'a'` or `'A'` with Shift.
    Char(char, Mods),
    Named(Named, Mods),
}

/// US-layout physical key for a character, and its unshifted character.
fn physical(c: char) -> (sys::GhosttyKey, Option<char>) {
    let lower = c.to_ascii_lowercase();
    if lower.is_ascii_lowercase() {
        return (
            sys::GHOSTTY_KEY_A + (lower as u32 - 'a' as u32) as sys::GhosttyKey,
            Some(lower),
        );
    }
    if c.is_ascii_digit() {
        return (
            sys::GHOSTTY_KEY_DIGIT_0 + (c as u32 - '0' as u32) as sys::GhosttyKey,
            Some(c),
        );
    }
    let (key, base) = match c {
        '`' | '~' => (sys::GHOSTTY_KEY_BACKQUOTE, '`'),
        '-' | '_' => (sys::GHOSTTY_KEY_MINUS, '-'),
        '=' | '+' => (sys::GHOSTTY_KEY_EQUAL, '='),
        '[' | '{' => (sys::GHOSTTY_KEY_BRACKET_LEFT, '['),
        ']' | '}' => (sys::GHOSTTY_KEY_BRACKET_RIGHT, ']'),
        '\\' | '|' => (sys::GHOSTTY_KEY_BACKSLASH, '\\'),
        ';' | ':' => (sys::GHOSTTY_KEY_SEMICOLON, ';'),
        '\'' | '"' => (sys::GHOSTTY_KEY_QUOTE, '\''),
        ',' | '<' => (sys::GHOSTTY_KEY_COMMA, ','),
        '.' | '>' => (sys::GHOSTTY_KEY_PERIOD, '.'),
        '/' | '?' => (sys::GHOSTTY_KEY_SLASH, '/'),
        ' ' => (sys::GHOSTTY_KEY_SPACE, ' '),
        '!' => (sys::GHOSTTY_KEY_DIGIT_1, '1'),
        '@' => (sys::GHOSTTY_KEY_DIGIT_2, '2'),
        '#' => (sys::GHOSTTY_KEY_DIGIT_3, '3'),
        '$' => (sys::GHOSTTY_KEY_DIGIT_4, '4'),
        '%' => (sys::GHOSTTY_KEY_DIGIT_5, '5'),
        '^' => (sys::GHOSTTY_KEY_DIGIT_6, '6'),
        '&' => (sys::GHOSTTY_KEY_DIGIT_7, '7'),
        '*' => (sys::GHOSTTY_KEY_DIGIT_8, '8'),
        '(' => (sys::GHOSTTY_KEY_DIGIT_9, '9'),
        ')' => (sys::GHOSTTY_KEY_DIGIT_0, '0'),
        _ => return (sys::GHOSTTY_KEY_UNIDENTIFIED, None),
    };
    (key, Some(base))
}

fn named(n: Named) -> sys::GhosttyKey {
    match n {
        Named::Enter => sys::GHOSTTY_KEY_ENTER,
        Named::Tab => sys::GHOSTTY_KEY_TAB,
        Named::Backspace => sys::GHOSTTY_KEY_BACKSPACE,
        Named::Escape => sys::GHOSTTY_KEY_ESCAPE,
        Named::Space => sys::GHOSTTY_KEY_SPACE,
        Named::Up => sys::GHOSTTY_KEY_ARROW_UP,
        Named::Down => sys::GHOSTTY_KEY_ARROW_DOWN,
        Named::Left => sys::GHOSTTY_KEY_ARROW_LEFT,
        Named::Right => sys::GHOSTTY_KEY_ARROW_RIGHT,
        Named::Home => sys::GHOSTTY_KEY_HOME,
        Named::End => sys::GHOSTTY_KEY_END,
        Named::PageUp => sys::GHOSTTY_KEY_PAGE_UP,
        Named::PageDown => sys::GHOSTTY_KEY_PAGE_DOWN,
        Named::Insert => sys::GHOSTTY_KEY_INSERT,
        Named::Delete => sys::GHOSTTY_KEY_DELETE,
        Named::F(n) => {
            const F: [sys::GhosttyKey; 12] = [
                sys::GHOSTTY_KEY_F1,
                sys::GHOSTTY_KEY_F2,
                sys::GHOSTTY_KEY_F3,
                sys::GHOSTTY_KEY_F4,
                sys::GHOSTTY_KEY_F5,
                sys::GHOSTTY_KEY_F6,
                sys::GHOSTTY_KEY_F7,
                sys::GHOSTTY_KEY_F8,
                sys::GHOSTTY_KEY_F9,
                sys::GHOSTTY_KEY_F10,
                sys::GHOSTTY_KEY_F11,
                sys::GHOSTTY_KEY_F12,
            ];
            F.get(n.saturating_sub(1) as usize)
                .copied()
                .unwrap_or(sys::GHOSTTY_KEY_UNIDENTIFIED)
        }
    }
}

pub(crate) fn encode(vt: &mut Vt, key: &Key) -> Vec<u8> {
    let (code, mods, text, unshifted) = match key {
        Key::Char(c, m) => {
            let (code, base) = physical(*c);
            // Text must be the printable character, never a control byte.
            let text = (!c.is_control()).then(|| c.to_string());
            (code, *m, text, base.or(Some(*c)))
        }
        Key::Named(n, m) => (named(*n), *m, matches!(n, Named::Space).then(|| " ".to_string()), None),
    };
    let ev = vt.key_event;
    let mut out = vec![0u8; 64];
    let mut len = 0usize;
    // SAFETY: encoder/event handles are owned by `vt`; `text` outlives the encode call.
    unsafe {
        sys::ghostty_key_encoder_setopt_from_terminal(vt.keys, vt.term);
        let yes = true;
        // Alt+x sends ESC x, as xterm's metaSendsEscape.
        sys::ghostty_key_encoder_setopt(
            vt.keys,
            sys::GHOSTTY_KEY_ENCODER_OPT_ALT_ESC_PREFIX,
            (&yes as *const bool).cast(),
        );
        // The host terminal already turned Option into Alt or text; an Alt we see is meant as Alt.
        let as_alt: sys::GhosttyOptionAsAlt = sys::GHOSTTY_OPTION_AS_ALT_TRUE;
        sys::ghostty_key_encoder_setopt(
            vt.keys,
            sys::GHOSTTY_KEY_ENCODER_OPT_MACOS_OPTION_AS_ALT,
            (&as_alt as *const sys::GhosttyOptionAsAlt).cast(),
        );
        sys::ghostty_key_event_set_action(ev, sys::GHOSTTY_KEY_ACTION_PRESS);
        sys::ghostty_key_event_set_key(ev, code);
        sys::ghostty_key_event_set_mods(ev, mods.0);
        sys::ghostty_key_event_set_consumed_mods(ev, 0);
        match &text {
            Some(t) => sys::ghostty_key_event_set_utf8(ev, t.as_ptr().cast(), t.len()),
            None => sys::ghostty_key_event_set_utf8(ev, std::ptr::null(), 0),
        }
        sys::ghostty_key_event_set_unshifted_codepoint(ev, unshifted.map_or(0, |c| c as u32));
        let mut rc = sys::ghostty_key_encoder_encode(vt.keys, ev, out.as_mut_ptr().cast(), out.len(), &mut len);
        if rc == sys::GHOSTTY_OUT_OF_SPACE {
            out.resize(len, 0);
            rc = sys::ghostty_key_encoder_encode(vt.keys, ev, out.as_mut_ptr().cast(), out.len(), &mut len);
        }
        if rc != sys::GHOSTTY_SUCCESS {
            return Vec::new();
        }
        // The event keeps a pointer to `text` only until the next set_utf8.
        sys::ghostty_key_event_set_utf8(ev, std::ptr::null(), 0);
    }
    out.truncate(len);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn enc(vt: &mut Vt, k: Key) -> String {
        String::from_utf8_lossy(&vt.encode_key(&k)).into_owned()
    }

    #[test]
    fn text_control_alt_and_named_keys() {
        let mut vt = Vt::new(10, 2).unwrap();
        assert_eq!(enc(&mut vt, Key::Char('a', Mods::default())), "a");
        assert_eq!(enc(&mut vt, Key::Char('A', Mods::SHIFT)), "A");
        assert_eq!(enc(&mut vt, Key::Char('中', Mods::default())), "中");
        assert_eq!(enc(&mut vt, Key::Char('c', Mods::CTRL)), "\x03");
        assert_eq!(enc(&mut vt, Key::Char('x', Mods::ALT)), "\x1bx");
        assert_eq!(enc(&mut vt, Key::Named(Named::Enter, Mods::default())), "\r");
        assert_eq!(enc(&mut vt, Key::Named(Named::Backspace, Mods::default())), "\x7f");
        assert_eq!(enc(&mut vt, Key::Named(Named::Up, Mods::default())), "\x1b[A");
        assert_eq!(enc(&mut vt, Key::Named(Named::Right, Mods::CTRL)), "\x1b[1;5C");
        assert_eq!(enc(&mut vt, Key::Named(Named::F(5), Mods::default())), "\x1b[15~");
    }

    #[test]
    fn follows_the_programs_modes() {
        let mut vt = Vt::new(10, 2).unwrap();
        vt.write(b"\x1b[?1h"); // DECCKM: application cursor keys (vim, less)
        assert_eq!(enc(&mut vt, Key::Named(Named::Up, Mods::default())), "\x1bOA");
        vt.write(b"\x1b[>1u"); // kitty keyboard: disambiguate
        assert_eq!(enc(&mut vt, Key::Char('c', Mods::CTRL)), "\x1b[99;5u");
        assert_eq!(enc(&mut vt, Key::Named(Named::Escape, Mods::default())), "\x1b[27u");
    }
}
