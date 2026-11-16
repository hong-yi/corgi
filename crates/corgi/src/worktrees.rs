//! The Worktrees dialog (`C-b W`): a repository's git worktrees, which are open as workspaces,
//! which have changes. Enter opens one (or goes to its workspace), `n` makes one, `d` removes one.
//! Pure: what `git worktree list --porcelain` says, the list's keys and drawing.

use crate::theme::Theme;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Widget};
use unicode_width::UnicodeWidthStr;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Worktree {
    pub path: String,
    /// The branch checked out (none: detached, or bare).
    pub branch: Option<String>,
    /// The repository's own checkout (the first listed): it can't be removed.
    pub main: bool,
    /// Uncommitted changes in it.
    pub dirty: bool,
    pub locked: bool,
}

/// The shell line that lists `folder`'s repository's worktrees: git's porcelain list, a `---`,
/// then `path<TAB>x` for each one with changes.
pub fn list_script(folder: &str) -> String {
    let f = crate::worktree::quote(folder);
    format!(
        "git -C {f} worktree list --porcelain && echo --- && \
         git -C {f} worktree list --porcelain | sed -n 's/^worktree //p' | \
         while IFS= read -r p; do [ -n \"$(git -C \"$p\" status --porcelain 2>/dev/null | head -c1)\" ] && printf '%s\\tx\\n' \"$p\"; done; true"
    )
}

/// What `list_script` printed.
pub fn parse(out: &str) -> Vec<Worktree> {
    let (list, dirty) = out.split_once("\n---\n").unwrap_or((out, ""));
    let dirty: Vec<&str> = dirty
        .lines()
        .filter_map(|l| l.split_once('\t').map(|(p, _)| p))
        .collect();
    let mut out: Vec<Worktree> = Vec::new();
    for line in list.lines() {
        if let Some(p) = line.strip_prefix("worktree ") {
            out.push(Worktree {
                path: p.to_string(),
                main: out.is_empty(),
                dirty: dirty.contains(&p),
                ..Default::default()
            });
        } else if let Some(w) = out.last_mut() {
            if let Some(b) = line.strip_prefix("branch ") {
                w.branch = Some(b.trim_start_matches("refs/heads/").to_string());
            } else if line == "locked" || line.starts_with("locked ") {
                w.locked = true;
            }
        }
    }
    out
}

#[derive(Debug, Default)]
pub struct View {
    pub at: usize,
}

#[derive(Debug, PartialEq)]
pub enum Outcome {
    Stay,
    Close,
    /// Open (or go to) the worktree at this index.
    Open(usize),
    New,
    Remove(usize),
}

impl View {
    pub fn key(&mut self, k: &KeyEvent, n: usize) -> Outcome {
        let ctrl = k.modifiers.contains(KeyModifiers::CONTROL);
        match k.code {
            KeyCode::Esc | KeyCode::Char('q') => return Outcome::Close,
            KeyCode::Char('c' | 'g') if ctrl => return Outcome::Close,
            KeyCode::Up | KeyCode::Char('k') => self.at = self.at.saturating_sub(1),
            KeyCode::Char('p') if ctrl => self.at = self.at.saturating_sub(1),
            KeyCode::Down | KeyCode::Char('j') => self.at = (self.at + 1).min(n.saturating_sub(1)),
            KeyCode::Char('n') if ctrl => self.at = (self.at + 1).min(n.saturating_sub(1)),
            KeyCode::Enter if n > 0 => return Outcome::Open(self.at),
            KeyCode::Char('n') => return Outcome::New,
            KeyCode::Char('d') | KeyCode::Delete if n > 0 => return Outcome::Remove(self.at),
            _ => {}
        }
        Outcome::Stay
    }

