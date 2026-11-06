//! Minimal stderr logger. Debug output is enabled with `GROK_RPC_LOG=debug` (or `RUST_LOG=debug`).

use std::sync::OnceLock;
use std::time::{SystemTime, UNIX_EPOCH};

pub fn debug_enabled() -> bool {
    static ON: OnceLock<bool> = OnceLock::new();
    *ON.get_or_init(|| {
        ["GROK_RPC_LOG", "RUST_LOG"]
            .iter()
            .any(|k| std::env::var(k).is_ok_and(|v| v.contains("debug") || v.contains("trace")))
    })
}

/// `HH:MM:SS` (UTC).
pub fn ts() -> String {
    let s = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    format!("{:02}:{:02}:{:02}", (s / 3600) % 24, (s / 60) % 60, s % 60)
}

#[macro_export]
macro_rules! info {
    ($($t:tt)*) => { eprintln!("{} INFO  {}", $crate::log::ts(), format!($($t)*)) };
}

#[macro_export]
macro_rules! warn {
    ($($t:tt)*) => { eprintln!("{} WARN  {}", $crate::log::ts(), format!($($t)*)) };
}

#[macro_export]
macro_rules! debug {
    ($($t:tt)*) => {
        if $crate::log::debug_enabled() {
            eprintln!("{} DEBUG {}", $crate::log::ts(), format!($($t)*))
        }
    };
}
