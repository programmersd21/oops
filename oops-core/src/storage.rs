use crate::protocol::{DiffFile, DiffResult, SnapshotSummary, UndoResult};
use crate::{Config, DestructiveKind};
use rusqlite::{Connection, OptionalExtension, params};
use std::{
    fs, io,
    os::unix::fs::{MetadataExt, PermissionsExt},
    path::{Path, PathBuf},
};

pub fn now_ns() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos() as i64)
        .unwrap_or(0)
}

fn path_str(p: &Path) -> String {
    p.display().to_string()
}

#[derive(Debug, Clone)]
pub struct SnapshotFile {
    pub original_path: PathBuf,
    pub new_path: Option<PathBuf>,
    pub op: String,
    pub blob_path: Option<String>,
    pub mode: u32,
    pub size_bytes: i64,
    pub entry_type: String,
    pub mtime_ns: i64,
}

#[derive(Debug, Default)]
pub struct RestoreReport {
    pub restored: Vec<PathBuf>,
    pub conflicts: Vec<(PathBuf, String)>,
    pub failed: Vec<(PathBuf, String)>,
}

impl From<RestoreReport> for UndoResult {
    fn from(v: RestoreReport) -> Self {
        Self {
            restored: v.restored.iter().map(|p| path_str(p)).collect(),
            conflicts: v
                .conflicts
                .iter()
                .map(|(p, s)| (path_str(p), s.clone()))
                .collect(),
            failed: v
                .failed
                .iter()
                .map(|(p, s)| (path_str(p), s.clone()))
                .collect(),
        }
    }
}

fn errmap<T, E: std::fmt::Display>(r: Result<T, E>) -> Result<T, String> {
    r.map_err(|e| e.to_string())
}

pub struct SnapshotStore {
    config: Config,
    db: Connection,
}