    /// `open`: which are open as workspaces; `repo`: a folder in the repository, for the title;
    /// `home`: shown as `~`.
    pub fn render(
        &mut self,
        list: &[Worktree],
        open: &[bool],
        (repo, home): (&str, &str),
        area: Rect,
        buf: &mut Buffer,
        t: &Theme,
    ) {
        self.at = self.at.min(list.len().saturating_sub(1));
        let short = |p: &str| match p.strip_prefix(home) {
            Some(rest) if !home.is_empty() && (rest.is_empty() || rest.starts_with('/')) => format!("~{rest}"),
            _ => p.to_string(),
        };
        let rows: Vec<(String, String, String)> = list
            .iter()
            .map(|w| {
                let branch = w.branch.clone().unwrap_or_else(|| "(detached)".into());
                let mut tags = Vec::new();
                if w.main {
                    tags.push("main");
                }
                if w.dirty {
                    tags.push("changes");
                }
                if w.locked {
                    tags.push("locked");
                }
                (branch, short(&w.path), tags.join(" · "))
            })
            .collect();
        let bw = rows.iter().map(|r| r.0.width()).max().unwrap_or(8).max(8);
        let footer = "j/k move · Enter open · n new · d remove · Esc close";
        let widest = rows
            .iter()
            .map(|r| bw + r.1.width() + r.2.width() + 10)
            .max()
            .unwrap_or(0);
        let w = (widest.max(footer.width() + 4) as u16 + 2)
            .min(area.width.saturating_sub(4))
            .max(30.min(area.width));
        let h = (list.len().max(1) as u16 + 5).min(area.height);
        let rect = crate::ui::centered(area, w, h);
        Clear.render(rect, buf);
        // Named after the repository's own checkout, wherever it was opened from.
        let repo = list.iter().find(|w| w.main).map_or(repo, |w| w.path.as_str());
        let title = format!(
            " worktrees of {} ",
            repo.trim_end_matches('/').rsplit('/').next().unwrap_or(repo)
        );
        let block = Block::default()
            .borders(Borders::ALL)
            .border_type(t.border_type())
            .border_style(Style::new().fg(t.prefix))
            .style(Style::new().bg(t.surface).fg(t.fg))
            .title(Span::styled(
                title,
                Style::new().fg(t.prefix).add_modifier(Modifier::BOLD),
            ));
        let inner = block.inner(rect);
        block.render(rect, buf);
        if list.is_empty() {
            buf.set_stringn(
                inner.x + 1,
                inner.y + 1,
                "no worktrees (not a git repository?)",
                inner.width as usize,
                Style::new().fg(t.muted),
            );
        }
        for (i, (branch, path, tags)) in rows.iter().enumerate() {
            let y = inner.y + 1 + i as u16;
            if y + 2 >= inner.bottom() {
                break;
            }
            let sel = i == self.at;
            let base = if sel {
                Style::new().bg(t.accent).fg(t.accent_fg)
            } else {
                Style::new().fg(t.fg)
            };
            let dim = if sel { base } else { Style::new().fg(t.muted) };
            let row = Rect::new(inner.x, y, inner.width, 1);
            buf.set_style(row, base);
            let mark = if open.get(i).copied().unwrap_or(false) {
                "● "
            } else {
                "  "
            };
            let mut spans = vec![
                Span::styled(format!(" {mark}"), if sel { base } else { Style::new().fg(t.done) }),
                Span::styled(format!("{branch:<bw$}  "), base.add_modifier(Modifier::BOLD)),
                Span::styled(path.clone(), dim),
            ];
            if !tags.is_empty() {
                let warn = list[i].dirty && !sel;
                spans.push(Span::styled(
                    format!("  {tags}"),
                    if warn { Style::new().fg(t.working) } else { dim },
                ));
            }
            Line::from(spans).render(row, buf);
        }
        let fw = (footer.width() as u16).min(inner.width);
        buf.set_stringn(
            inner.x + (inner.width - fw) / 2,
            inner.bottom() - 1,
            footer,
            fw as usize,
            Style::new().fg(t.muted),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const OUT: &str = "worktree /src/corgi\nHEAD 1111\nbranch refs/heads/main\n\nworktree /trees/corgi/feat-x\nHEAD 2222\nbranch refs/heads/feat/x\nlocked\n\nworktree /trees/corgi/old\nHEAD 3333\ndetached\n\n---\n/trees/corgi/feat-x\tx\n";

    #[test]
    fn porcelain() {
        let w = parse(OUT);
        assert_eq!(w.len(), 3);
        assert!(w[0].main && !w[1].main);
        assert_eq!(w[0].branch.as_deref(), Some("main"));
        assert_eq!(w[1].branch.as_deref(), Some("feat/x"));
        assert!(w[1].dirty && w[1].locked && !w[0].dirty);
        assert_eq!(w[2].branch, None, "detached");
        assert!(parse("").is_empty());
    }

    #[test]
    fn keys_and_drawing() {
        let w = parse(OUT);
        let mut v = View::default();
        let k = |c| KeyEvent::new(c, KeyModifiers::NONE);
        assert_eq!(v.key(&k(KeyCode::Char('j')), 3), Outcome::Stay);
        assert_eq!(v.key(&k(KeyCode::Enter), 3), Outcome::Open(1));
        assert_eq!(v.key(&k(KeyCode::Char('d')), 3), Outcome::Remove(1));
        assert_eq!(v.key(&k(KeyCode::Char('n')), 3), Outcome::New);
        assert_eq!(v.key(&k(KeyCode::Esc), 3), Outcome::Close);
        assert_eq!(v.key(&k(KeyCode::Enter), 0), Outcome::Stay, "nothing to open");
        let theme = crate::theme::builtin("terminal").unwrap();
        let area = Rect::new(0, 0, 90, 14);
        let mut buf = Buffer::empty(area);
        v.render(
            &w,
            &[true, false, false],
            ("/src/corgi", "/src"),
            area,
            &mut buf,
            &theme,
        );
        let text: Vec<String> = (0..14)
            .map(|y| (0..90).map(|x| buf[(x, y)].symbol().to_string()).collect())
            .collect();
        let all = text.join("\n");
        assert!(all.contains("worktrees of corgi"), "{all}");
        assert!(
            text.iter()
                .any(|l| l.contains("● main") && l.contains("~/corgi") && l.contains("main")),
            "{all}"
        );
        assert!(
            text.iter()
                .any(|l| l.contains("feat/x") && l.contains("changes · locked")),
            "{all}"
        );
        assert!(text.iter().any(|l| l.contains("(detached)")), "{all}");
        assert!(all.contains("d remove"));
    }

    #[test]
    fn the_list_script_runs() {
        if std::process::Command::new("git").arg("--version").output().is_err() {
            return;
        }
        let root = std::env::temp_dir().join(format!("corgi-wtl-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let repo = root.join("r");
        std::fs::create_dir_all(&repo).unwrap();
        let git = |args: &[&str]| {
            assert!(
                std::process::Command::new("git")
                    .current_dir(&repo)
                    .args(args)
                    .output()
                    .unwrap()
                    .status
                    .success(),
                "{args:?}"
            )
        };
        git(&["init", "-q"]);
        git(&[
            "-c",
            "user.email=h@example.invalid",
            "-c",
            "user.name=h",
            "commit",
            "-q",
            "--allow-empty",
            "-m",
            "x",
        ]);
        let other = root.join("other wt");
        git(&["worktree", "add", "-q", "-b", "feat", other.to_str().unwrap()]);
        std::fs::write(other.join("new.txt"), "x").unwrap();
        let out = std::process::Command::new("sh")
            .arg("-c")
            .arg(list_script(repo.to_str().unwrap()))
            .output()
            .unwrap();
        assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
        let w = parse(&String::from_utf8_lossy(&out.stdout));
        assert_eq!(w.len(), 2);
        assert!(w[1].path.ends_with("other wt") && w[1].dirty && !w[0].dirty, "{w:?}");
        assert_eq!(w[1].branch.as_deref(), Some("feat"));
        std::fs::remove_dir_all(&root).unwrap();
    }
}
