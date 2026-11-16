//! Drawing helpers shared by every screen.
//!
//! Dialogs (key hints, the tree chooser, and every dialog after them) are drawn over the
//! whole window and placed with [`centered`], so they open in the same place whatever the
//! sidebar, status line and panes are doing.

use corgi_vt::{Frame, Rgb, Wide};
use ratatui::buffer::Buffer;
use ratatui::layout::{Position, Rect};
use ratatui::style::{Color, Modifier, Style};

fn color(c: Option<Rgb>) -> Color {
    c.map_or(Color::Reset, |Rgb(r, g, b)| Color::Rgb(r, g, b))
}

/// Copy an emulator frame into `area`, clipped to whichever is smaller.
/// Returns the cursor position in screen coordinates, if visible inside `area`.
pub fn draw_frame(buf: &mut Buffer, area: Rect, frame: &Frame) -> Option<Position> {
    let w = area.width.min(frame.cols);
    let h = area.height.min(frame.rows);
    for y in 0..h {
        for x in 0..w {
            let src = frame.cell(x, y);
            let Some(dst) = buf.cell_mut((area.x + x, area.y + y)) else {
                continue;
            };
            if src.wide == Wide::Tail {
                // The wide glyph to the left covers this column; ratatui's diff
                // skips it, the same way its own wide-text widgets leave it.
                dst.reset();
                continue;
            }
            let mut m = Modifier::empty();
            for (on, flag) in [
                (src.bold, Modifier::BOLD),
                (src.italic, Modifier::ITALIC),
                (src.faint, Modifier::DIM),
                (src.underline, Modifier::UNDERLINED),
                (src.inverse, Modifier::REVERSED),
                (src.strike, Modifier::CROSSED_OUT),
                (src.invisible, Modifier::HIDDEN),
            ] {
                if on {
                    m |= flag;
                }
            }
            // A wide glyph that doesn't fit in the last column would spill out of the pane.
            let text = if src.text.is_empty() || (src.wide == Wide::Wide && x + 1 == w) {
                " "
            } else {
                &src.text
            };
            dst.set_symbol(text);
            dst.set_style(Style::new().fg(color(src.fg)).bg(color(src.bg)).add_modifier(m));
        }
    }
    let c = frame.cursor?;
    (c.x < w && c.y < h).then(|| Position::new(area.x + c.x, area.y + c.y))
}

/// A `w × h` box in the middle of `area`, shrunk to fit if `area` is smaller.
pub fn centered(area: Rect, w: u16, h: u16) -> Rect {
    let (w, h) = (w.min(area.width), h.min(area.height));
    Rect::new(area.x + (area.width - w) / 2, area.y + (area.height - h) / 2, w, h)
}

#[cfg(test)]
mod tests {
    #[test]
    fn centered_boxes() {
        use super::centered;
        use ratatui::layout::Rect;
        let screen = Rect::new(0, 0, 100, 30);
        assert_eq!(centered(screen, 40, 10), Rect::new(30, 10, 40, 10));
        assert_eq!(
            centered(screen, 41, 11),
            Rect::new(29, 9, 41, 11),
            "odd sizes: within a cell"
        );
        assert_eq!(centered(screen, 200, 50), screen, "too big: the whole area");
        assert_eq!(
            centered(Rect::new(10, 5, 20, 10), 10, 4),
            Rect::new(15, 8, 10, 4),
            "offset areas"
        );
    }

    use super::*;
    use corgi_vt::Vt;

    #[test]
    fn frame_lands_in_area_with_styles_and_wide_chars() {
        let mut vt = Vt::new(8, 2).unwrap();
        vt.write(b"\x1b[1;38;2;1;2;3mA\x1b[0m\xe4\xb8\xadz");
        let frame = vt.frame().unwrap();
        let mut buf = Buffer::empty(Rect::new(0, 0, 12, 4));
        let cursor = draw_frame(&mut buf, Rect::new(2, 1, 8, 2), frame);
        let a = &buf[(2, 1)];
        assert_eq!(a.symbol(), "A");
        assert_eq!(a.fg, Color::Rgb(1, 2, 3));
        assert!(a.modifier.contains(Modifier::BOLD));
        assert_eq!(buf[(3, 1)].symbol(), "中");
        assert_eq!(buf[(4, 1)].symbol(), " ");
        assert_eq!(buf[(5, 1)].symbol(), "z");
        assert_eq!(cursor, Some(Position::new(6, 1)));
    }

    #[test]
    fn clips_to_smaller_area() {
        let mut vt = Vt::new(10, 5).unwrap();
        vt.write(b"0123456789");
        let frame = vt.frame().unwrap();
        let mut buf = Buffer::empty(Rect::new(0, 0, 4, 1));
        draw_frame(&mut buf, Rect::new(0, 0, 4, 1), frame);
        assert_eq!(buf[(3, 0)].symbol(), "3");
    }
}
