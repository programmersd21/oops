use oops_core::{
    Config, DaemonStatus, Request, Response, SnapshotStore, classify_command, paths_at_risk,
};
use std::{path::PathBuf, sync::Arc};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    net::{UnixListener, UnixStream},
    sync::Mutex,
    time::{self, Duration},
};
use tracing::{error, warn};

struct State {
    store: Mutex<SnapshotStore>,
}

pub async fn run() -> Result<(), Box<dyn std::error::Error>> {
    let config = Config::load();
    std::fs::create_dir_all(&config.data_dir)?;
    tracing_subscriber::fmt()
        .with_writer(std::fs::File::create(config.data_dir.join("oopsd.log"))?)
        .with_ansi(false)
        .init();
    let state = Arc::new(State {
        store: Mutex::new(SnapshotStore::open(config.clone())?),
    });
    let socket = config.data_dir.join("oopsd.sock");
    if socket.exists() {
        std::fs::remove_file(&socket)?;
    }
    let listener = UnixListener::bind(socket)?;
    sd_notify::notify(false, &[sd_notify::NotifyState::Ready])?;
    watchdog();
    gc_loop(state.clone());
    loop {
        let (stream, _) = listener.accept().await?;
        let s = state.clone();
        tokio::spawn(async move {
            if let Err(e) = handle(stream, s).await {
                warn!(error=%e,"ipc failed")
            }
        });
    }
}

async fn handle(
    stream: UnixStream,
    state: Arc<State>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let (read, mut write) = stream.into_split();
    let mut line = String::new();
    BufReader::new(read).read_line(&mut line).await?;
    let request: Request = serde_json::from_str(&line)?;
    let response = match request {
        Request::InternalNotify { cmd, cwd } => notify(&state, cmd, cwd).await,
        Request::Status => status(&state).await,
        Request::ListSnapshots { limit, offset } => state
            .store
            .lock()
            .await
            .summaries(limit.min(1000), offset)
            .map(Response::Snapshots)
            .unwrap_or_else(error),
        Request::Diff { snapshot_id } => match state.store.lock().await.diff(snapshot_id) {
            Ok(Some(x)) => Response::Diff(x),
            Ok(None) => Response::Error {
                code: 1,
                message: "snapshot not found".into(),
            },
            Err(e) => error(e),
        },
        Request::Gc => state
            .store
            .lock()
            .await
            .gc()
            .map(|_| Response::Ack)
            .unwrap_or_else(|e| Response::Error {
                code: 1,
                message: e,
            }),
        Request::Pin {
            snapshot_id,
            pinned,
        } => match state.store.lock().await.pin(snapshot_id, pinned) {
            Ok(true) => Response::Ack,
            Ok(false) => Response::Error {
                code: 1,
                message: "snapshot not found".into(),
            },
            Err(e) => error(e),
        },
        Request::Undo { snapshot_id } => match state.store.lock().await.undo(snapshot_id) {
            Ok(Some(x)) => Response::Undo(x.into()),
            Ok(None) => Response::Error {
                code: 1,
                message: "nothing to undo".into(),
            },
            Err(e) => Response::Error {
                code: 1,
                message: e,
            },
        },
    };
    write
        .write_all(serde_json::to_string(&response)?.as_bytes())
        .await?;
    write.write_all(b"\n").await?;
    Ok(())
}

fn error(e: impl std::fmt::Display) -> Response {
    Response::Error {
        code: 1,
        message: e.to_string(),
    }
}

async fn notify(state: &State, cmd: String, cwd: String) -> Response {
    let Some(kind) = classify_command(&cmd) else {
        return Response::Ack;
    };
    let argv = oops_core::classify::parse_command(&cmd);
    let cwd = PathBuf::from(&cwd);
    let mut paths = paths_at_risk(&argv, &cwd);
    if let Some(target) = oops_core::redirect_scan::truncating_redirect(&cmd) {
        paths.push(if target.is_absolute() {
            target
        } else {
            cwd.join(target)
        });
    }
    match state.store.lock().await.capture(&cmd, &cwd, kind, &paths) {
        Ok(_) => Response::Ack,
        Err(e) => {
            error!(error=%e,"snapshot failed");
            Response::Error {
                code: 1,
                message: e,
            }
        }
    }
}

async fn status(state: &State) -> Response {
    let store = state.store.lock().await;
    let (bytes, count) = store.usage().unwrap_or((0, 0));
    let user = std::env::var("USER").unwrap_or_default();
    Response::Status(DaemonStatus{
        ready:true,
        capture_backend:"shell-hook".into(),
        capture_detail:"destructive commands typed directly in an interactive shell with hooks loaded".into(),
        degraded_warning:Some("does not catch deletions from scripts, cron, services, GUI applications, or non-interactive shells that do not source the hook.".into()),
        hook_timeout_ms:std::env::var("OOPS_HOOK_TIMEOUT_MS").ok().and_then(|x|x.parse().ok()).unwrap_or(200),
        storage_bytes:bytes,
        snapshot_count:count,
        lingering:std::path::Path::new("/var/lib/systemd/linger").join(user).exists()
    })
}

fn gc_loop(state: Arc<State>) {
    tokio::spawn(async move {
        let mut timer = time::interval(Duration::from_secs(600));
        loop {
            timer.tick().await;
            let _ = state.store.lock().await.gc();
        }
    });
}

fn watchdog() {
    tokio::spawn(async move {
        let mut timer = time::interval(Duration::from_secs(5));
        loop {
            timer.tick().await;
            let _ = sd_notify::notify(false, &[sd_notify::NotifyState::Watchdog]);
        }
    });
}
