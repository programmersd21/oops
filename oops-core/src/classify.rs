use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum DestructiveKind {
    Delete,
    Overwrite,
    Move,
    GitDestructive,
    Truncate,
}

impl DestructiveKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Delete => "delete",
            Self::Overwrite => "overwrite",
            Self::Move => "move",
            Self::GitDestructive => "git-destructive",
            Self::Truncate => "truncate",
        }
    }
}

pub fn classify(argv: &[String]) -> Option<DestructiveKind> {
    let cmd = Path::new(argv.first()?).file_name()?.to_str()?;
    let rest = &argv[1..];
    match cmd {
        "rm" | "unlink" | "rmdir" => Some(DestructiveKind::Delete),
        "mv" | "rename" => Some(DestructiveKind::Move),
        "dd" => Some(DestructiveKind::Overwrite),
        "truncate" => Some(DestructiveKind::Truncate),
        "sed" if rest.iter().any(|a| a == "-i" || a.starts_with("-i")) => {
            Some(DestructiveKind::Overwrite)
        }
        "find"
            if rest.iter().any(|a| a == "-delete")
                || rest.windows(2).any(|x| {
                    x[0] == "-exec"
                        && matches!(
                            Path::new(&x[1]).file_name().and_then(|s| s.to_str()),
                            Some("rm" | "unlink" | "rmdir")
                        )
                }) =>
        {
            Some(DestructiveKind::Delete)
        }
        "git" => match rest.first().map(String::as_str) {
            Some("reset") if rest.iter().any(|a| a == "--hard") => {
                Some(DestructiveKind::GitDestructive)
            }
            Some("clean") if rest.iter().any(|a| a.starts_with('-') && a.contains('f')) => {
                Some(DestructiveKind::GitDestructive)
            }
            Some("checkout") | Some("restore") if rest.iter().any(|a| a == "--") => {
                Some(DestructiveKind::GitDestructive)
            }
            _ => None,
        },
        _ => None,
    }
}

pub fn paths_at_risk(argv: &[String], cwd: &Path) -> Vec<PathBuf> {
    let Some(kind) = classify(argv) else {
        return vec![];
    };
    let args = &argv[1..];
    let mut raw: Vec<&String> = match kind {
        DestructiveKind::Move => args
            .iter()
            .filter(|a| !a.starts_with('-'))
            .take(1)
            .collect(),
        DestructiveKind::GitDestructive => vec![],
        _ => args
            .iter()
            .filter(|a| !a.starts_with('-') && *a != "--" && !a.starts_with('+'))
            .collect(),
    };
    if matches!(kind, DestructiveKind::Delete)
        && argv.first().map(|s| s.ends_with("find")).unwrap_or(false)
    {
        raw.clear();
    }
    if raw.is_empty()
        && matches!(
            kind,
            DestructiveKind::Delete | DestructiveKind::GitDestructive
        )
    {
        return vec![cwd.to_path_buf()];
    }
    raw.into_iter()
        .filter(|p| *p != ";" && *p != "{}")
        .map(|p| {
            let p = PathBuf::from(p);
            if p.is_absolute() { p } else { cwd.join(p) }
        })
        .collect()
}

pub fn parse_command(command: &str) -> Vec<String> {
    shell_words::split(command).unwrap_or_default()
}

/// The raw command is needed because shell redirections are not argv entries.
pub fn classify_command(command: &str) -> Option<DestructiveKind> {
    let argv = parse_command(command);
    classify(&argv).or_else(|| {
        crate::redirect_scan::truncating_redirect(command).map(|_| DestructiveKind::Overwrite)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn table() {
        for (s, expected) in [
            ("rm -rf x", Some(DestructiveKind::Delete)),
            ("sed -i s/a/b/ a", Some(DestructiveKind::Overwrite)),
            ("git reset --hard", Some(DestructiveKind::GitDestructive)),
            ("echo hello", None),
        ] {
            assert_eq!(classify(&parse_command(s)), expected);
        }
    }
}
