# Git worktrees

Make, open and remove git worktrees as workspaces, so each branch gets its own folder, panes and agents.

A git worktree is a second checkout of the same repository, on another branch, in its own folder. corgi makes them with `git worktree add` and opens each one as a workspace. corgi runs git through herdr-core, so all of this works the same for a workspace on another machine: git runs there.

## Make a worktree workspace

Use the New Workspace dialog with its Worktree switch on. Open it in one of these ways:

| How | What it starts with |
|---|---|
| `C-b W`, then `n` | Worktree on, Folder set to the current workspace's folder |
| `C-b :` `dialog new-worktree` | The same |
| `C-b N`, then switch Worktree on | Folder set to the focused pane's folder |

With Worktree on, two more fields appear:

| Field | What it does | Empty means |
|---|---|---|
| Folder | Any folder inside the repository | — |
| Branch | The branch to check out. An existing branch is checked out; a new name is created from the repository's current `HEAD`. | A new branch named like `worktree/brave-river-1a2b` |
| Path | Where the worktree goes | `<worktrees.directory>/<repository>/<branch>` |

The other fields (Name, Machine, Program) work as they do for any workspace. See [workspaces-and-windows.md](workspaces-and-windows.md).

Press Enter. The dialog says `making a worktree on <branch>…` while git runs. When git is done, the workspace opens in the new folder. If git fails, its last error line shows in the dialog and you can fix the fields and try again. Press Esc to close the dialog while you wait: the workspace still opens when git finishes.

### Where worktrees go

```toml
[worktrees]
# <directory>/<repository>/<branch>. ~ is home on the workspace's machine.
directory = "~/.corgi/worktrees"
```

The `<repository>` part is the name of the checkout's top-level folder. The `<branch>` part is the branch as a folder name: lowercase letters and digits, with every other run of characters turned into one `-`. For example, the branch `feat/Login` in `~/src/app` goes to `~/.corgi/worktrees/app/feat-login`.

## The Worktrees dialog

`C-b W` (`choose-worktree`, alias `worktrees`) lists the worktrees of the current workspace's repository. It runs `git worktree list` on the workspace's machine.

Each row shows:

- `●` if the worktree is open as a workspace
- its branch, or `(detached)`
- its path, with your home shown as `~`
- tags: `main` (the repository's own checkout), `changes` (uncommitted changes), `locked`

| Key | What it does |
|---|---|
| `j` `k`, `↑` `↓`, `C-n` `C-p` | Move |
| Enter | Go to its workspace, or open it as a new workspace |
| `n` | Make a new worktree (the dialog above) |
| `d` or Delete | Remove it, after asking |
| Esc, `q`, `C-c`, `C-g` | Close |

If the workspace has no folder, or its folder isn't in a git repository, corgi says so in the status line instead.

## Remove a worktree

Select it in `C-b W` and press `d`. corgi asks first, and warns you when:

- it has uncommitted changes: they will be lost
- it's locked
- it's open as a workspace: that workspace closes too, and the programs in its panes end

When you confirm, corgi closes the workspace (if open), then runs `git worktree remove`. With changes or a lock, it forces the removal. The status line says `removed the worktree at <path>`, or git's reason if it failed.

You can't remove the repository's own checkout (the row tagged `main`).

Removing a worktree doesn't delete its branch.

## How worktree workspaces group

In the sidebar and the tree chooser (`C-b w`), a workspace in a worktree is listed right under the workspace of its repository's main checkout, marked `↳`:

```
● app main
  ↳ ● feat-login feat/Login
  ↳ ○ worktree-brave-river-1a2b worktree/brave-river-1a2b
● notes
```

For this to happen:

- the parent workspace's folder must be the repository's main checkout itself (not a subfolder of it)
- both workspaces must be on the same machine

A workspace without a name of its own is named after the git checkout its first pane is in, and shows that checkout's branch. A worktree's name starts with its repository's: `app/feat-login`. Under its repository's row in the tree, it leaves that part out, as above; a filter still matches the whole name. corgi reads these git facts every 10 seconds, and at once when a workspace's folder changes. For workspaces on another machine, it reads them there, while that machine's line is up.

## Related

- [workspaces-and-windows.md](workspaces-and-windows.md): the New Workspace dialog
- [remote-machines.md](remote-machines.md): worktrees on another machine
- [navigation.md](navigation.md): the sidebar and tree chooser
- [configuration.md](configuration.md): `[worktrees]`
