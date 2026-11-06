//! Drop sessions Grok is no longer running.
//!
//! `~/.grok/active_sessions.json` is a JSON array. Verified row:
//! `{session_id, pid, pid_start, cwd, opened_at}`.
//! `session_id` matches the hook `sessionId`. `pid_start` is field 22 of
//! `/proc/<pid>/stat` (starttime, clock ticks since boot), so a recycled pid
//! does not keep a dead session on the profile.

use super::state::State;
use serde::Deserialize;
use std::collections::HashSet;
use std::fs;
use std::io;
use std::path::Path;

/// How often the daemon re-reads `active_sessions.json`.
pub const INTERVAL_MS: u64 = 30_000;
/// Keep a session that is not listed, counted from its last hook event. Grok writes
/// the file just after SessionStart, and a session that keeps sending events is
/// obviously running even if it never shows up in the file.
pub const GRACE_MS: u64 = 30_000;

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct ActiveSession {
    pub session_id: String,
    #[serde(default)]
    pub pid: u32,
    #[serde(default)]
    pub pid_start: u64,
    #[serde(default)]
    pub cwd: String,
    #[serde(default)]
    pub opened_at: String,
}

pub fn load(path: &Path) -> io::Result<Vec<ActiveSession>> {
    parse(&fs::read_to_string(path)?)
}

pub fn parse(text: &str) -> io::Result<Vec<ActiveSession>> {
    let mut rows: Vec<ActiveSession> =
        serde_json::from_str(text).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    rows.retain(|r| !r.session_id.is_empty());
    Ok(rows)
}

/// `(listed ids, ids whose pid is still that process)`.
pub fn listed_and_alive(rows: &[ActiveSession]) -> (HashSet<String>, HashSet<String>) {
    let mut listed = HashSet::new();
    let mut alive = HashSet::new();
    for row in rows {
        listed.insert(row.session_id.clone());
        if process_matches(row.pid, row.pid_start) {
            alive.insert(row.session_id.clone());
        }
    }
    (listed, alive)
}

/// Remove sessions whose row disappeared, or whose pid died or was reused.
/// An id missing from `listed` is kept until `GRACE_MS` after its last event.
/// An id in `listed` but not `alive` is removed immediately. Alive ids get
/// `alive_ms` refreshed so the stale backstop in `State::tick` leaves them alone.
pub fn reconcile(
    state: &mut State,
    listed: &HashSet<String>,
    alive: &HashSet<String>,
    now_ms: u64,
) {
    state.sessions.retain(|id, session| {
        if alive.contains(id) {
            session.alive_ms = Some(now_ms);
            return true;
        }
        if listed.contains(id) {
            return false;
        }
        now_ms.saturating_sub(session.last_event_ms) < GRACE_MS
    });
}

/// `pid_start == 0` skips the start-time check (the field was absent).
pub fn process_matches(pid: u32, pid_start: u64) -> bool {
    match start_ticks(pid) {
        Some(ticks) => pid_start == 0 || ticks == pid_start,
        None => false,
    }
}

