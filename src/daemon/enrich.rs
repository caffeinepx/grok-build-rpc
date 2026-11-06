//! Cheap, read-only lookups that enrich a session: git branch and Grok's session summary.

use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};

/// The repo's git dir: `.git`, or the target of a `.git` file (worktrees, submodules).
fn git_dir(workspace: &Path) -> Option<PathBuf> {
    let git = workspace.join(".git");
    if git.is_dir() {
        return Some(git);
    }
    let content = fs::read_to_string(&git).ok()?;
    let rel = content.trim().strip_prefix("gitdir:")?.trim();
    let p = PathBuf::from(rel);
    Some(if p.is_absolute() {
        p
    } else {
        workspace.join(p)
    })
}

/// Current branch of the repo at `workspace`, by reading `.git/HEAD` directly (no `git` process).
/// `None` on a detached HEAD or outside a repo.
pub fn git_branch(workspace: &Path) -> Option<String> {
    let head = fs::read_to_string(git_dir(workspace)?.join("HEAD")).ok()?;
    head.trim()
        .strip_prefix("ref: refs/heads/")
        .map(str::to_owned)
}

#[derive(Debug, Clone, PartialEq)]
pub struct Repo {
    /// Browsable https URL, credentials stripped.
    pub url: String,
    /// `owner/name` (or just `name`).
    pub name: String,
}

/// `origin` remote of the repo at `workspace`, from `.git/config` (or the worktree's common dir).
pub fn git_origin(workspace: &Path) -> Option<Repo> {
    let dir = git_dir(workspace)?;
    let common = fs::read_to_string(dir.join("commondir"))
        .ok()
        .map(|c| {
            let p = PathBuf::from(c.trim());
            if p.is_absolute() { p } else { dir.join(p) }
        })
        .unwrap_or(dir);
    let config = fs::read_to_string(common.join("config")).ok()?;
    let mut in_origin = false;
    for line in config.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            in_origin = line == r#"[remote "origin"]"#;
        } else if in_origin
            && let Some((k, v)) = line.split_once('=')
            && k.trim() == "url"
        {
            return parse_remote(v.trim());
        }
    }
    None
}

/// `git@host:owner/name.git`, `ssh://git@host/owner/name`, `https://user:tok@host/owner/name.git`
/// → `https://host/owner/name`.
pub fn parse_remote(url: &str) -> Option<Repo> {
    let (host, path) = if let Some(rest) = url
        .strip_prefix("https://")
        .or_else(|| url.strip_prefix("http://"))
    {
        rest.split_once('/')?
    } else if let Some(rest) = url.strip_prefix("ssh://") {
        let (h, p) = rest.split_once('/')?;
        (h.split(':').next()?, p)
    } else if let Some((h, p)) = url.split_once(':') {
        if h.contains('/') || url.contains("://") {
            return None; // local path or unsupported scheme (file://, git://…)
        }
        (h, p)
    } else {
        return None;
    };
    let host = host.rsplit('@').next()?;
    let path = path.trim_matches('/').trim_end_matches(".git");
    if host.is_empty() || path.is_empty() {
        return None;
    }
    Some(Repo {
        url: format!("https://{host}/{path}"),
        name: path.to_owned(),
    })
}

#[derive(Debug, Default, Clone, PartialEq)]
pub struct Usage {
    pub total_tokens: u64,
    /// Unit unverified; assumed 1e-10 USD.
    pub cost_ticks: u64,
    pub turns: u64,
}

/// Reads `<session_dir>/usage.json` (`session.*` totals).
pub fn read_usage(session_dir: &Path) -> Option<Usage> {
    let v: Value = serde_json::from_slice(&fs::read(session_dir.join("usage.json")).ok()?).ok()?;
    let s = v.get("session")?;
    let n = |k: &str| s.get(k).and_then(Value::as_u64).unwrap_or(0);
    Some(Usage {
        total_tokens: n("totalTokens"),
        cost_ticks: n("costUsdTicks"),
        turns: n("turnCount"),
    })
}

#[derive(Debug, Default, Clone, PartialEq)]
pub struct Summary {
    pub model: Option<String>,
    pub effort: Option<String>,
    pub title: Option<String>,
}

