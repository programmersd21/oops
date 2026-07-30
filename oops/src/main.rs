use clap::{Parser, Subcommand};
use oops_core::{Request, Response};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    net::UnixStream,
    time::{Duration, timeout},
};

mod daemon;

#[derive(Parser)]
#[command(name = "oops", about = "Undo destructive shell commands")]
struct Cli {
    #[command(subcommand)]
    command: Option<Cmd>,
}

#[derive(Subcommand)]
enum Cmd {
    #[command(about = "Run the oops daemon (background service)")]
    Daemon,
    #[command(about = "Browse snapshot history (TUI)")]
    List,
    #[command(about = "Show files captured in a snapshot (TUI)")]
    Diff { id: u64 },
    #[command(about = "Restore files from a snapshot (defaults to newest)")]
    Undo { id: Option<u64> },
    #[command(about = "Show daemon status, storage usage, and lingering state")]
    Status,
    #[command(about = "Run garbage collection (48h / 2 GB retention)")]
    Gc,
    #[command(about = "Exempt a snapshot from garbage collection")]
    Pin { id: u64 },
    #[command(about = "Remove pinned exemption from a snapshot")]
    Unpin { id: u64 },
    #[command(about = "Print shell hook script for the given shell")]
    Init { shell: String },
    #[command(
        name = "__internal-notify",
        hide = true,
        about = "Notify daemon of an impending command (called by shell hook)"
    )]
    Notify {
        #[arg(long)]
        cmd: String,
        #[arg(long)]
        cwd: String,
    },
}

#[tokio::main]
async fn main() {
    let cli = Cli::parse();
    match cli.command {
        Some(Cmd::Daemon) => {
            if let Err(e) = daemon::run().await {
                eprintln!("daemon failed: {e}");
                std::process::exit(1)
            }
        }
        Some(Cmd::Init { shell }) => match shell.as_str() {
            "bash" => print!("{}", include_str!("../../shell-hooks/oops.bash")),
            "zsh" => print!("{}", include_str!("../../shell-hooks/oops.zsh")),
            "fish" => print!("{}", include_str!("../../shell-hooks/oops.fish")),
            _ => {
                eprintln!("supported shells: bash, zsh, fish");
                std::process::exit(1)
            }
        },
        Some(Cmd::List) => match send(Request::ListSnapshots {
            limit: 200,
            offset: 0,
        })
        .await
        {
            Ok(Response::Snapshots(x)) => {
                if let Err(e) = oops_tui::list(x) {
                    eprintln!("{e}");
                    std::process::exit(1)
                }
            }
            Ok(x) => show(x),
            Err(e) => down(e),
        },
        Some(Cmd::Diff { id }) => match send(Request::Diff { snapshot_id: id }).await {
            Ok(Response::Diff(x)) => {
                if let Err(e) = oops_tui::diff(x) {
                    eprintln!("{e}");
                    std::process::exit(1)
                }
            }
            Ok(x) => show(x),
            Err(e) => down(e),
        },
        Some(Cmd::Undo { id }) => handle(send(Request::Undo { snapshot_id: id }).await),
        None => handle(send(Request::Undo { snapshot_id: None }).await),
        Some(Cmd::Status) => handle(send(Request::Status).await),
        Some(Cmd::Gc) => handle(send(Request::Gc).await),
        Some(Cmd::Pin { id }) => handle(
            send(Request::Pin {
                snapshot_id: id,
                pinned: true,
            })
            .await,
        ),
        Some(Cmd::Unpin { id }) => handle(
            send(Request::Pin {
                snapshot_id: id,
                pinned: false,
            })
            .await,
        ),
        Some(Cmd::Notify { cmd, cwd }) => {
            let _ = send(Request::InternalNotify { cmd, cwd }).await;
        }
    }
}

fn check_daemon() -> Result<std::path::PathBuf, String> {
    let sock = oops_core::Config::load().data_dir.join("oopsd.sock");
    if !sock.exists() {
        return Err("daemon socket not found".into());
    }
    Ok(sock)
}

async fn send(request: Request) -> Result<Response, String> {
    let sock = check_daemon()?;
    let timeout_ms = std::env::var("OOPS_HOOK_TIMEOUT_MS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(200);
    let stream = timeout(
        Duration::from_millis(timeout_ms),
        UnixStream::connect(&sock),
    )
    .await
    .map_err(|_| String::from("oopsd not responding"))?
    .map_err(|e| format!("socket unavailable: {e}"))?;
    let (read, mut write) = stream.into_split();
    write
        .write_all(
            serde_json::to_string(&request)
                .map_err(|e| e.to_string())?
                .as_bytes(),
        )
        .await
        .map_err(|e| e.to_string())?;
    write.write_all(b"\n").await.map_err(|e| e.to_string())?;
    let mut s = String::new();
    timeout(
        Duration::from_millis(timeout_ms),
        BufReader::new(read).read_line(&mut s),
    )
    .await
    .map_err(|_| String::from("oopsd not responding"))?
    .map_err(|e| e.to_string())?;
    serde_json::from_str(&s).map_err(|e| e.to_string())
}

fn handle(r: Result<Response, String>) {
    match r {
        Ok(Response::Error { code, message }) => {
            eprintln!("error: {message}");
            std::process::exit(code)
        }
        Ok(x) => show(x),
        Err(e) => down(e),
    }
}

fn show(x: Response) {
    match x {
        Response::Undo(r) => {
            println!("restored {} file(s)", r.restored.len());
            let partial = !r.conflicts.is_empty() || !r.failed.is_empty();
            for (p, msg) in r.conflicts {
                eprintln!("conflict: {p}: {msg}")
            }
            for (p, msg) in r.failed {
                eprintln!("failed: {p}: {msg}")
            }
            if partial {
                std::process::exit(3)
            }
        }
        Response::Status(s) => {
            println!(
                "daemon: ready\ncapture: {} ({})\nhook timeout: {} ms\nstorage: {} snapshots, {} bytes\nlingering: {}",
                s.capture_backend,
                s.capture_detail,
                s.hook_timeout_ms,
                s.snapshot_count,
                s.storage_bytes,
                s.lingering
            );
            if let Some(w) = s.degraded_warning {
                println!("warning: {w}")
            }
        }
        Response::Ack => println!("ok"),
        Response::Error { .. } | Response::Snapshots(_) | Response::Diff(_) => println!("{x:?}"),
    }
}

fn down(e: String) -> ! {
    eprintln!("error: {e}");
    eprintln!("oops daemon is not running (exit code 2)");
    eprintln!("  start:  systemctl --user enable --now oopsd");
    eprintln!("  linger: loginctl enable-linger $USER");
    eprintln!("  status: systemctl --user status oopsd");
    std::process::exit(2)
}