/// Field 22 of `/proc/<pid>/stat`. `None` when the process does not exist.
pub fn start_ticks(pid: u32) -> Option<u64> {
    if pid == 0 {
        return None;
    }
    let stat = fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    // `comm` is wrapped in parentheses and may itself contain ')'.
    let rest = stat.rsplit_once(')')?.1.trim_start();
    rest.split_whitespace().nth(19)?.parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn session(st: &mut State, id: &str, now: u64) {
        st.apply(
            &json!({"hook_event_name": "SessionStart", "sessionId": id, "cwd": "/p/proj"}),
            now,
        );
    }

    #[test]
    fn parses_verified_row_and_ignores_future_fields() {
        let rows = parse(
            r#"[{
                "session_id": "01a19aab-a4c8-7561-b5ce-8ca30259636f",
                "pid": 440498,
                "pid_start": 7001040,
                "cwd": "/home/latte/Documents/grok-build-rpc",
                "opened_at": "2026-11-02T05:11:43.201546595Z",
                "future_field": true
            }]"#,
        )
        .unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].session_id, "01a19aab-a4c8-7561-b5ce-8ca30259636f");
        assert_eq!(rows[0].pid, 440498);
        assert_eq!(rows[0].pid_start, 7001040);
        assert_eq!(rows[0].cwd, "/home/latte/Documents/grok-build-rpc");
    }

    #[test]
    fn empty_file_and_blank_id() {
        assert!(parse("[]").unwrap().is_empty());
        assert!(
            parse(r#"[{"session_id": "", "pid": 1}]"#)
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn rejects_a_non_array() {
        assert!(parse(r#"{"session_id": "x"}"#).is_err());
    }

    #[test]
    fn dead_pid_is_listed_but_not_alive() {
        let rows = parse(r#"[{"session_id": "s", "pid": 4294967295, "pid_start": 1}]"#).unwrap();
        let (listed, alive) = listed_and_alive(&rows);
        assert!(listed.contains("s"));
        assert!(alive.is_empty());
    }

    #[test]
    fn reconcile_grace_dead_and_live() {
        let mut st = State::new("/nonexistent-grok-home");
        session(&mut st, "live", 1_000);
        session(&mut st, "dead", 1_000);
        session(&mut st, "new", 1_000);
        let listed = HashSet::from(["live".into(), "dead".into()]);
        let alive = HashSet::from(["live".into()]);

        reconcile(&mut st, &listed, &alive, 1_000);
        assert!(st.sessions.contains_key("live"));
        assert!(
            !st.sessions.contains_key("dead"),
            "a listed dead pid drops immediately"
        );
        assert!(
            st.sessions.contains_key("new"),
            "unlisted session stays during grace"
        );

        reconcile(&mut st, &listed, &alive, 1_000 + GRACE_MS);
        assert!(st.sessions.contains_key("live"));
        assert!(!st.sessions.contains_key("new"));
    }

    #[test]
    fn unlisted_session_that_keeps_sending_events_stays() {
        let mut st = State::new("/nonexistent-grok-home");
        session(&mut st, "busy", 1_000);
        let none = HashSet::new();
        let t = 1_000 + GRACE_MS - 1;
        st.apply(
            &json!({"hook_event_name": "UserPromptSubmit", "sessionId": "busy", "cwd": "/p/proj"}),
            t,
        );
        reconcile(&mut st, &none, &none, t + GRACE_MS - 1);
        assert!(
            st.sessions.contains_key("busy"),
            "grace counts from the last event"
        );
        reconcile(&mut st, &none, &none, t + GRACE_MS);
        assert!(!st.sessions.contains_key("busy"));
    }

    #[test]
    fn idle_but_alive_session_outlives_the_stale_backstop() {
        use crate::daemon::state::STALE_SESSION_MS;
        let mut st = State::new("/nonexistent-grok-home");
        session(&mut st, "idle", 1_000);
        session(&mut st, "gone", 1_000);
        let ids = HashSet::from(["idle".into()]);
        // No hook events for an hour, but Grok keeps listing the process as alive.
        let mut now = 1_000;
        while now < 1_000 + 2 * STALE_SESSION_MS {
            now += INTERVAL_MS;
            reconcile(&mut st, &ids, &ids, now);
            st.tick(now);
        }
        assert!(st.sessions.contains_key("idle"));
        assert!(!st.sessions.contains_key("gone"));
    }

    #[test]
    fn own_process_start_ticks_roundtrip() {
        let pid = std::process::id();
        let ticks = start_ticks(pid).expect("/proc/self/stat");
        assert!(process_matches(pid, ticks));
        assert!(!process_matches(pid, ticks.wrapping_add(1)));
        assert!(
            process_matches(pid, 0),
            "missing pid_start only checks that the pid exists"
        );
        assert!(!process_matches(u32::MAX, ticks));
    }
}
