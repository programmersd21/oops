<div align="center">

![demo](https://raw.githubusercontent.com/programmersd21/oops/main/demo.gif)

[![Crate][badge-crate]][link-crate]
[![Downloads][badge-downloads]][link-crate]
[![MSRV][badge-msrv]][link-rust]
[![License][badge-license]][link-license]
[![Platform][badge-platform]][link-platform]
[![PRs][badge-prs]][link-prs]

### Undo daemon for destructive shell commands

Before `rm`, `mv`, `sed -i`, `>`, `git reset --hard`, and friends run, their target files get snapshotted — so you can undo the damage.

<br>

```sh
oops
```
**→ undoes whatever you just broke.**

</div>

---

[badge-crate]:     https://img.shields.io/crates/v/oops-rs?style=for-the-badge&logo=rust&label=crate&labelColor=1a1a1a&color=D85A30
[badge-downloads]: https://img.shields.io/crates/d/oops-rs?style=for-the-badge&logo=rust&label=downloads&labelColor=1a1a1a&color=D85A30
[badge-edition]: https://img.shields.io/badge/2024-fff?style=for-the-badge&logo=rust&logoColor=7F77DD&label=edition&labelColor=1a1a1a&color=7F77DD
[badge-msrv]:    https://img.shields.io/badge/1.85-fff?style=for-the-badge&logo=rust&logoColor=378ADD&label=MSRV&labelColor=1a1a1a&color=378ADD
[badge-license]: https://img.shields.io/badge/MIT-fff?style=for-the-badge&logo=opensourceinitiative&logoColor=fff&label=license&labelColor=1a1a1a&color=1D9E75
[badge-platform]: https://img.shields.io/badge/Linux-FCC624?style=for-the-badge&logo=linux&logoColor=fff&label=&labelColor=1a1a1a
[badge-prs]:     https://img.shields.io/badge/welcome-fff?style=for-the-badge&logo=github&logoColor=fff&label=PRs&labelColor=1a1a1a&color=D4537E
[link-crate]:    https://crates.io/crates/oops-rs
[link-repo]:     https://github.com/programmersd21/oops
[link-rust]:     https://rust-lang.org
[link-license]:  LICENSE
[link-platform]: https://kernel.org
[link-prs]:      https://github.com/programmersd21/oops/pulls

## Install

```sh
cargo build --release
install -Dm755 target/release/oops ~/.local/bin/oops
install -Dm644 systemd/oopsd.service ~/.config/systemd/user/oopsd.service
systemctl --user daemon-reload
systemctl --user enable --now oopsd
loginctl enable-linger "$USER"
eval "$(oops init bash)"   # or: zsh, fish
```

## Commands

| Command | Description |
|---|---|
| `oops` | Undo the most recent restorable snapshot |
| `oops undo [id]` | Undo a specific snapshot (default: newest) |
| `oops list` | Browse snapshot history (TUI, ↑↓/jk to navigate) |
| `oops diff <id>` | Show files captured in a snapshot (TUI) |
| `oops status` | Daemon status, storage usage, lingering state |
| `oops gc` | Trigger garbage collection |
| `oops pin <id>` / `oops unpin <id>` | Exempt / unexempt a snapshot from GC |
| `oops init <bash\|zsh\|fish>` | Print shell hook script |
| `oops daemon` | Run the background daemon (started via systemd) |

Commands that talk to the daemon exit with code `2` and print setup instructions if it's not running.

## How it works

A `preexec` hook (bash `DEBUG` trap, zsh/fish `preexec`) intercepts every command before execution and sends it to the daemon over a Unix socket. The daemon classifies the command — if it's destructive, it snapshots every at-risk path before acknowledging the hook. Only then does the shell run the command.

Files are copied (reflink on Btrfs/ZFS, plain copy elsewhere) to `~/.local/share/oops/blobs/<id>/`. Metadata lives in SQLite. mtime is recorded for conflict detection during restore.

`oops undo` replays the snapshot: directories first (with original permissions), then files from blobs. The latest snapshot restores unconditionally. Older snapshots check mtime to avoid clobbering independently modified files. `.git/` is never touched.

## Configuration

| Variable | Default | Description |
|---|---|---|
| `OOPS_HOOK_TIMEOUT_MS` | `200` | Max ms to wait for daemon ack before running the command unprotected |
| `OOPS_ALLOW_PATHS` | `$HOME` | Colon-separated additional directories to permit snapshotting |

Data lives in `$XDG_DATA_HOME/oops` (`~/.local/share/oops`). Retention is 48 hours or 2 GB, whichever hits first. Pinned snapshots are exempt.

## Limitations

Shell-hook capture cannot intercept destructive commands from cron, systemd services, GUI applications, scripts without the hook loaded, or non-interactive shells. fd redirects (`1>&2`, `>&2`) are correctly ignored; only truncating redirects (`> file`) trigger a snapshot. See `oops status` for the full degraded-warning message.

## Project layout

- `oops-core/` — types, command classifier, redirect scanner, SQLite storage, IPC protocol
- `oops/` — single binary (CLI + daemon subcommand)
- `oops-tui/` — ratatui terminal UI for `list` and `diff`
- `shell-hooks/` — bash, zsh, fish preexec hook scripts
- `systemd/` — systemd user service unit

## License

MIT
