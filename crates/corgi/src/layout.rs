//! A window's panes as a binary split tree, and its geometry. Pure: no IO,
//! no terminals, so every operation is unit-tested.
//!
//! Panes are named by their core terminal address. Splits leave a one-cell
//! separator between their halves, like tmux.

use ratatui::layout::Rect;
use serde::{Deserialize, Serialize};

pub type PaneId = String;

/// `Horizontal`: side by side (tmux `split-window -h`). `Vertical`: stacked (`split-window -v`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Dir {
    Horizontal,
    Vertical,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Side {
    Left,
    Right,
    Up,
    Down,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Node {
    Pane(PaneId),
    /// `ratio`: the share of the space (minus the separator) that `a` gets.
    Split {
        dir: Dir,
        ratio: f32,
        a: Box<Node>,
        b: Box<Node>,
    },
}

/// tmux's preset layouts, in `next-layout` order.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum Preset {
    #[default]
    EvenHorizontal,
    EvenVertical,
    MainHorizontal,
    MainVertical,
    Tiled,
}

impl Preset {
    pub const ALL: [Preset; 5] = [
        Preset::EvenHorizontal,
        Preset::EvenVertical,
        Preset::MainHorizontal,
        Preset::MainVertical,
        Preset::Tiled,
    ];

    pub fn next(self) -> Preset {
        let i = Preset::ALL.iter().position(|p| *p == self).unwrap_or(0);
        Preset::ALL[(i + 1) % Preset::ALL.len()]
    }

    pub fn name(self) -> &'static str {
        match self {
            Preset::EvenHorizontal => "even-horizontal",
            Preset::EvenVertical => "even-vertical",
            Preset::MainHorizontal => "main-horizontal",
            Preset::MainVertical => "main-vertical",
            Preset::Tiled => "tiled",
        }
    }
}

/// A separator line between two halves of a split.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Separator {
    pub rect: Rect,
    pub dir: Dir,
}

impl Node {
    pub fn pane(id: impl Into<PaneId>) -> Node {
        Node::Pane(id.into())
    }