impl SnapshotStore {
    pub fn open(config: Config) -> rusqlite::Result<Self> {
        fs::create_dir_all(config.data_dir.join("blobs"))
            .map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))?;
        let db = Connection::open(config.data_dir.join("oops.db"))?;
        db.execute_batch("PRAGMA foreign_keys=ON; PRAGMA journal_mode=WAL;
 CREATE TABLE IF NOT EXISTS snapshots (id INTEGER PRIMARY KEY AUTOINCREMENT, command TEXT NOT NULL, cwd TEXT NOT NULL, kind TEXT NOT NULL, created_at_ns INTEGER NOT NULL, method TEXT NOT NULL DEFAULT 'copy', restorable BOOLEAN NOT NULL DEFAULT 0, total_bytes INTEGER NOT NULL, file_count INTEGER NOT NULL, pinned BOOLEAN NOT NULL DEFAULT 0);
  CREATE TABLE IF NOT EXISTS snapshot_files (id INTEGER PRIMARY KEY AUTOINCREMENT, snapshot_id INTEGER NOT NULL REFERENCES snapshots(id) ON DELETE CASCADE, original_path TEXT NOT NULL, new_path TEXT, op TEXT NOT NULL, blob_path TEXT, mode INTEGER NOT NULL, size_bytes INTEGER NOT NULL, entry_type TEXT NOT NULL DEFAULT 'file', mtime_ns INTEGER NOT NULL DEFAULT 0);
  CREATE INDEX IF NOT EXISTS idx_snapshot_files_snapshot_id ON snapshot_files(snapshot_id); CREATE INDEX IF NOT EXISTS idx_snapshots_created_at ON snapshots(created_at_ns DESC);")?;
        let _ = db.execute("ALTER TABLE snapshots DROP COLUMN backend", []);
        let _ = db.execute(
            "ALTER TABLE snapshots ADD COLUMN method TEXT NOT NULL DEFAULT 'copy'",
            [],
        );
        let _ = db.execute(
            "ALTER TABLE snapshot_files ADD COLUMN mtime_ns INTEGER NOT NULL DEFAULT 0",
            [],
        );
        Ok(Self { config, db })
    }
    pub fn capture(
        &mut self,
        command: &str,
        cwd: &Path,
        kind: DestructiveKind,
        roots: &[PathBuf],
    ) -> Result<Option<u64>, String> {
        let roots: Vec<_> = roots
            .iter()
            .filter(|p| self.config.permits(p))
            .cloned()
            .collect();
        if roots.is_empty() {
            return Ok(None);
        }
        let stamp = now_ns();
        errmap(self.db.execute("INSERT INTO snapshots(command,cwd,kind,created_at_ns,method,restorable,total_bytes,file_count) VALUES(?1,?2,?3,?4,'copy',0,0,0)", params![command, path_str(cwd), kind.as_str(), stamp]))?;
        let id = self.db.last_insert_rowid() as u64;
        let dir = self.config.data_dir.join("blobs").join(id.to_string());
        errmap(fs::create_dir_all(&dir))?;
        let mut entries = Vec::new();
        for root in roots {
            self.collect(&root, &mut entries)?;
        }
        let mut fully = true;
        let mut all_reflink = true;
        let mut bytes = 0i64;
        let mut blob_id = 0u64;
        for f in &mut entries {
            if f.entry_type == "file" {
                let source = f.original_path.clone();
                let blob = format!("{}.blob", blob_id);
                blob_id += 1;
                let target = dir.join(&blob);
                match snapshot_file(&source, &target) {
                    Ok((n, method)) => {
                        f.blob_path = Some(blob);
                        bytes += n as i64;
                        all_reflink &= method == "reflink";
                    }
                    Err(_) => fully = false,
                }
            }
        }
        let tx = errmap(self.db.transaction())?;
        for f in &entries {
            errmap(tx.execute(
                "INSERT INTO snapshot_files(snapshot_id,original_path,new_path,op,blob_path,mode,size_bytes,entry_type,mtime_ns) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9)",
                params![id, path_str(&f.original_path), f.new_path.as_ref().map(|x| path_str(x)), f.op, f.blob_path, f.mode, f.size_bytes, f.entry_type, f.mtime_ns],
            ))?;
        }
        errmap(tx.execute(
            "UPDATE snapshots SET restorable=?1,total_bytes=?2,file_count=?3,method=?4 WHERE id=?5",
            params![
                fully,
                bytes,
                entries.len() as i64,
                if all_reflink { "reflink" } else { "copy" },
                id
            ],
        ))?;
        errmap(tx.commit())?;
        Ok(Some(id))
    }
    fn collect(&self, root: &Path, out: &mut Vec<SnapshotFile>) -> Result<(), String> {
        let md = match fs::symlink_metadata(root) {
            Ok(m) => m,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(()),
            Err(e) => return Err(e.to_string()),
        };
        if md.file_type().is_symlink() {
            return Ok(());
        }
        let typ = if md.is_dir() {
            "dir"
        } else if md.is_file() {
            "file"
        } else {
            return Ok(());
        };
        out.push(SnapshotFile {
            original_path: root.to_path_buf(),
            new_path: None,
            op: "delete".into(),
            blob_path: None,
            mode: md.permissions().mode(),
            size_bytes: md.len() as i64,
            entry_type: typ.into(),
            mtime_ns: if md.is_file() {
                md.mtime() * 1_000_000_000 + md.mtime_nsec()
            } else {
                0
            },
        });
        if md.is_dir() {
            for child in fs::read_dir(root).map_err(|e| e.to_string())? {
                self.collect(&child.map_err(|e| e.to_string())?.path(), out)?;
            }
        }
        Ok(())
    }
    pub fn summaries(&self, limit: u32, offset: u32) -> rusqlite::Result<Vec<SnapshotSummary>> {
        let mut st=self.db.prepare("SELECT id,command,cwd,kind,created_at_ns,method,restorable,pinned,total_bytes,file_count FROM snapshots ORDER BY created_at_ns DESC LIMIT ?1 OFFSET ?2")?;
        st.query_map(params![limit, offset], row_summary)?.collect()
    }
    pub fn diff(&self, id: u64) -> rusqlite::Result<Option<DiffResult>> {
        let snapshot=self.db.query_row("SELECT id,command,cwd,kind,created_at_ns,method,restorable,pinned,total_bytes,file_count FROM snapshots WHERE id=?1",params![id],row_summary).optional()?;
        let Some(snapshot) = snapshot else {
            return Ok(None);
        };
        let mut st=self.db.prepare("SELECT original_path,new_path,op,blob_path,mode,size_bytes,entry_type FROM snapshot_files WHERE snapshot_id=?1 ORDER BY original_path")?;
        let files = st
            .query_map(params![id], |r| {
                Ok(DiffFile {
                    original_path: r.get(0)?,
                    new_path: r.get(1)?,
                    op: r.get(2)?,
                    recoverable: r.get::<_, Option<String>>(3)?.is_some()
                        || r.get::<_, String>(6)? == "dir",
                    mode: r.get(4)?,
                    size_bytes: r.get(5)?,
                    entry_type: r.get(6)?,
                })
            })?
            .collect::<rusqlite::Result<_>>()?;
        Ok(Some(DiffResult { snapshot, files }))
    }
    pub fn pin(&self, id: u64, pinned: bool) -> rusqlite::Result<bool> {
        Ok(self.db.execute(
            "UPDATE snapshots SET pinned=?1 WHERE id=?2",
            params![pinned, id],
        )? > 0)
    }
    pub fn usage(&self) -> rusqlite::Result<(i64, i64)> {
        self.db.query_row(
            "SELECT COALESCE(SUM(total_bytes),0),COUNT(*) FROM snapshots",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
    }
    pub fn gc(&mut self) -> Result<u32, String> {
        let (mut total, _) = errmap(self.usage())?;
        let cutoff = now_ns() - self.config.retention_ns;
        let mut removed = 0;
        loop {
            let row = errmap(
                self.db
                    .query_row(
                        "SELECT id,total_bytes FROM snapshots WHERE pinned=0 AND (created_at_ns<?1 OR ?2>?3) ORDER BY created_at_ns ASC LIMIT 1",
                        params![cutoff, total, self.config.max_bytes],
                        |r| Ok((r.get::<_, u64>(0)?, r.get::<_, i64>(1)?)),
                    )
                    .optional(),
            )?;
            let Some((id, size)) = row else { break };
            errmap(fs::remove_dir_all(
                self.config.data_dir.join("blobs").join(id.to_string()),
            ))?;
            errmap(
                self.db
                    .execute("DELETE FROM snapshots WHERE id=?1", params![id]),
            )?;
            total -= size;
            removed += 1;
        }
        Ok(removed)
    }
    pub fn undo(&self, id: Option<u64>) -> Result<Option<RestoreReport>, String> {
        let id = match id {
            Some(x) => Some(x),
            None => errmap(self.db.query_row("SELECT id FROM snapshots WHERE restorable=1 ORDER BY created_at_ns DESC LIMIT 1", [], |r| r.get::<_, u64>(0)).optional())?,
        };
        let Some(id) = id else { return Ok(None) };
        let (_stamp, is_latest) = errmap(
            self.db
                .query_row(
                    "SELECT created_at_ns, id = (SELECT id FROM snapshots WHERE restorable=1 ORDER BY created_at_ns DESC LIMIT 1) FROM snapshots WHERE id=?1 AND restorable=1",
                    params![id],
                    |r| Ok((r.get::<_, i64>(0)?, r.get::<_, Option<u64>>(1)?.is_some())),
                )
                .optional(),
        )?
        .ok_or("snapshot is not restorable")?;
        let mut st = errmap(self.db.prepare(
            "SELECT original_path,new_path,op,blob_path,mode,size_bytes,entry_type,mtime_ns FROM snapshot_files WHERE snapshot_id=?1 ORDER BY CASE entry_type WHEN 'dir' THEN 0 ELSE 1 END, original_path",
        ))?;
        let rows = errmap(st.query_map(params![id], row_file))?;
        let mut rep = RestoreReport::default();
        for f in rows {
            let f = errmap(f)?;
            self.restore_file(id, is_latest, &f, &mut rep);
        }
        Ok(Some(rep))
    }
    fn restore_file(&self, id: u64, is_latest: bool, f: &SnapshotFile, rep: &mut RestoreReport) {
        if f.original_path
            .components()
            .any(|c| c.as_os_str() == ".git")
        {
            rep.conflicts
                .push((f.original_path.clone(), "inside .git".into()));
            return;
        }
        if f.entry_type == "dir" {
            if let Err(e) = fs::create_dir_all(&f.original_path) {
                rep.failed.push((f.original_path.clone(), e.to_string()))
            } else {
                let _ = fs::set_permissions(&f.original_path, fs::Permissions::from_mode(f.mode));
                rep.restored.push(f.original_path.clone())
            };
            return;
        }
        if !is_latest
            && f.original_path.exists()
            && f.mtime_ns > 0
            && let Ok(m) = fs::metadata(&f.original_path)
        {
            let current = m.mtime() * 1_000_000_000 + m.mtime_nsec();
            if current != f.mtime_ns {
                rep.conflicts
                    .push((f.original_path.clone(), "modified since snapshot".into()));
                return;
            }
        }
        let Some(blob) = &f.blob_path else {
            rep.failed
                .push((f.original_path.clone(), "no recoverable blob".into()));
            return;
        };
        if let Some(parent) = f.original_path.parent()
            && let Err(e) = fs::create_dir_all(parent)
        {
            rep.failed.push((f.original_path.clone(), e.to_string()));
            return;
        }
        let src = self
            .config
            .data_dir
            .join("blobs")
            .join(id.to_string())
            .join(blob);
        match fs::copy(src, &f.original_path) {
            Ok(_) => {
                let _ = fs::set_permissions(&f.original_path, fs::Permissions::from_mode(f.mode));
                rep.restored.push(f.original_path.clone())
            }
            Err(e) => rep.failed.push((f.original_path.clone(), e.to_string())),
        }
    }
}
fn row_summary(r: &rusqlite::Row<'_>) -> rusqlite::Result<SnapshotSummary> {
    Ok(SnapshotSummary {
        id: r.get(0)?,
        command: r.get(1)?,
        cwd: r.get(2)?,
        kind: match r.get::<_, String>(3)?.as_str() {
            "delete" => DestructiveKind::Delete,
            "overwrite" => DestructiveKind::Overwrite,
            "move" => DestructiveKind::Move,
            "truncate" => DestructiveKind::Truncate,
            _ => DestructiveKind::GitDestructive,
        },
        created_at_ns: r.get(4)?,
        method: r.get(5)?,
        restorable: r.get(6)?,
        pinned: r.get(7)?,
        total_bytes: r.get(8)?,
        file_count: r.get(9)?,
    })
}
fn row_file(r: &rusqlite::Row<'_>) -> rusqlite::Result<SnapshotFile> {
    Ok(SnapshotFile {
        original_path: PathBuf::from(r.get::<_, String>(0)?),
        new_path: r.get::<_, Option<String>>(1)?.map(PathBuf::from),
        op: r.get(2)?,
        blob_path: r.get(3)?,
        mode: r.get(4)?,
        size_bytes: r.get(5)?,
        entry_type: r.get(6)?,
        mtime_ns: r.get(7)?,
    })
}
fn snapshot_file(src: &Path, dst: &Path) -> io::Result<(u64, &'static str)> {
    let tmp = dst.with_extension("tmp");
    let copied = reflink_copy::reflink_or_copy(src, &tmp)?;
    fs::File::open(&tmp)?.sync_all()?;
    fs::rename(tmp, dst)?;
    Ok((
        fs::metadata(src)?.len(),
        if copied.is_none() { "reflink" } else { "copy" },
    ))
}
