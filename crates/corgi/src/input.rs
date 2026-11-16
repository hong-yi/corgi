//! Host key events → keys for the pane's emulator to encode.

use corgi_vt::{Key, Mods, Named};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

pub fn mods(m: KeyModifiers) -> Mods {
    let mut out = Mods::default();
    for (flag, m2) in [
        (KeyModifiers::SHIFT, Mods::SHIFT),
        (KeyModifiers::CONTROL, Mods::CTRL),
        (KeyModifiers::ALT, Mods::ALT),
        (KeyModifiers::SUPER, Mods::SUPER),
    ] {
        if m.contains(flag) {
            out = out | m2;
        }
    }
    out
}

/// `None` for keys a terminal program can't receive (media keys, lone modifiers).
pub fn to_vt(k: &KeyEvent) -> Option<Key> {
    let m = mods(k.modifiers);
    let named = |n| Some(Key::Named(n, m));
    match k.code {
        KeyCode::Char(' ') => named(Named::Space),
        KeyCode::Char(c) => Some(Key::Char(c, m)),
        KeyCode::Enter => named(Named::Enter),
        KeyCode::Tab => named(Named::Tab),
        KeyCode::BackTab => Some(Key::Named(Named::Tab, m | Mods::SHIFT)),
        KeyCode::Backspace => named(Named::Backspace),
        KeyCode::Esc => named(Named::Escape),
        KeyCode::Up => named(Named::Up),
        KeyCode::Down => named(Named::Down),
        KeyCode::Left => named(Named::Left),
        KeyCode::Right => named(Named::Right),
        KeyCode::Home => named(Named::Home),
        KeyCode::End => named(Named::End),
        KeyCode::PageUp => named(Named::PageUp),
        KeyCode::PageDown => named(Named::PageDown),
        KeyCode::Insert => named(Named::Insert),
        KeyCode::Delete => named(Named::Delete),
        KeyCode::F(n) => named(Named::F(n)),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_keys_and_modifiers() {
        let k = |code, m| to_vt(&KeyEvent::new(code, m));
        assert_eq!(
            k(KeyCode::Char('a'), KeyModifiers::CONTROL),
            Some(Key::Char('a', Mods::CTRL))
        );
        assert_eq!(
            k(KeyCode::BackTab, KeyModifiers::SHIFT),
            Some(Key::Named(Named::Tab, Mods::SHIFT))
        );
        assert_eq!(
            k(KeyCode::Char(' '), KeyModifiers::NONE),
            Some(Key::Named(Named::Space, Mods::default()))
        );
        assert_eq!(k(KeyCode::CapsLock, KeyModifiers::NONE), None);
    }
}
