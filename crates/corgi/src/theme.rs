//! Colours for corgi's own chrome (status line, sidebar, borders, dialogs).
//! Panes always draw with the program's colours over your terminal's own
//! background; themes never repaint them.

use crate::config::ThemeConfig;
use ratatui::style::Color;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Theme {
    pub fg: Color,
    pub muted: Color,
    /// Background of the status line and sidebar.
    pub surface: Color,
    /// The active window tab and selections.
    pub accent: Color,
    pub accent_fg: Color,
    /// The workspace badge.
    pub badge: Color,
    pub badge_fg: Color,
    /// The PREFIX badge and key-hint keys.
    pub prefix: Color,
    pub border: Color,
    pub border_active: Color,
    pub working: Color,
    pub blocked: Color,
    pub done: Color,
    pub idle: Color,
    pub unknown: Color,
    /// The mode badge's colour in each state.
    pub mode_normal: Color,
    pub mode_prefix: Color,
    pub mode_sidebar: Color,
    pub mode_dialog: Color,
    /// Box corners: rounded (╭) or sharp (┌), from `ui.corners`.
    pub rounded: bool,
}

const fn rgb(hex: u32) -> Color {
    Color::Rgb((hex >> 16) as u8, (hex >> 8) as u8, hex as u8)
}

#[allow(clippy::too_many_arguments)]
const fn palette(
    fg: u32,
    muted: u32,
    surface: u32,
    accent: u32,
    on_accent: u32,
    badge: u32,
    prefix: u32,
    border: u32,
    green: u32,
    yellow: u32,
    red: u32,
    teal: u32,
) -> Theme {
    Theme {
        fg: rgb(fg),
        muted: rgb(muted),
        surface: rgb(surface),
        accent: rgb(accent),
        accent_fg: rgb(on_accent),
        badge: rgb(badge),
        badge_fg: rgb(on_accent),
        prefix: rgb(prefix),
        border: rgb(border),
        border_active: rgb(green),
        working: rgb(yellow),
        blocked: rgb(red),
        done: rgb(teal),
        idle: rgb(green),
        unknown: rgb(muted),
        mode_normal: rgb(accent),
        mode_prefix: rgb(prefix),
        mode_sidebar: rgb(green),
        mode_dialog: rgb(badge),
        rounded: false,
    }
}

pub const NAMES: [&str; 7] = [
    "catppuccin-mocha",
    "catppuccin-latte",
    "tokyo-night",
    "gruvbox-dark",
    "nord",
    "dracula",
    "terminal",
];

pub fn builtin(name: &str) -> Option<Theme> {
    Some(match name {
        "catppuccin-mocha" => palette(
            0xcdd6f4, 0x7f849c, 0x181825, 0x89b4fa, 0x11111b, 0xcba6f7, 0xf9e2af, 0x45475a, 0xa6e3a1, 0xf9e2af,
            0xf38ba8, 0x94e2d5,
        ),
        "catppuccin-latte" => palette(
            0x4c4f69, 0x8c8fa1, 0xe6e9ef, 0x1e66f5, 0xeff1f5, 0x8839ef, 0xdf8e1d, 0xbcc0cc, 0x40a02b, 0xdf8e1d,
            0xd20f39, 0x179299,
        ),
        "tokyo-night" => palette(
            0xc0caf5, 0x565f89, 0x16161e, 0x7aa2f7, 0x15161e, 0xbb9af7, 0xe0af68, 0x3b4261, 0x9ece6a, 0xe0af68,
            0xf7768e, 0x73daca,
        ),
        "gruvbox-dark" => palette(
            0xebdbb2, 0x928374, 0x1d2021, 0x83a598, 0x1d2021, 0xd3869b, 0xfabd2f, 0x504945, 0xb8bb26, 0xfabd2f,
            0xfb4934, 0x8ec07c,
        ),
        "nord" => palette(
            0xeceff4, 0x6c7a96, 0x2e3440, 0x88c0d0, 0x2e3440, 0xb48ead, 0xebcb8b, 0x4c566a, 0xa3be8c, 0xebcb8b,
            0xbf616a, 0x8fbcbb,
        ),
        "dracula" => palette(
            0xf8f8f2, 0x6272a4, 0x21222c, 0xbd93f9, 0x21222c, 0xff79c6, 0xf1fa8c, 0x44475a, 0x50fa7b, 0xf1fa8c,
            0xff5555, 0x8be9fd,
        ),
        // Your terminal's own palette: ANSI colours and its default background.
        "terminal" => Theme {
            fg: Color::Reset,
            muted: Color::DarkGray,
            surface: Color::Reset,
            accent: Color::Blue,
            accent_fg: Color::Black,
            badge: Color::Magenta,
            badge_fg: Color::Black,
            prefix: Color::Yellow,
            border: Color::DarkGray,
            border_active: Color::Green,
            working: Color::Yellow,
            blocked: Color::Red,
            done: Color::Cyan,
            idle: Color::Green,
            unknown: Color::DarkGray,
            mode_normal: Color::Blue,
            mode_prefix: Color::Yellow,
            mode_sidebar: Color::Green,
            mode_dialog: Color::Magenta,
            rounded: false,
        },
        _ => return None,
    })
}

