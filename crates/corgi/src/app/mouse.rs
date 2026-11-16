//! The mouse for the program in a pane: once it asks for mouse reports (vim's `mouse=a`,
//! lazygit, htop), its clicks, drags, motion and wheel are sent to it, in the format it asked
//! for; a button pressed in a pane keeps its drag and release there. Shift held: corgi's own
//! (select and copy, the menu), as terminals do. A full-screen program that didn't ask (less,
//! man) gets the wheel as arrow keys.

use super::*;
use corgi_vt::{Key, Mods, MouseAction, MouseButton as Btn, Named};
use crossterm::event::MouseEvent;

impl App {
    /// True if the program in a pane took it.
    pub(super) fn mouse_to_program(&mut self, m: MouseEvent) -> bool {
        if m.modifiers.contains(KeyModifiers::SHIFT) {
            return false;
        }
        let at = ratatui::layout::Position::new(m.column, m.row);
        let rects = self.rects();
        // A held button stays with the pane it was pressed in, wherever it goes.
        let held = matches!(m.kind, MouseEventKind::Drag(_) | MouseEventKind::Up(_));
        let target = match (&self.grab, held) {
            (Some(g), true) => rects.iter().find(|(id, _)| id == g).cloned(),
            _ => rects.iter().find(|(_, r)| r.contains(at)).cloned(),
        };
        let Some((id, r)) = target else {
            if !held {
                self.grab = None;
            }
            return false;
        };
        if self.myself.as_deref() == Some(id.as_str()) {
            return false;
        }
        let Some(p) = self.panes.get_mut(&id).filter(|p| p.state == State::Live) else {
            return false;
        };
        if !p.vt.mouse_tracking() {
            // A full-screen program without the mouse: the wheel as arrow keys.
            let arrow = match m.kind {
                MouseEventKind::ScrollUp => Named::Up,
                MouseEventKind::ScrollDown => Named::Down,
                _ => return false,
            };
            if !p.vt.alternate_screen() {
                return false;
            }
            for _ in 0..3 {
                let bytes = p.vt.encode_key(&Key::Named(arrow, Mods::default()));
                p.input(&bytes);
            }
            return true;
        }
        let button = |b: MouseButton| match b {
            MouseButton::Left => Btn::Left,
            MouseButton::Right => Btn::Right,
            MouseButton::Middle => Btn::Middle,
        };
        let (action, btn) = match m.kind {
            MouseEventKind::Down(b) => (MouseAction::Press, Some(button(b))),
            MouseEventKind::Up(b) => (MouseAction::Release, Some(button(b))),
            MouseEventKind::Drag(b) => (MouseAction::Motion, Some(button(b))),
            MouseEventKind::Moved => (MouseAction::Motion, None),
            MouseEventKind::ScrollUp => (MouseAction::Press, Some(Btn::WheelUp)),
            MouseEventKind::ScrollDown => (MouseAction::Press, Some(Btn::WheelDown)),
            MouseEventKind::ScrollLeft => (MouseAction::Press, Some(Btn::WheelLeft)),
            MouseEventKind::ScrollRight => (MouseAction::Press, Some(Btn::WheelRight)),
        };
        let mut mods = Mods::default();
        if m.modifiers.contains(KeyModifiers::CONTROL) {
            mods = Mods(mods.0 | Mods::CTRL.0);
        }
        if m.modifiers.contains(KeyModifiers::ALT) {
            mods = Mods(mods.0 | Mods::ALT.0);
        }
        // Where in the pane (dragged outside it: its nearest edge).
        let col = m.column.clamp(r.x, r.right().saturating_sub(1)) - r.x;
        let row = m.row.clamp(r.y, r.bottom().saturating_sub(1)) - r.y;
        let pressed = matches!(m.kind, MouseEventKind::Drag(_));
        let bytes = p.vt.encode_mouse(action, btn, (col, row), mods, pressed);
        if !bytes.is_empty() {
            p.input(&bytes);
        }
        match m.kind {
            MouseEventKind::Down(_) => {
                self.grab = Some(id.clone());
                // A click there gives it the keys too.
                self.focus_pane(&id);
            }
            MouseEventKind::Up(_) => self.grab = None,
            _ => {}
        }
        true
    }
}
