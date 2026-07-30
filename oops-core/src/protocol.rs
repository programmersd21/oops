use crate::DestructiveKind;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug)]
#[serde(tag = "method", content = "params")]
pub enum Request {
    Undo { snapshot_id: Option<u64> },
    ListSnapshots { limit: u32, offset: u32 },
    Diff { snapshot_id: u64 },
    Status,
    Gc,
    Pin { snapshot_id: u64, pinned: bool },
    InternalNotify { cmd: String, cwd: String },
}
#[derive(Serialize, Deserialize, Debug)]
#[serde(tag = "result", content = "data")]
pub enum Response {
    Undo(UndoResult),
    Snapshots(Vec<SnapshotSummary>),
    Diff(DiffResult),
    Status(DaemonStatus),
    Ack,
    Error { code: i32, message: String },
}
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct SnapshotSummary {
    pub id: u64,
    pub command: String,
    pub cwd: String,
    pub kind: DestructiveKind,
    pub created_at_ns: i64,
    pub method: String,
    pub restorable: bool,
    pub pinned: bool,
    pub total_bytes: i64,
    pub file_count: i64,
}
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct DiffFile {
    pub original_path: String,
    pub new_path: Option<String>,
    pub op: String,
    pub mode: u32,
    pub size_bytes: i64,
    pub recoverable: bool,
    pub entry_type: String,
}
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct DiffResult {
    pub snapshot: SnapshotSummary,
    pub files: Vec<DiffFile>,
}
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct UndoResult {
    pub restored: Vec<String>,
    pub conflicts: Vec<(String, String)>,
    pub failed: Vec<(String, String)>,
}
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct DaemonStatus {
    pub ready: bool,
    pub capture_backend: String,
    pub capture_detail: String,
    pub degraded_warning: Option<String>,
    pub hook_timeout_ms: u64,
    pub storage_bytes: i64,
    pub snapshot_count: i64,
    pub lingering: bool,
}
