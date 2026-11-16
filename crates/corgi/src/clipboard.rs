//! Images for panes: the clipboard's image (this machine's), a dropped image file (a paste of
//! its path), what kind of image some bytes are, and base64 for core's `terminal.put`, which
//! puts the file beside a terminal on its machine.

use std::path::PathBuf;
use std::time::Duration;

/// Core takes at most this much in one `terminal.put`.
pub const MAX: usize = 8 * 1024 * 1024;

#[derive(Debug, PartialEq)]
pub struct Image {
    pub bytes: Vec<u8>,
    /// `png`, `jpg`, `gif`, `webp` or `bmp`.
    pub ext: &'static str,
}

/// What kind of image these bytes are, by their first bytes; `None` if not one.
pub fn sniff(b: &[u8]) -> Option<&'static str> {
    if b.starts_with(b"\x89PNG\r\n\x1a\n") {
        Some("png")
    } else if b.starts_with(&[0xff, 0xd8, 0xff]) {
        Some("jpg")
    } else if b.starts_with(b"GIF87a") || b.starts_with(b"GIF89a") {
        Some("gif")
    } else if b.len() >= 12 && &b[..4] == b"RIFF" && &b[8..12] == b"WEBP" {
        Some("webp")
    } else if b.starts_with(b"BM") && b.len() > 14 {
        Some("bmp")
    } else {
        None
    }
}

fn image_ext(ext: &str) -> Option<&'static str> {
    match ext.to_ascii_lowercase().as_str() {
        "png" => Some("png"),
        "jpg" | "jpeg" => Some("jpg"),
        "gif" => Some("gif"),
        "webp" => Some("webp"),
        "bmp" => Some("bmp"),
        _ => None,
    }
}

/// A paste that is one image file dropped on the terminal: its path on this machine. Terminals
/// paste a dropped file as its path, quoted or with backslash escapes.
pub fn dropped(text: &str) -> Option<PathBuf> {
    let text = text.trim_end_matches(['\r', '\n']).trim();
    if text.is_empty() || text.contains(['\r', '\n']) {
        return None;
    }
    let quoted = text.len() >= 2
        && ((text.starts_with('\'') && text.ends_with('\'')) || (text.starts_with('"') && text.ends_with('"')));
    let text = if quoted { &text[1..text.len() - 1] } else { text };
    let text = text.strip_prefix("file://").unwrap_or(text);
    let mut path = String::with_capacity(text.len());
    let mut chars = text.chars();
    while let Some(c) = chars.next() {
        match c {
            '\\' if !quoted => path.push(chars.next().unwrap_or('\\')),
            c => path.push(c),
        }
    }
    let path = PathBuf::from(path);
    image_ext(path.extension()?.to_str()?)?;
    (path.is_absolute() && path.is_file()).then_some(path)
}

/// An image file, if it is one and small enough.
pub fn read_file(path: &std::path::Path) -> Result<Image, String> {
    let meta = std::fs::metadata(path).map_err(|e| e.to_string())?;
    if meta.len() as usize > MAX {
        return Err(format!("{} is over 8 MB", path.display()));
    }
    let bytes = std::fs::read(path).map_err(|e| e.to_string())?;
    let ext = sniff(&bytes).ok_or_else(|| format!("{} isn't an image", path.display()))?;
    Ok(Image { bytes, ext })
}

/// This machine's clipboard image: from `command` (a shell line that prints it) if given, else
/// the system's (macOS: osascript; Linux: wl-paste, then xclip).
pub fn read(command: Option<&str>) -> Result<Image, String> {
    let bytes = match command.filter(|c| !c.trim().is_empty()) {
        Some(line) => output("sh", &["-c", line])?,
        None => system()?,
    };
    if bytes.is_empty() {
        return Err("no image on the clipboard".into());
    }
    if bytes.len() > MAX {
        return Err("the clipboard's image is over 8 MB".into());
    }
    let ext = sniff(&bytes).ok_or("no image on the clipboard")?;
    Ok(Image { bytes, ext })
}

#[cfg(target_os = "macos")]
fn system() -> Result<Vec<u8>, String> {
    let file = std::env::temp_dir().join(format!(
        "corgi-clipboard-{}-{}.png",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos())
    ));
    let script = format!(
        "set png to (the clipboard as «class PNGf»)\n\
         set f to open for access POSIX file \"{}\" with write permission\n\
         write png to f\nclose access f",
        file.display()
    );
    let r = output("osascript", &["-e", &script]).map_err(|_| "no image on the clipboard".to_string());
    let bytes = r.and_then(|_| std::fs::read(&file).map_err(|e| e.to_string()));
    let _ = std::fs::remove_file(&file);
    bytes
}

