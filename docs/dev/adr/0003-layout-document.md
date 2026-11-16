# ADR 0003: the layout lives in one core document; labels make it recoverable

- Status: accepted (M3, 2026-11-09)
- Context: [PLAN.md](../../PLAN.md) §1 D4, D5 and D6

## Decisions

1. **One document, on the local core.**
   - Workspaces, windows and pane trees are stored as the core document `corgi` on the machine corgi connects to.
     The schema is `model::Doc` (`schema: 1`).
   - Panes on other machines appear in it by address (`vps/t7`).
   - This replaces the earlier idea of one document per machine: one place to read, one compare-and-swap to win.
2. **Focus is shared, like tmux.**
   - The active window and the focused pane of each window are stored in the document, so every client sees the
     same thing.
   - Only *which workspace this client looks at* stays local, in `view-<session>.json`.
3. **Every corgi terminal carries `corgi.workspace` and `corgi.window` labels.**
   - On load, reconnect, `missed`, or a conflict, `Doc::reconcile` runs. It:
     - drops panes whose terminal is gone or finished, except remote ones, which vanish while their machine is down;
     - puts back live terminals whose labels point at a workspace or window, creating them if needed.
   - Terminals without corgi labels are never pulled in uninvited. They become the "Unplaced" list in the M6
     chooser.
   - M2's single `corgi.pane=main` terminal is adopted and relabelled.
4. **On a conflict, take theirs.**
   - A save that loses the compare-and-swap reloads the document. No terminal is lost, because labels put it back.
   - At worst a local layout tweak (a ratio, a rename) is dropped.
   - A three-way merge can come later if two-client use needs it.
5. **Saving rules.**
   - One write in flight at a time; changes made during it go in the next write.
   - Each saved version is also written to `backups/` (the last 20 are kept).
   - A document corgi can't parse is never overwritten.
6. **A pane closes when its program exits, as in tmux.**
   - The finished terminal's record is removed from core (`terminal.rm`).
   - When the last pane goes, corgi prints `[exited]` and exits 0.
   - This replaces M2's "Enter: new shell".
7. **Only the visible window's panes are attached.**
   - The others reattach when shown, with a fresh first paint, which is fast on a local core.
   - Status for every terminal still comes from events, so hidden windows show their status dots.

## Consequences

- Two corgi clients on the same core share focus, and their window switches are visible to each other. This is
  tmux's behaviour; per-client focus would need the view file to grow.
- Keys pressed before the first window exists, such as straight after launch, are queued and replayed. Keys typed
  while a pane attaches are queued in the pane.