    /// Panes in reading order (a before b, depth first).
    pub fn panes(&self) -> Vec<&PaneId> {
        let mut out = Vec::new();
        fn walk<'a>(n: &'a Node, out: &mut Vec<&'a PaneId>) {
            match n {
                Node::Pane(id) => out.push(id),
                Node::Split { a, b, .. } => {
                    walk(a, out);
                    walk(b, out);
                }
            }
        }
        walk(self, &mut out);
        out
    }

    pub fn contains(&self, id: &str) -> bool {
        self.panes().iter().any(|p| p.as_str() == id)
    }

    /// Split `target`, putting `new` after it (right or below), taking `pct`% of the space.
    pub fn split(&mut self, target: &str, new: PaneId, dir: Dir, pct: u8) -> bool {
        match self {
            Node::Pane(id) if id == target => {
                let old = std::mem::replace(self, Node::pane(""));
                let ratio = 1.0 - (pct.clamp(1, 99) as f32 / 100.0);
                *self = Node::Split {
                    dir,
                    ratio,
                    a: Box::new(old),
                    b: Box::new(Node::Pane(new)),
                };
                true
            }
            Node::Pane(_) => false,
            Node::Split { a, b, .. } => a.split(target, new.clone(), dir, pct) || b.split(target, new, dir, pct),
        }
    }

    /// Remove a pane; its sibling takes the space. `None` when the tree is now empty.
    pub fn remove(self, id: &str) -> Option<Node> {
        match self {
            Node::Pane(p) if p == id => None,
            Node::Pane(_) => Some(self),
            Node::Split { dir, ratio, a, b } => match ((*a).remove(id), (*b).remove(id)) {
                (Some(a), Some(b)) => Some(Node::Split {
                    dir,
                    ratio,
                    a: Box::new(a),
                    b: Box::new(b),
                }),
                (Some(only), None) | (None, Some(only)) => Some(only),
                (None, None) => None,
            },
        }
    }

    /// Give panes new ids: `f` says what each becomes (`None`: it stays).
    pub fn rename(&mut self, f: &dyn Fn(&str) -> Option<PaneId>) {
        match self {
            Node::Pane(id) => {
                if let Some(new) = f(id) {
                    *id = new;
                }
            }
            Node::Split { a, b, .. } => {
                a.rename(f);
                b.rename(f);
            }
        }
    }

    pub fn swap(&mut self, x: &str, y: &str) {
        match self {
            Node::Pane(id) if id == x => *id = y.to_string(),
            Node::Pane(id) if id == y => *id = x.to_string(),
            Node::Pane(_) => {}
            Node::Split { a, b, .. } => {
                a.swap(x, y);
                b.swap(x, y);
            }
        }
    }

    /// `geometry_with` with one-cell separators.
    #[cfg(test)]
    pub fn geometry(&self, area: Rect) -> (Vec<(PaneId, Rect)>, Vec<Separator>) {
        self.geometry_with(area, 1)
    }

    /// Where every pane goes inside `area`, with `sep` cells between panes (0: they touch, e.g.
    /// each in its own box), and the separators between them.
    pub fn geometry_with(&self, area: Rect, sep: u16) -> (Vec<(PaneId, Rect)>, Vec<Separator>) {
        let (mut panes, mut seps) = (Vec::new(), Vec::new());
        self.place(area, sep, &mut panes, &mut seps);
        (panes, seps)
    }

    fn place(&self, area: Rect, gap: u16, panes: &mut Vec<(PaneId, Rect)>, seps: &mut Vec<Separator>) {
        match self {
            Node::Pane(id) => panes.push((id.clone(), area)),
            Node::Split { dir, ratio, a, b } => {
                let (ra, sep, rb) = cut(area, *dir, *ratio, gap);
                if sep.width > 0 && sep.height > 0 {
                    seps.push(Separator { rect: sep, dir: *dir });
                }
                a.place(ra, gap, panes, seps);
                b.place(rb, gap, panes, seps);
            }
        }
    }

    /// `neighbour_with` with one-cell separators.
    #[cfg(test)]
    pub fn neighbour(&self, area: Rect, from: &str, side: Side) -> Option<PaneId> {
        self.neighbour_with(area, 1, from, side)
    }

    /// The pane next to `from` in direction `side` (most overlap wins), if any.
    pub fn neighbour_with(&self, area: Rect, sep: u16, from: &str, side: Side) -> Option<PaneId> {
        let (rects, _) = self.geometry_with(area, sep);
        let f = rects.iter().find(|(id, _)| id == from)?.1;
        let overlap = |a0: u16, a1: u16, b0: u16, b1: u16| a1.min(b1).saturating_sub(a0.max(b0));
        rects
            .iter()
            .filter(|(id, r)| id != from && r.width > 0 && r.height > 0)
            .filter_map(|(id, r)| {
                let (adjacent, ov) = match side {
                    Side::Left => (r.right() + sep == f.x, overlap(r.y, r.bottom(), f.y, f.bottom())),
                    Side::Right => (f.right() + sep == r.x, overlap(r.y, r.bottom(), f.y, f.bottom())),
                    Side::Up => (r.bottom() + sep == f.y, overlap(r.x, r.right(), f.x, f.right())),
                    Side::Down => (f.bottom() + sep == r.y, overlap(r.x, r.right(), f.x, f.right())),
                };
                (adjacent && ov > 0).then_some(((ov, std::cmp::Reverse((r.y, r.x))), id))
            })
            // Most overlap; on a tie, the top-most then left-most.
            .max_by_key(|(key, _)| *key)
            .map(|(_, id)| id.clone())
    }

    /// `resize_with` with one-cell separators.
    #[cfg(test)]
    pub fn resize(&mut self, area: Rect, id: &str, side: Side, cells: u16) -> bool {
        self.resize_with(area, 1, id, side, cells)
    }

    /// Move the nearest divider beside `id` toward `side` by `cells`. False if there is none that way.
    pub fn resize_with(&mut self, area: Rect, sep: u16, id: &str, side: Side, cells: u16) -> bool {
        let want = if matches!(side, Side::Left | Side::Right) {
            Dir::Horizontal
        } else {
            Dir::Vertical
        };
        let grow = matches!(side, Side::Right | Side::Down);
        self.resize_in(area, sep, id, want, grow, cells)
    }

    fn resize_in(&mut self, area: Rect, sep: u16, id: &str, want: Dir, grow: bool, cells: u16) -> bool {
        let Node::Split { dir, ratio, a, b } = self else {
            return false;
        };
        let (ra, _, rb) = cut(area, *dir, *ratio, sep);
        let in_a = a.contains(id);
        if !in_a && !b.contains(id) {
            return false;
        }
        // The deepest matching split moves first, as in tmux.
        let deeper = if in_a {
            a.resize_in(ra, sep, id, want, grow, cells)
        } else {
            b.resize_in(rb, sep, id, want, grow, cells)
        };
        if deeper || *dir != want {
            return deeper;
        }
        let total = if want == Dir::Horizontal {
            area.width
        } else {
            area.height
        }
        .saturating_sub(sep);
        if total < 2 {
            return false;
        }
        let a_now = if want == Dir::Horizontal { ra.width } else { ra.height } as i32;
        let delta = if grow { cells as i32 } else { -(cells as i32) };
        let a_new = (a_now + delta).clamp(1, total as i32 - 1);
        *ratio = a_new as f32 / total as f32;
        true
    }

    /// Rebuild with a preset arrangement of the same panes, in order.
    pub fn preset(&self, p: Preset) -> Node {
        let ids: Vec<PaneId> = self.panes().into_iter().cloned().collect();
        build(p, &ids)
    }
}

