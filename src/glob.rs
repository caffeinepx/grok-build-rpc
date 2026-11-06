//! Minimal path glob for `privacy.hidden_projects`.
//! `*` = any chars except `/`, `**` = anything (incl. `/`; `**/` may match zero dirs), `?` = one non-`/` char.

use std::path::Path;

pub fn matches(pattern: &str, text: &str) -> bool {
    fn go(p: &[u8], t: &[u8]) -> bool {
        match p.first() {
            None => t.is_empty(),
            Some(b'*') if p.get(1) == Some(&b'*') => {
                let rest = &p[2..];
                // `**/` can also match zero directories.
                if rest.first() == Some(&b'/') && go(&rest[1..], t) {
                    return true;
                }
                (0..=t.len()).any(|i| go(rest, &t[i..]))
            }
            Some(b'*') => {
                let rest = &p[1..];
                for i in 0..=t.len() {
                    if go(rest, &t[i..]) {
                        return true;
                    }
                    if i < t.len() && t[i] == b'/' {
                        break;
                    }
                }
                false
            }
            Some(b'?') => t.first().is_some_and(|&c| c != b'/') && go(&p[1..], &t[1..]),
            Some(&c) => t.first() == Some(&c) && go(&p[1..], &t[1..]),
        }
    }
    go(pattern.as_bytes(), text.as_bytes())
}

pub fn expand_tilde(p: &str) -> String {
    match (p.strip_prefix('~'), std::env::var("HOME")) {
        (Some(rest), Ok(home)) if rest.is_empty() || rest.starts_with('/') => {
            format!("{home}{rest}")
        }
        _ => p.to_owned(),
    }
}

/// True if `path` or any of its ancestors matches one of `patterns`.
/// A pattern without wildcards therefore hides that directory and everything below it.
pub fn path_matches_any(patterns: &[String], path: &Path) -> bool {
    patterns.iter().any(|raw| {
        let pat = expand_tilde(raw.trim());
        let pat = if pat.len() > 1 {
            pat.trim_end_matches('/')
        } else {
            pat.as_str()
        };
        path.ancestors().any(|a| matches(pat, &a.to_string_lossy()))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn glob_semantics() {
        assert!(matches("/a/*/c", "/a/b/c"));
        assert!(!matches("/a/*/c", "/a/b/x/c"));
        assert!(matches("/a/**/c", "/a/b/x/c"));
        assert!(matches("/a/**/c", "/a/c"));
        assert!(matches("/a/**", "/a/b/c"));
        assert!(matches("/a/b?", "/a/bc"));
        assert!(!matches("/a/b?", "/a/b/"));
        assert!(matches("*secret*", "topsecretproj"));
    }

    #[test]
    fn ancestors_and_plain_dirs() {
        let pats = vec!["/home/u/work".to_string(), "/srv/*-private".to_string()];
        assert!(path_matches_any(&pats, Path::new("/home/u/work")));
        assert!(path_matches_any(
            &pats,
            Path::new("/home/u/work/client/app")
        ));
        assert!(!path_matches_any(&pats, Path::new("/home/u/workshop")));
        assert!(path_matches_any(&pats, Path::new("/srv/acme-private/api")));
        assert!(!path_matches_any(&pats, Path::new("/srv/public")));
    }
}