/// `#rrggbb`, `#rgb`, or an ANSI name (`red`, `bright-blue`, `reset`).
pub fn parse_color(s: &str) -> Option<Color> {
    let s = s.trim();
    if let Some(hex) = s.strip_prefix('#') {
        let v = u32::from_str_radix(hex, 16).ok()?;
        return match hex.len() {
            6 => Some(rgb(v)),
            3 => Some(Color::Rgb(
                ((v >> 8) & 0xf) as u8 * 17,
                ((v >> 4) & 0xf) as u8 * 17,
                (v & 0xf) as u8 * 17,
            )),
            _ => None,
        };
    }
    Some(match s.to_ascii_lowercase().replace('_', "-").as_str() {
        "reset" | "default" => Color::Reset,
        "black" => Color::Black,
        "red" => Color::Red,
        "green" => Color::Green,
        "yellow" => Color::Yellow,
        "blue" => Color::Blue,
        "magenta" => Color::Magenta,
        "cyan" => Color::Cyan,
        "white" | "gray" => Color::Gray,
        "bright-black" | "dark-gray" => Color::DarkGray,
        "bright-red" => Color::LightRed,
        "bright-green" => Color::LightGreen,
        "bright-yellow" => Color::LightYellow,
        "bright-blue" => Color::LightBlue,
        "bright-magenta" => Color::LightMagenta,
        "bright-cyan" => Color::LightCyan,
        "bright-white" => Color::White,
        _ => return None,
    })
}

/// Whether a `#rrggbb` background is light (relative luminance above half).
pub fn is_light(hex: &str) -> bool {
    match parse_color(hex) {
        Some(Color::Rgb(r, g, b)) => (0.2126 * r as f32 + 0.7152 * g as f32 + 0.0722 * b as f32) / 255.0 > 0.5,
        _ => false,
    }
}

impl Theme {
    pub fn border_type(&self) -> ratatui::widgets::BorderType {
        if self.rounded {
            ratatui::widgets::BorderType::Rounded
        } else {
            ratatui::widgets::BorderType::Plain
        }
    }

    /// Top-left, top-right, bottom-left, bottom-right.
    pub fn corners(&self) -> [&'static str; 4] {
        if self.rounded {
            ["╭", "╮", "╰", "╯"]
        } else {
            ["┌", "┐", "└", "┘"]
        }
    }

    /// The theme a config asks for. `host_bg` is the terminal's background (from the display query) for `auto`.
    pub fn resolve(c: &ThemeConfig, host_bg: Option<&str>) -> Result<Theme, String> {
        let name = match c.name.as_str() {
            "auto" if host_bg.is_some_and(is_light) => c.light.as_str(),
            "auto" => c.dark.as_str(),
            n => n,
        };
        let mut t = builtin(name).ok_or_else(|| format!("unknown theme {name:?}; built in: {}", NAMES.join(", ")))?;
        for (key, value) in &c.custom {
            let color = parse_color(value).ok_or_else(|| format!("theme.custom.{key}: {value:?} isn't a colour"))?;
            let slot = match key.as_str() {
                "fg" => &mut t.fg,
                "muted" => &mut t.muted,
                "surface" => &mut t.surface,
                "accent" => &mut t.accent,
                "accent_fg" => &mut t.accent_fg,
                "badge" => &mut t.badge,
                "badge_fg" => &mut t.badge_fg,
                "prefix" => &mut t.prefix,
                "border" => &mut t.border,
                "border_active" => &mut t.border_active,
                "working" => &mut t.working,
                "blocked" => &mut t.blocked,
                "done" => &mut t.done,
                "idle" => &mut t.idle,
                "unknown" => &mut t.unknown,
                "mode_normal" => &mut t.mode_normal,
                "mode_prefix" => &mut t.mode_prefix,
                "mode_sidebar" => &mut t.mode_sidebar,
                "mode_dialog" => &mut t.mode_dialog,
                _ => return Err(format!("theme.custom.{key}: no such colour")),
            };
            *slot = color;
        }
        Ok(t)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_name_is_built_in() {
        for n in NAMES {
            assert!(builtin(n).is_some(), "{n}");
        }
        assert!(builtin("nope").is_none());
    }

    #[test]
    fn colours_parse() {
        assert_eq!(parse_color("#89b4fa"), Some(Color::Rgb(0x89, 0xb4, 0xfa)));
        assert_eq!(parse_color("#f00"), Some(Color::Rgb(255, 0, 0)));
        assert_eq!(parse_color("bright_blue"), Some(Color::LightBlue));
        assert_eq!(parse_color("reset"), Some(Color::Reset));
        assert_eq!(parse_color("#12"), None);
        assert_eq!(parse_color("blue-ish"), None);
    }

    #[test]
    fn auto_follows_the_terminal_background_and_custom_overrides() {
        let mut c = ThemeConfig::default();
        assert_eq!(
            Theme::resolve(&c, Some("#1e1e2e")).unwrap(),
            builtin("catppuccin-mocha").unwrap()
        );
        assert_eq!(
            Theme::resolve(&c, Some("#eff1f5")).unwrap(),
            builtin("catppuccin-latte").unwrap()
        );
        assert_eq!(
            Theme::resolve(&c, None).unwrap(),
            builtin("catppuccin-mocha").unwrap(),
            "unknown: dark"
        );
        c.name = "nord".into();
        c.custom.insert("accent".into(), "#ff0000".into());
        assert_eq!(Theme::resolve(&c, None).unwrap().accent, Color::Rgb(255, 0, 0));
        c.custom.insert("sparkle".into(), "#ff0000".into());
        assert!(Theme::resolve(&c, None).unwrap_err().contains("sparkle"));
    }
}