fn even(dir: Dir, ids: &[PaneId]) -> Node {
    match ids {
        [] => Node::pane(""),
        [one] => Node::Pane(one.clone()),
        [first, rest @ ..] => Node::Split {
            dir,
            ratio: 1.0 / ids.len() as f32,
            a: Box::new(Node::Pane(first.clone())),
            b: Box::new(even(dir, rest)),
        },
    }
}

fn build(p: Preset, ids: &[PaneId]) -> Node {
    let main = |dir: Dir, rest_dir: Dir| match ids {
        [] | [_] => even(dir, ids),
        [first, rest @ ..] => Node::Split {
            dir,
            ratio: 0.6,
            a: Box::new(Node::Pane(first.clone())),
            b: Box::new(even(rest_dir, rest)),
        },
    };
    match p {
        Preset::EvenHorizontal => even(Dir::Horizontal, ids),
        Preset::EvenVertical => even(Dir::Vertical, ids),
        Preset::MainHorizontal => main(Dir::Vertical, Dir::Horizontal),
        Preset::MainVertical => main(Dir::Horizontal, Dir::Vertical),
        Preset::Tiled => {
            let n = ids.len().max(1);
            let cols = (n as f32).sqrt().ceil() as usize;
            let rows: Vec<Node> = ids.chunks(cols).map(|row| even(Dir::Horizontal, row)).collect();
            stack(rows)
        }
    }
}

fn stack(mut rows: Vec<Node>) -> Node {
    let n = rows.len();
    if n <= 1 {
        return rows.pop().unwrap_or_else(|| Node::pane(""));
    }
    let first = rows.remove(0);
    Node::Split {
        dir: Dir::Vertical,
        ratio: 1.0 / n as f32,
        a: Box::new(first),
        b: Box::new(stack(rows)),
    }
}

