//! Mouse reports for the program in a terminal: whether it asked for the mouse (DEC modes 9,
//! 1000, 1002, 1003), and encoding a press, release, motion or wheel in the format it asked
//! for (X10, UTF-8, SGR, urxvt, SGR pixels), by libghostty's own encoder.

use crate::keys::Mods;
use crate::{Vt, sys};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MouseAction {
    Press,
    Release,
    Motion,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MouseButton {
    Left,
    Middle,
    Right,
    WheelUp,
    WheelDown,
    WheelLeft,
    WheelRight,
}

impl Vt {
    /// The program asked for mouse reports.
    pub fn mouse_tracking(&self) -> bool {
        [9, 1000, 1002, 1003].iter().any(|&m| self.mode(m, false))
    }

    /// On the alternate screen (a full-screen program): without mouse reports, the wheel is
    /// sent as arrow keys there, as terminals do (`less`, `man`).
    pub fn alternate_screen(&self) -> bool {
        [1049, 1047, 47].iter().any(|&m| self.mode(m, false))
    }

    /// A mouse event at (`col`, `row`) of this terminal, as the program asked to hear it;
    /// empty if it doesn't want this one (motion without a button in mode 1000, say).
    /// `pressed`: a button is held (for motion reports in mode 1002).
    pub fn encode_mouse(
        &mut self,
        action: MouseAction,
        button: Option<MouseButton>,
        (col, row): (u16, u16),
        mods: Mods,
        pressed: bool,
    ) -> Vec<u8> {
        let mut out = vec![0u8; 64];
        let mut len = 0usize;
        let (cw, ch) = match (self.sink.size.cell_width, self.sink.size.cell_height) {
            (0, _) | (_, 0) => (1, 1),
            (w, h) => (w, h),
        };
        let size = sys::GhosttyMouseEncoderSize {
            size: std::mem::size_of::<sys::GhosttyMouseEncoderSize>(),
            screen_width: self.sink.size.columns as u32 * cw,
            screen_height: self.sink.size.rows as u32 * ch,
            cell_width: cw,
            cell_height: ch,
            padding_top: 0,
            padding_bottom: 0,
            padding_right: 0,
            padding_left: 0,
        };
        let pos = sys::GhosttyMousePosition {
            x: col as f32 * cw as f32 + cw as f32 / 2.0,
            y: row as f32 * ch as f32 + ch as f32 / 2.0,
        };
        let action = match action {
            MouseAction::Press => sys::GHOSTTY_MOUSE_ACTION_PRESS,
            MouseAction::Release => sys::GHOSTTY_MOUSE_ACTION_RELEASE,
            MouseAction::Motion => sys::GHOSTTY_MOUSE_ACTION_MOTION,
        };
        // SAFETY: the encoder and event are owned by `self`; options get the types they document.
        unsafe {
            let (enc, ev) = (self.mouse, self.mouse_event);
            sys::ghostty_mouse_encoder_setopt_from_terminal(enc, self.term);
            sys::ghostty_mouse_encoder_setopt(
                enc,
                sys::GHOSTTY_MOUSE_ENCODER_OPT_SIZE,
                (&size as *const sys::GhosttyMouseEncoderSize).cast(),
            );
            sys::ghostty_mouse_encoder_setopt(
                enc,
                sys::GHOSTTY_MOUSE_ENCODER_OPT_ANY_BUTTON_PRESSED,
                (&pressed as *const bool).cast(),
            );
            sys::ghostty_mouse_event_set_action(ev, action);
            match button {
                Some(b) => sys::ghostty_mouse_event_set_button(
                    ev,
                    match b {
                        MouseButton::Left => sys::GHOSTTY_MOUSE_BUTTON_LEFT,
                        MouseButton::Middle => sys::GHOSTTY_MOUSE_BUTTON_MIDDLE,
                        MouseButton::Right => sys::GHOSTTY_MOUSE_BUTTON_RIGHT,
                        MouseButton::WheelUp => sys::GHOSTTY_MOUSE_BUTTON_FOUR,
                        MouseButton::WheelDown => sys::GHOSTTY_MOUSE_BUTTON_FIVE,
                        MouseButton::WheelLeft => sys::GHOSTTY_MOUSE_BUTTON_SIX,
                        MouseButton::WheelRight => sys::GHOSTTY_MOUSE_BUTTON_SEVEN,
                    },
                ),
                None => sys::ghostty_mouse_event_clear_button(ev),
            }
            sys::ghostty_mouse_event_set_mods(ev, mods.0);
            sys::ghostty_mouse_event_set_position(ev, pos);
            let mut rc = sys::ghostty_mouse_encoder_encode(enc, ev, out.as_mut_ptr().cast(), out.len(), &mut len);
            if rc == sys::GHOSTTY_OUT_OF_SPACE {
                out.resize(len, 0);
                rc = sys::ghostty_mouse_encoder_encode(enc, ev, out.as_mut_ptr().cast(), out.len(), &mut len);
            }
            if rc != sys::GHOSTTY_SUCCESS {
                return Vec::new();
            }
        }
        out.truncate(len);
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reports_in_the_format_asked_for() {
        let mut vt = Vt::new(80, 24).unwrap();
        assert!(!vt.mouse_tracking());
        // Not asked: nothing.
        let none = vt.encode_mouse(
            MouseAction::Press,
            Some(MouseButton::Left),
            (4, 2),
            Mods::default(),
            false,
        );
        assert!(none.is_empty(), "{none:?}");
        // Button events, SGR.
        vt.write(b"\x1b[?1002h\x1b[?1006h");
        assert!(vt.mouse_tracking());
        let press = vt.encode_mouse(
            MouseAction::Press,
            Some(MouseButton::Left),
            (4, 2),
            Mods::default(),
            false,
        );
        assert_eq!(press, b"\x1b[<0;5;3M");
        let drag = vt.encode_mouse(
            MouseAction::Motion,
            Some(MouseButton::Left),
            (6, 2),
            Mods::default(),
            true,
        );
        assert_eq!(drag, b"\x1b[<32;7;3M");
        let up = vt.encode_mouse(
            MouseAction::Release,
            Some(MouseButton::Left),
            (6, 2),
            Mods::default(),
            false,
        );
        assert_eq!(up, b"\x1b[<0;7;3m");
        let wheel = vt.encode_mouse(
            MouseAction::Press,
            Some(MouseButton::WheelUp),
            (0, 0),
            Mods::default(),
            false,
        );
        assert_eq!(wheel, b"\x1b[<64;1;1M");
        let moved = vt.encode_mouse(MouseAction::Motion, None, (1, 1), Mods::default(), false);
        assert!(moved.is_empty(), "no motion without a button in mode 1002: {moved:?}");
        // The alternate screen.
        assert!(!vt.alternate_screen());
        vt.write(b"\x1b[?1049h");
        assert!(vt.alternate_screen());
    }
}
