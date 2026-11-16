# Layout engine

The pane tree (`crates/corgi/src/layout.rs`), the workspace model around it (`model.rs`), how panes get their rectangles, the preset layouts, and the stored document.

Both modules are pure: no IO, no terminals. Every operation has a unit test in the same file.

## The model

```
Doc { schema, next, workspaces }
 └─ Workspace { id "s3", name?, folder, machine?, windows, active, last? }
     └─ Window { id "w4", name?, root: Node, focus, last?, zoomed, preset? }
         └─ Node: a binary split tree whose leaves are core terminal addresses ("t7", "vps/t7")
```

- `Doc::alloc('s' | 'w')` makes ids from one shared counter, `next`.
- A pane is named by its terminal's address (`PaneId = String`). There is no separate pane id.
- `Window::focus` is always a pane in `root`; `Window::set_focus` ignores a pane that isn't, and remembers the old
  one in `last`. `Workspace::select` does the same for windows.
- `Workspace::machine` is where new terminals start (`None`: this machine).

## The tree

```rust
enum Node {
    Pane(PaneId),
    Split { dir: Dir, ratio: f32, a: Box<Node>, b: Box<Node> },
}
```

`Dir::Horizontal` puts `a` and `b` side by side (tmux `split-window -h`); `Dir::Vertical` stacks them (`-v`).
`ratio` is the share of the space, less the separator, that `a` gets.

```
split t1 -h 50, then split t2 -v 50:

        Split H 0.5                 ┌────────┬────────┐
        /        \                  │        │   t2   │
    Pane t1    Split V 0.5          │   t1   ├────────┤
               /       \            │        │   t3   │
           Pane t2   Pane t3        └────────┴────────┘
```

## Operations

| `Node` method | Behaviour | Used by |
|---|---|---|
| `split(target, new, dir, pct)` | Replaces the `target` leaf with a split; the new pane is `b` (right or below) and gets `pct`% (clamped 1–99), so `ratio = 1 - pct/100`. | `Doc::split` (from `Purpose::Split` in `App::place`) |
| `remove(id)` | Drops a leaf; its sibling subtree takes the split's place. `None` when the tree is empty. | `Doc::remove_pane` |
| `swap(x, y)` | Exchanges two leaves. | `swap-pane` (`app/actions.rs`) |
| `rename(f)` | Rewrites leaf ids. | `Doc::rename_machine` (`old/t3` → `new/t3`) |
| `neighbour_with(area, sep, from, side)` | The pane adjacent on `side` with the most overlap; ties go to the top-most, then left-most. | `select-pane`, `swap-pane -L/-R/-U/-D` |
| `resize_with(area, sep, id, side, cells)` | Moves the deepest split of the matching direction that contains `id`, as tmux does. `a` stays at least one cell, and so does `b`. | `resize-pane` |
| `preset(p)` | Rebuilds the same panes, in reading order, as a preset. | `select-layout`, `next-layout`, the New Window dialog |
| `panes()` | Leaves in reading order (`a` before `b`, depth first). | everywhere |

`Doc` wraps these with the invariants that matter across windows:

- `Doc::split` falls back to the window's focused pane if `target` is gone, unzooms, and focuses the new pane.
- `Doc::remove_pane` removes empty windows, and empty workspaces with them. If the focused pane goes, focus moves
  to `last` when still present, else the first pane, and zoom ends. A removed `last` is cleared. A removed active
  window hands over to the workspace's `last` window, else the nearest.
- `Doc::locate(pane)` finds `(workspace, window)` for an address; a terminal appears in at most one window.
- `break-pane` is `remove_pane` plus `add_window`, and relabels the terminal (`App::label`).

## Geometry

`Node::geometry_with(area, sep)` returns every pane's `Rect` and the separators between them. Each split is cut by
`cut(area, dir, ratio, sep)`:

```
total = width (H) or height (V)
if total < sep + 2: everything to a, nothing to b        (tiny areas never panic)
avail = total - sep
a = round(avail * ratio), clamped to 1..=avail-1
b = avail - a, after a separator `sep` cells wide
```

`sep` is 1 for tmux-style shared dividers and `panes.padding` (0 or more) when each pane has its own box. With
`sep = 0` boxes touch.

