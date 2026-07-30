use std::path::PathBuf;

pub fn truncating_redirect(command: &str) -> Option<PathBuf> {
    let bytes = command.as_bytes();
    let mut i = 0;
    let mut quote = None;
    while i < bytes.len() {
        let b = bytes[i];
        if b == b'\\' && quote != Some(b'\'') {
            i += 2;
            continue;
        }
        if matches!(b, b'\'' | b'"') {
            quote = if quote == Some(b) {
                None
            } else if quote.is_none() {
                Some(b)
            } else {
                quote
            };
            i += 1;
            continue;
        }
        if quote.is_none()
            && b == b'>'
            && (i == 0 || bytes[i - 1] != b'>')
            && !matches!(bytes.get(i + 1), Some(b'>') | Some(b'&'))
        {
            i += 1;
            while bytes.get(i).is_some_and(|c| c.is_ascii_whitespace()) {
                i += 1;
            }
            let start = i;
            while bytes
                .get(i)
                .is_some_and(|c| !c.is_ascii_whitespace() && *c != b';' && *c != b'|')
            {
                i += 1;
            }
            if start < i {
                return Some(PathBuf::from(
                    String::from_utf8_lossy(&bytes[start..i])
                        .trim_matches(['\'', '"'])
                        .to_string(),
                ));
            }
        }
        i += 1;
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn redirects() {
        assert_eq!(
            truncating_redirect("echo x > file"),
            Some(PathBuf::from("file"))
        );
        assert_eq!(truncating_redirect("echo x >> file"), None);
        assert_eq!(truncating_redirect("echo '>'"), None);
        assert_eq!(
            truncating_redirect("__bp_invoke_preexec_from_ps0 \"$_\" 1>&2"),
            None
        );
        assert_eq!(truncating_redirect("cmd >&2"), None);
        assert_eq!(
            truncating_redirect("echo x > file 2>&1"),
            Some(PathBuf::from("file"))
        );
    }
}
