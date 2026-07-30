use std::path::PathBuf;

#[derive(Clone, Debug)]
pub struct Config {
    pub data_dir: PathBuf,
    pub allowed_roots: Vec<PathBuf>,
    pub retention_ns: i64,
    pub max_bytes: i64,
}

impl Config {
    pub fn load() -> Self {
        let home = std::env::var_os("HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("/"));
        let data_dir = std::env::var_os("XDG_DATA_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| home.join(".local/share"))
            .join("oops");
        let mut roots = vec![home];
        if let Ok(extra) = std::env::var("OOPS_ALLOW_PATHS") {
            roots.extend(
                extra
                    .split(':')
                    .filter(|p| !p.is_empty())
                    .map(PathBuf::from),
            );
        }
        Self {
            data_dir,
            allowed_roots: roots,
            retention_ns: 48 * 60 * 60 * 1_000_000_000,
            max_bytes: 2 * 1024 * 1024 * 1024,
        }
    }
    pub fn permits(&self, path: &std::path::Path) -> bool {
        let candidate = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
        self.allowed_roots.iter().any(|r| candidate.starts_with(r))
    }
}