The app turns the window's area into pane contents in steps (`crates/corgi/src/app/mod.rs`):

1. `App::areas` (`app/draw.rs`) cuts the screen into sidebar, tab bar and body. The sidebar hides below 60
   columns or in the narrow layout; its width is clamped to 12..=(screen width − 20). The panes' box needs a body of
   at least 6×4; a boxed tab bar needs 8 rows and 10 columns, else it's one plain row.
2. `App::tiling` picks the area and separator: separate panes in their own boxes (`sep = panes.padding`), separate
   panes without boxes (at least 1 blank cell), or shared one-cell dividers inside the panes' box.
3. `App::frames` is each pane's whole cell: `geometry_with`, or one full-size rectangle when the window is zoomed or
   the screen is narrow.
4. `App::titled` takes off the pane's own box (when at least 3×3) and the breadcrumbs row and its divider (0, 1 or
   2 rows, by height). Without boxes or breadcrumbs, `App::title_room` gives the top row of panes one row for tabs.
5. `App::inside` takes off the inset (`panes.inset` columns, `panes.inset_rows` rows) when the pane is wide or tall
   enough to keep at least two cells.

`App::rects` is the result: where each terminal goes, and so the size it attaches and resizes with. `App::spawn`
uses the same steps to start a terminal at its final size, so the shell doesn't redraw at once.

## Preset layouts

`Preset` (`layout.rs`), in `next-layout` order:

| Preset | Shape |
|---|---|
| `even-horizontal` | All side by side: a chain of horizontal splits with ratio `1/n`. |
| `even-vertical` | All stacked, the same way. |
| `main-horizontal` | The first pane on top with 60%, the rest side by side below. |
| `main-vertical` | The first pane on the left with 60%, the rest stacked on the right. |
| `tiled` | `ceil(sqrt(n))` columns per row, rows stacked evenly. |

`Window::preset` remembers the last one applied, so `next-layout` steps on from it. Presets keep the panes' reading
order, not their positions.

## The persisted document

The model is stored as the core document `corgi` (`model::DOC`), written by `App::save` with compare-and-swap
([protocol.md](protocol.md#documents-and-compare-and-swap)). serde derives the JSON:

```json
{"schema":1,"next":5,"workspaces":[{"id":"s1","folder":"/src/corgi","windows":[
  {"id":"w2","root":{"split":{"dir":"horizontal","ratio":0.5,"a":{"pane":"t1"},"b":{"pane":"vps/t2"}}},
   "focus":"t1","zoomed":false,"preset":null}],"active":"w2"}]}
```

`name`, `machine` and `last` are left out when unset. The test `serializes_compactly` pins the `Node` shape.

What's local, not in the document: which workspace this client shows, the tree's folding and the sidebar width,
in `view-<session>.json` (`persist.rs`), and the last 20 saved versions in `backups/` under the state folder.

## Recovering the layout from labels

Every terminal corgi starts carries `corgi.workspace` and `corgi.window` labels (`App::spawn`).
`Doc::reconcile(&terminals)` runs on load, reconnect, `missed`, and after a conflict:

- It removes panes whose terminal is gone or finished, except remote ones (the address has a `/`), which vanish
  while their machine is down.
- It puts back live terminals whose labels name a workspace (creating it, and the window, if needed). A terminal
  with only a workspace label gets a new window and is relabelled. M2's `corgi.pane` terminal is adopted into the
  first workspace.
- It never adopts terminals without corgi labels (popups carry `corgi.popup` instead).

It returns `Reconciled { removed, adopted, relabel }`; `App::reconcile` acts on it. See
[ADR 0003](adr/0003-layout-document.md).

## Schema versions

There is one schema, `SCHEMA = 1`, and no migration code yet. `doc_loaded` doesn't check the `schema` field; it
relies on serde. Fields added with `#[serde(default)]` read older documents unchanged. A document that doesn't
parse is never overwritten: corgi reports it and keeps working without saving.

If a change can't be additive, bump `SCHEMA`, read the old shape explicitly, and convert it in `App::doc_loaded`
before `reconcile`. Keep a test that loads a document of the old version.