/// Cut `area` for a split: (a, separator, b). Tiny areas give everything to `a`.
fn cut(area: Rect, dir: Dir, ratio: f32, sep: u16) -> (Rect, Rect, Rect) {
    let total = if dir == Dir::Horizontal {
        area.width
    } else {
        area.height
    };
    if total < sep + 2 {
        let empty = Rect::new(area.x, area.y, 0, 0);
        return (area, empty, empty);
    }
    let avail = total - sep;
    let a = ((avail as f32 * ratio).round() as u16).clamp(1, avail - 1);
    match dir {
        Dir::Horizontal => (
            Rect::new(area.x, area.y, a, area.height),
            Rect::new(area.x + a, area.y, sep, area.height),
            Rect::new(area.x + a + sep, area.y, avail - a, area.height),
        ),
        Dir::Vertical => (
            Rect::new(area.x, area.y, area.width, a),
            Rect::new(area.x, area.y + a, area.width, sep),
            Rect::new(area.x, area.y + a + sep, area.width, avail - a),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const AREA: Rect = Rect {
        x: 0,
        y: 1,
        width: 81,
        height: 25,
    };

    fn rect_of(n: &Node, id: &str) -> Rect {
        n.geometry(AREA).0.into_iter().find(|(p, _)| p == id).unwrap().1
    }

    #[test]
    fn splits_share_space_with_one_cell_separators() {
        let mut n = Node::pane("t1");
        assert!(n.split("t1", "t2".into(), Dir::Horizontal, 50));
        assert!(n.split("t2", "t3".into(), Dir::Vertical, 50));
        assert_eq!(n.panes(), ["t1", "t2", "t3"]);
        let (rects, seps) = n.geometry(AREA);
        assert_eq!(rects[0].1, Rect::new(0, 1, 40, 25));
        assert_eq!(rects[1].1, Rect::new(41, 1, 40, 12));
        assert_eq!(rects[2].1, Rect::new(41, 14, 40, 12));
        assert_eq!(
            seps,
            vec![
                Separator {
                    rect: Rect::new(40, 1, 1, 25),
                    dir: Dir::Horizontal
                },
                Separator {
                    rect: Rect::new(41, 13, 40, 1),
                    dir: Dir::Vertical
                },
            ]
        );
        assert!(!n.split("t9", "t4".into(), Dir::Vertical, 50), "unknown target");
    }

    #[test]
    fn split_percentage_is_the_new_panes_share() {
        let mut n = Node::pane("t1");
        n.split("t1", "t2".into(), Dir::Horizontal, 25);
        assert_eq!(rect_of(&n, "t2").width, 20);
        assert_eq!(rect_of(&n, "t1").width, 60);
    }

    #[test]
    fn removing_a_pane_gives_its_space_to_the_sibling() {
        let mut n = Node::pane("t1");
        n.split("t1", "t2".into(), Dir::Horizontal, 50);
        n.split("t2", "t3".into(), Dir::Vertical, 50);
        let n = n.remove("t2").unwrap();
        assert_eq!(n.panes(), ["t1", "t3"]);
        assert_eq!(rect_of(&n, "t3"), Rect::new(41, 1, 40, 25));
        let n = n.remove("t1").unwrap();
        assert_eq!(n, Node::pane("t3"));
        assert_eq!(n.remove("t3"), None);
    }

    #[test]
    fn panes_can_touch_or_keep_a_gap() {
        let mut n = Node::pane("t1");
        assert!(n.split("t1", "t2".into(), Dir::Horizontal, 50));
        let area = Rect::new(0, 0, 41, 10);
        // Touching (each pane its own box): no separator, the second starts where the first ends.
        let (r, seps) = n.geometry_with(area, 0);
        assert!(seps.is_empty());
        assert_eq!(r[0].1.right(), r[1].1.x);
        assert_eq!(r[0].1.width + r[1].1.width, 41);
        // A 3-cell gap.
        let (r, seps) = n.geometry_with(area, 3);
        assert_eq!(r[1].1.x - r[0].1.right(), 3);
        assert_eq!(seps[0].rect.width, 3);
        for sep in [0, 1, 3] {
            assert_eq!(
                n.neighbour_with(area, sep, "t1", Side::Right).as_deref(),
                Some("t2"),
                "sep {sep}"
            );
            assert_eq!(
                n.neighbour_with(area, sep, "t2", Side::Left).as_deref(),
                Some("t1"),
                "sep {sep}"
            );
        }
        let mut m = n.clone();
        let before = m.geometry_with(area, 0).0[0].1.width;
        assert!(m.resize_with(area, 0, "t1", Side::Right, 4));
        assert_eq!(m.geometry_with(area, 0).0[0].1.width, before + 4);
    }

    #[test]
    fn neighbours_by_direction() {
        let mut n = Node::pane("t1");
        n.split("t1", "t2".into(), Dir::Horizontal, 50);
        n.split("t2", "t3".into(), Dir::Vertical, 50);
        assert_eq!(
            n.neighbour(AREA, "t1", Side::Right).as_deref(),
            Some("t2"),
            "most overlap: the top one"
        );
        assert_eq!(n.neighbour(AREA, "t3", Side::Left).as_deref(), Some("t1"));
        assert_eq!(n.neighbour(AREA, "t3", Side::Up).as_deref(), Some("t2"));
        assert_eq!(n.neighbour(AREA, "t2", Side::Down).as_deref(), Some("t3"));
        assert_eq!(n.neighbour(AREA, "t1", Side::Left), None);
    }

    #[test]
    fn resize_moves_the_nearest_divider() {
        let mut n = Node::pane("t1");
        n.split("t1", "t2".into(), Dir::Horizontal, 50);
        n.split("t2", "t3".into(), Dir::Vertical, 50);
        assert!(n.resize(AREA, "t1", Side::Right, 5));
        assert_eq!(rect_of(&n, "t1").width, 45);
        assert!(
            n.resize(AREA, "t3", Side::Up, 2),
            "t3 is below its divider: Up grows it"
        );
        assert_eq!(rect_of(&n, "t2").height, 10);
        assert!(n.resize(AREA, "t2", Side::Left, 100), "clamped, never zero");
        assert_eq!(rect_of(&n, "t1").width, 1);
        let mut single = Node::pane("t1");
        assert!(!single.resize(AREA, "t1", Side::Left, 1));
    }

    #[test]
    fn swap_replace_and_presets() {
        let mut n = Node::pane("t1");
        n.split("t1", "t2".into(), Dir::Horizontal, 50);
        n.split("t2", "t3".into(), Dir::Horizontal, 50);
        n.split("t3", "t4".into(), Dir::Horizontal, 50);
        n.swap("t1", "t4");
        assert_eq!(n.panes(), ["t4", "t2", "t3", "t1"]);

        let even = n.preset(Preset::EvenHorizontal);
        let widths: Vec<u16> = even.geometry(AREA).0.iter().map(|(_, r)| r.width).collect();
        assert!(
            widths.iter().max().unwrap() - widths.iter().min().unwrap() <= 1,
            "even within a cell: {widths:?}"
        );
        assert_eq!(widths.iter().sum::<u16>() + 3, AREA.width, "plus three separators");
        let tiled = n.preset(Preset::Tiled).geometry(AREA).0;
        assert_eq!(tiled.iter().filter(|(_, r)| r.y == 1).count(), 2, "2x2 grid");
        let main = n.preset(Preset::MainVertical).geometry(AREA).0;
        assert_eq!(main[0].1.height, 25);
        assert!(main[0].1.width > main[1].1.width);
        assert_eq!(Preset::Tiled.next(), Preset::EvenHorizontal);
    }

    #[test]
    fn tiny_areas_never_panic() {
        let mut n = Node::pane("t1");
        n.split("t1", "t2".into(), Dir::Horizontal, 50);
        n.split("t2", "t3".into(), Dir::Vertical, 50);
        for w in 0..4 {
            for h in 0..4 {
                let (rects, _) = n.geometry(Rect::new(0, 0, w, h));
                assert_eq!(rects.len(), 3);
            }
        }
    }

    #[test]
    fn serializes_compactly() {
        let mut n = Node::pane("t1");
        n.split("t1", "vps/t2".into(), Dir::Vertical, 50);
        let j = serde_json::to_string(&n).unwrap();
        assert_eq!(
            j,
            r#"{"split":{"dir":"vertical","ratio":0.5,"a":{"pane":"t1"},"b":{"pane":"vps/t2"}}}"#
        );
        assert_eq!(serde_json::from_str::<Node>(&j).unwrap(), n);
    }
}