#[cfg(all(unix, not(target_os = "macos")))]
fn system() -> Result<Vec<u8>, String> {
    let wayland = std::env::var_os("WAYLAND_DISPLAY").is_some();
    let mut tried = Vec::new();
    for ty in ["image/png", "image/jpeg", "image/gif", "image/webp", "image/bmp"] {
        let tries: [(&str, Vec<&str>); 2] = [
            ("wl-paste", vec!["--no-newline", "--type", ty]),
            ("xclip", vec!["-selection", "clipboard", "-t", ty, "-o"]),
        ];
        for (prog, args) in tries {
            if prog == "wl-paste" && !wayland {
                continue;
            }
            match output(prog, &args) {
                Ok(b) if sniff(&b).is_some() => return Ok(b),
                Ok(_) => {}
                Err(e) => tried.push(e),
            }
        }
    }
    Err(if tried.iter().all(|e| e.contains("not found")) && !tried.is_empty() {
        "can't read the clipboard: install wl-clipboard or xclip (or set paste.image_command)".into()
    } else {
        "no image on the clipboard".into()
    })
}

#[cfg(not(unix))]
fn system() -> Result<Vec<u8>, String> {
    Err("reading the clipboard's image isn't supported here: set paste.image_command".into())
}

/// What a program prints (at most `MAX` and a byte), if it succeeds within 5 s.
fn output(prog: &str, args: &[&str]) -> Result<Vec<u8>, String> {
    let o = crate::process::run(prog, args, Duration::from_secs(5), MAX + 1)?;
    if o.ok {
        Ok(o.bytes)
    } else {
        Err(format!("{prog} failed"))
    }
}

/// Standard base64, padded (what `terminal.put` takes).
pub fn base64(b: &[u8]) -> String {
    const A: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut s = String::with_capacity(b.len().div_ceil(3) * 4);
    for c in b.chunks(3) {
        let n = (c[0] as u32) << 16 | (*c.get(1).unwrap_or(&0) as u32) << 8 | *c.get(2).unwrap_or(&0) as u32;
        for i in 0..4 {
            if i <= c.len() {
                s.push(A[(n >> (18 - 6 * i) & 63) as usize] as char);
            } else {
                s.push('=');
            }
        }
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    const PNG: &[u8] = b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR";

    #[test]
    fn kinds_of_image() {
        assert_eq!(sniff(PNG), Some("png"));
        assert_eq!(sniff(&[0xff, 0xd8, 0xff, 0xe0]), Some("jpg"));
        assert_eq!(sniff(b"GIF89a.."), Some("gif"));
        assert_eq!(sniff(b"RIFF\0\0\0\0WEBPVP8 "), Some("webp"));
        assert_eq!(sniff(b"# Tasks"), None, "text served as an image");
        assert_eq!(sniff(b""), None);
    }

    #[test]
    fn base64_round_trips() {
        for s in [&b""[..], b"f", b"fo", b"foo", b"foob", b"fooba", b"foobar", PNG] {
            assert_eq!(crate::worktree::unbase64(&base64(s)).unwrap(), s);
        }
        assert_eq!(base64(b"foobar"), "Zm9vYmFy");
        assert_eq!(base64(b"fo"), "Zm8=");
    }

    #[test]
    fn dropped_files() {
        let dir = std::env::temp_dir().join(format!("corgi drop {}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let f = dir.join("shot 1.PNG");
        std::fs::write(&f, PNG).unwrap();
        let p = f.display().to_string();
        assert_eq!(dropped(&format!("'{p}'")), Some(f.clone()), "quoted");
        assert_eq!(dropped(&p.replace(' ', "\\ ")), Some(f.clone()), "escaped");
        assert_eq!(dropped(&format!("{}\n", p.replace(' ', "\\ "))), Some(f.clone()));
        assert_eq!(dropped(&format!("file://{p}")), Some(f.clone()));
        assert_eq!(dropped("shot.png"), None, "relative");
        assert_eq!(dropped(&format!("{p}\n{p}")), None, "two lines");
        assert_eq!(dropped("/no/such/file.png"), None);
        let txt = dir.join("notes.txt");
        std::fs::write(&txt, "x").unwrap();
        assert_eq!(dropped(&txt.display().to_string()), None, "not an image");
        assert_eq!(read_file(&f).unwrap().ext, "png");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_command_supplies_the_image() {
        let dir = std::env::temp_dir().join(format!("corgi-clip-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let f = dir.join("i.png");
        std::fs::write(&f, PNG).unwrap();
        let img = read(Some(&format!("cat '{}'", f.display()))).unwrap();
        assert_eq!((img.ext, img.bytes.len()), ("png", PNG.len()));
        assert!(read(Some("printf '# Tasks'")).unwrap_err().contains("no image"));
        assert!(read(Some("exit 1")).is_err());
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