/// Reads `<session_dir>/summary.json`.
pub fn read_summary(session_dir: &Path) -> Option<Summary> {
    let raw = fs::read(session_dir.join("summary.json")).ok()?;
    let v: Value = serde_json::from_slice(&raw).ok()?;
    let s = |k: &str| {
        v.get(k)
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
            .map(str::to_owned)
    };
    Some(Summary {
        model: s("current_model_id"),
        effort: s("reasoning_effort"),
        title: s("generated_title").or_else(|| s("session_summary")),
    })
}

/// Grok names a session group by percent-encoding the cwd (`/` → `%2F`).
pub fn encode_cwd(cwd: &str) -> String {
    let mut out = String::with_capacity(cwd.len() * 2);
    for b in cwd.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'.' | b'_' | b'~') {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

/// Best-effort session dir when an event has no `transcriptPath` (SessionStart `new`).
pub fn guess_session_dir(grok_home: &Path, cwd: &str, session_id: &str) -> Option<PathBuf> {
    let dir = grok_home
        .join("sessions")
        .join(encode_cwd(cwd))
        .join(session_id);
    dir.is_dir().then_some(dir)
}

/// `~/.grok/version.json` → `"1.0.70"`.
pub fn grok_version(grok_home: &Path) -> Option<String> {
    let v: Value = serde_json::from_slice(&fs::read(grok_home.join("version.json")).ok()?).ok()?;
    v.get("version").and_then(Value::as_str).map(str::to_owned)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encodes_cwd_like_grok() {
        assert_eq!(
            encode_cwd("/home/latte/Documents/grok-build-rpc"),
            "%2Fhome%2Flatte%2FDocuments%2Fgrok-build-rpc"
        );
    }

    #[test]
    fn parses_remotes() {
        let r = |u| parse_remote(u).map(|r| (r.url, r.name));
        let gh = Some(("https://github.com/o/n".to_string(), "o/n".to_string()));
        assert_eq!(r("git@github.com:o/n.git"), gh);
        assert_eq!(r("ssh://git@github.com:22/o/n.git"), gh);
        assert_eq!(r("https://user:token@github.com/o/n.git"), gh);
        assert_eq!(r("https://github.com/o/n/"), gh);
        assert_eq!(r("/srv/git/repo.git"), None);
        assert_eq!(r("file:///srv/repo"), None);
    }

    #[test]
    fn reads_usage_fixture_shape() {
        let dir = std::env::temp_dir().join(format!("grok-rpc-usage-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        fs::write(
            dir.join("usage.json"),
            r#"{"session":{"totalTokens":61005,"costUsdTicks":202497200,"turnCount":2}}"#,
        )
        .unwrap();
        assert_eq!(
            read_usage(&dir),
            Some(Usage {
                total_tokens: 61005,
                cost_ticks: 202497200,
                turns: 2
            })
        );
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn reads_branch_from_head() {
        // Self-contained: CI checks tags out with a detached HEAD, so don't use this repo.
        let dir = std::env::temp_dir().join(format!("grok-rpc-git-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let repo = dir.join("repo");
        fs::create_dir_all(repo.join(".git")).unwrap();

        fs::write(repo.join(".git/HEAD"), "ref: refs/heads/feature/x\n").unwrap();
        assert_eq!(git_branch(&repo).as_deref(), Some("feature/x"));

        fs::write(
            repo.join(".git/HEAD"),
            "0f9c261aa0b1c2d3e4f5a6b7c8d9e0f1a2b3c4d5\n",
        )
        .unwrap();
        assert_eq!(git_branch(&repo), None, "detached HEAD has no branch");

        // Worktree: `.git` is a file pointing at the real git dir.
        let wt = dir.join("wt");
        let wt_git = dir.join("wt-gitdir");
        fs::create_dir_all(&wt).unwrap();
        fs::create_dir_all(&wt_git).unwrap();
        fs::write(wt.join(".git"), format!("gitdir: {}\n", wt_git.display())).unwrap();
        fs::write(wt_git.join("HEAD"), "ref: refs/heads/main\n").unwrap();
        assert_eq!(git_branch(&wt).as_deref(), Some("main"));

        assert_eq!(git_branch(&dir.join("not-a-repo")), None);
        let _ = fs::remove_dir_all(&dir);
    }
}
