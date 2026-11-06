//! Filesystem locations used by grok-rpc and Grok Build.

use std::env;
use std::path::PathBuf;

fn home() -> PathBuf {
    env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/tmp"))
}

fn user_suffix() -> String {
    env::var("USER").unwrap_or_else(|_| "user".into())
}

/// Per-user runtime dir (`$XDG_RUNTIME_DIR`, else `/tmp`).
pub fn runtime_dir() -> PathBuf {
    env::var_os("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .filter(|p| p.is_dir())
        .unwrap_or_else(|| PathBuf::from("/tmp"))
}

fn runtime_file(name: &str) -> PathBuf {
    let dir = runtime_dir();
    if dir == std::path::Path::new("/tmp") {
        dir.join(format!("{name}-{}", user_suffix()))
    } else {
        dir.join(name)
    }
}

/// Unix socket the daemon listens on. `GROK_RPC_SOCKET` overrides it (tests, dev).
pub fn socket_path() -> PathBuf {
    env::var_os("GROK_RPC_SOCKET")
        .map(PathBuf::from)
        .unwrap_or_else(|| runtime_file("grok-rpc.sock"))
}

/// Lock file guaranteeing a single daemon.
pub fn lock_path() -> PathBuf {
    let mut p = socket_path().into_os_string();
    p.push(".lock");
    PathBuf::from(p)
}

/// `$XDG_STATE_HOME/grok-rpc` (logs).
pub fn state_dir() -> PathBuf {
    env::var_os("XDG_STATE_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| home().join(".local/state"))
        .join("grok-rpc")
}

pub fn log_path() -> PathBuf {
    state_dir().join("daemon.log")
}

/// Empty file present means `pause` is on. Survives a daemon restart.
pub fn pause_path() -> PathBuf {
    state_dir().join("paused")
}

/// Grok Build's home (`$GROK_HOME`, else `~/.grok`). Read-only for us.
pub fn grok_home() -> PathBuf {
    env::var_os("GROK_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| home().join(".grok"))
}
