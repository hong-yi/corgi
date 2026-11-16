//! Pastes from the host terminal into the focused pane, and images for panes: this machine's
//! clipboard image or a dropped image file goes beside the pane's terminal on its machine
//! (core's `terminal.put`), and the path it landed at is pasted instead. A pane on another
//! machine can't see this one's clipboard or files any other way.

use super::*;
use crate::config::PasteImages;

/// Where an image comes from.
#[derive(Debug)]
pub(super) enum From {
    Clipboard,
    File(std::path::PathBuf),
}

impl App {
    /// A paste from the host terminal, for the focused pane.
    pub(super) fn paste_into_pane(&mut self, text: String) {
        let Some(addr) = self.window().map(|w| w.focus.clone()) else {
            return;
        };
        let bridge = match self.config.paste.images {
            PasteImages::Remote => crate::machines::of(&addr).is_some(),
            PasteImages::Always => true,
            PasteImages::Off => false,
        };
        if bridge {
            // Nothing in it: the clipboard holds only an image.
            if text.is_empty() {
                return self.paste_image(addr, From::Clipboard);
            }
            if let Some(path) = crate::clipboard::dropped(&text) {
                return self.paste_image(addr, From::File(path));
            }
        }
        if let Some(p) = self.panes.get_mut(&addr) {
            let bytes = p.vt.encode_paste(&text);
            p.input(&bytes);
        }
    }

    /// `paste-image` (`C-b C-v`): the clipboard's image, into the focused pane.
    pub(super) fn paste_clipboard_image(&mut self) {
        match self.window().map(|w| w.focus.clone()) {
            Some(addr) => self.paste_image(addr, From::Clipboard),
            None => self.notify("no pane to paste into".into()),
        }
    }

    /// Read the image, put it beside `addr`'s terminal; `Msg::ImagePut` pastes where it landed.
    /// One at a time: it can be megabytes, on the connection keys travel on too.
    fn paste_image(&mut self, addr: String, from: From) {
        if self.uploading {
            self.notify("an image is still on its way".into());
            return;
        }
        self.uploading = true;
        let command = self.config.paste.image_command.clone();
        let (c, out) = (self.client.clone(), self.out.clone());
        let target = addr.clone();
        tokio::spawn(async move {
            // Reading it, and encoding it for core, away from the UI.
            let read = tokio::task::spawn_blocking(move || {
                let name = match &from {
                    From::File(p) => p.file_name().map(|n| n.to_string_lossy().into_owned()),
                    From::Clipboard => None,
                };
                let img = match from {
                    From::Clipboard => crate::clipboard::read(command.as_deref()),
                    From::File(p) => crate::clipboard::read_file(&p),
                }?;
                let name = name.unwrap_or_else(|| format!("clipboard.{}", img.ext));
                Ok::<_, String>(json!({
                    "terminal": target,
                    "name": name,
                    "data": crate::clipboard::base64(&img.bytes),
                }))
            })
            .await;
            let r = match read {
                Ok(Ok(params)) => match c.call("terminal.put", params).await {
                    Ok(v) => v["path"]
                        .as_str()
                        .map(str::to_string)
                        .ok_or_else(|| "core didn't say where it put it".to_string()),
                    Err(e) => Err(e.to_string()),
                },
                Ok(Err(e)) => Err(e),
                Err(e) => Err(e.to_string()),
            };
            let _ = out.send(Msg::ImagePut(addr, r));
        });
    }

    /// The image is beside the terminal: paste its path there (quoted if it needs to be).
    pub(super) fn image_put(&mut self, addr: String, r: Result<String, String>) {
        self.uploading = false;
        match r {
            Ok(path) => {
                let text = if path.contains(|c: char| c.is_whitespace() || "'\"\\$`".contains(c)) {
                    crate::worktree::quote(&path)
                } else {
                    path.clone()
                };
                match self.panes.get_mut(&addr) {
                    Some(p) => {
                        let bytes = p.vt.encode_paste(&text);
                        p.input(&bytes);
                        self.notify(format!("pasted an image: {path}"));
                    }
                    None => self.notify(format!("the image is at {path}")),
                }
            }
            Err(e) => self.notify(format!("no image pasted: {e}")),
        }
    }
}
