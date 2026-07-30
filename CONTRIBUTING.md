# Contributing

## Prerequisites

- Rust stable toolchain
- Linux (the daemon uses Unix sockets, `sd_notify`, and Linux-specific filesystem features)

## Development

```sh
# build all crates
cargo build --workspace

# run lints and tests before submitting
cargo fmt --all
cargo clippy --workspace -- -D warnings
cargo test --workspace
```

## Project layout

- `oops-core/` — shared types, command classifier, redirect scanner, SQLite storage, IPC protocol
- `oops/` — single binary: CLI client + daemon (`oops daemon` subcommand)
- `oops-tui/` — ratatui-based terminal UI for `list` and `diff`
- `oops/shell-hooks/` — bash/zsh/fish preexec hook scripts
- `systemd/` — systemd user service unit

## Design notes

- Capture is shell-hook-only (no eBPF). The hook blocks until the daemon confirms the snapshot is durable on disk.
- Shell truncating redirects (`> file`) are detected by a raw-byte scanner in `redirect_scan.rs` since they're shell syntax, not argv entries. fd redirects (`1>&2`) are correctly ignored.
- Snapshots use reflink (CoW) on Btrfs/ZFS, plain copy elsewhere. Each file's mtime is recorded for safe restore conflict detection.
- Undo restores the latest snapshot unconditionally (you're undoing the last command). Older snapshots check current mtime against recorded mtime to avoid clobbering independently modified files.
- GC evicts oldest unpinned snapshots when total exceeds 2 GB or age exceeds 48 hours.
- CLI commands that depend on the daemon (`undo`, `status`, `list`, etc.) check for the daemon socket before connecting and exit with setup instructions if the daemon is not running.
