//! Long-running daemon: receives hook events on a unix socket, keeps session state,
//! and mirrors the primary session to Discord.

pub mod enrich;
pub mod liveness;
pub mod state;

use crate::config::{self, Compiled};
use crate::discord::ipc::Ipc;
use crate::{debug, info, paths, presence, warn};
use serde_json::Value;
use state::State;
use std::fs::{self, File};
use std::io::{self, Read, Write};
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, RecvTimeoutError, Sender};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// Minimum gap between SET_ACTIVITY calls (Discord rate-limits presence updates).
const MIN_SEND_GAP_MS: u64 = 2_000;
/// Exit when no session has existed for this long; the next hook restarts us.
const IDLE_EXIT_MS: u64 = 10 * 60_000;
const TICK: Duration = Duration::from_millis(500);

pub enum Msg {
    Event(Value),
    Status(Sender<String>),
    SetPaused(bool, Sender<String>),
}

/// Persists pause across daemon restarts. An empty file means paused.
pub fn set_paused_flag(on: bool) -> io::Result<()> {
    let path = paths::pause_path();
    if on {
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir)?;
        }
        File::create(path).map(|_| ())
    } else if path.exists() {
        fs::remove_file(path)
    } else {
        Ok(())
    }
}

pub fn paused_on_disk() -> bool {
    paths::pause_path().is_file()
}

/// Watches the config file (by mtime/size) and reloads it; a broken edit keeps the last good config.
struct ConfigWatch {
    path: PathBuf,
    stamp: Option<(SystemTime, u64)>,
    current: Compiled,
    error: Option<String>,
}

fn file_stamp(path: &Path) -> Option<(SystemTime, u64)> {
    let m = fs::metadata(path).ok()?;
    Some((m.modified().ok()?, m.len()))
}

impl ConfigWatch {
    fn new(path: PathBuf) -> Self {
        let mut w = ConfigWatch {
            stamp: file_stamp(&path),
            path,
            current: Compiled::defaults(),
            error: None,
        };
        w.load();
        w
    }

    fn load(&mut self) {
        match config::load(&self.path) {
            Ok(c) => {
                info!("config loaded from {}", self.path.display());
                self.current = c;
                self.error = None;
            }
            Err(e) => {
                warn!("config error (keeping previous config): {e}");
                self.error = Some(e);
            }
        }
    }

    /// Returns true if a changed config was loaded successfully.
    fn poll(&mut self) -> bool {
        let stamp = file_stamp(&self.path);
        if stamp == self.stamp {
            return false;
        }
        self.stamp = stamp;
        self.load();
        self.error.is_none()
    }

    fn client_id(&self) -> String {
        std::env::var("GROK_RPC_CLIENT_ID").unwrap_or_else(|_| self.current.cfg.client_id.clone())
    }
}

pub fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// `idle_exit`: quit after `IDLE_EXIT_MS` with no sessions (autospawned daemons).
/// The systemd unit passes `--no-idle-exit`: a clean exit would not be restarted.
pub fn run(idle_exit: bool) -> io::Result<()> {
    let sock = paths::socket_path();
    if let Some(dir) = sock.parent() {
        fs::create_dir_all(dir)?;
    }
    let lock = File::create(paths::lock_path())?;
    if lock.try_lock().is_err() {
        info!("another grok-rpc daemon is already running; exiting");
        return Ok(());
    }
    let _ = fs::remove_file(&sock); // stale socket from a crashed daemon (we hold the lock)
    let listener = UnixListener::bind(&sock)?;
    fs::set_permissions(&sock, fs::Permissions::from_mode(0o600))?;
    info!("listening on {}", sock.display());

    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || serve(listener, tx));

    let mut cfg = ConfigWatch::new(config::config_path());
    let grok_home = paths::grok_home();
    let active_sessions = grok_home.join("active_sessions.json");
    let grok_version = enrich::grok_version(&grok_home);
    let mut state = State::new(grok_home);
    let mut ipc = Ipc::new(cfg.client_id());
    let mut sent: Option<Option<Value>> = None; // what Discord currently shows (None = unknown)
    let mut last_send_ms = 0u64;
    let mut next_connect_ms = 0u64;
    let mut backoff_ms = 5_000u64;
    let mut empty_since: Option<u64> = Some(now_ms());
    let mut paused = paused_on_disk();
    let mut last_liveness_ms = now_ms();
    if paused {
        info!("presence is paused");
    }

    loop {
        let first = rx.recv_timeout(TICK);
        let mut msgs = Vec::new();
        match first {
            Ok(m) => msgs.push(m),
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => break,
        }
        msgs.extend(rx.try_iter());

        let now = now_ms();
        for m in msgs {
            match m {
                Msg::Event(ev) => {
                    debug!(
                        "event {} session {}",
                        ev["hook_event_name"], ev["sessionId"]
                    );
                    state.apply(&ev, now);
                }
                Msg::Status(reply) => {
                    let desired = desired_presence(
                        &state,
                        &cfg.current,
                        grok_version.as_deref(),
                        now,
                        paused,
                    );
                    let _ = reply.send(status_text(
                        &state,
                        &cfg,
                        ipc.is_connected(),
                        paused,
                        desired.as_ref(),
                    ));
                }
                Msg::SetPaused(on, reply) => {
                    paused = on;
                    if let Err(e) = set_paused_flag(on) {
                        warn!("could not save pause flag: {e}");
                    }
                    sent = None;
                    info!("presence {}", if on { "paused" } else { "resumed" });
                    let _ = reply.send("ok\n".into());
                }
            }
        }
        state.tick(now);

        if now.saturating_sub(last_liveness_ms) >= liveness::INTERVAL_MS {
            last_liveness_ms = now;
            match liveness::load(&active_sessions) {
                Ok(rows) => {
                    let before = state.sessions.len();
                    let (listed, alive) = liveness::listed_and_alive(&rows);
                    liveness::reconcile(&mut state, &listed, &alive, now);
                    let dropped = before - state.sessions.len();
                    if dropped > 0 {
                        info!("liveness: dropped {dropped} session(s)");
                    }
                }
                Err(e) if e.kind() == io::ErrorKind::NotFound => {}
                Err(e) => warn!("active_sessions.json: {e}"),
            }
        }

        if cfg.poll() {
            let id = cfg.client_id();
            if ipc.client_id() != id {
                info!("client_id changed; reconnecting to Discord");
                if ipc.is_connected() {
                    let _ = ipc.set_activity(None);
                }
                ipc = Ipc::new(id);
                next_connect_ms = 0;
            }
            sent = None; // force a re-send with the new templates
        }

        if state.is_empty() && idle_exit {
            let since = *empty_since.get_or_insert(now);
            if now.saturating_sub(since) >= IDLE_EXIT_MS {
                info!(
                    "no Grok sessions for {} min; exiting",
                    IDLE_EXIT_MS / 60_000
                );
                break;
            }
        } else {
            empty_since = None;
        }

        if !ipc.is_connected() && now >= next_connect_ms {
            match ipc.connect() {
                Ok(path) => {
                    info!("connected to Discord at {}", path.display());
                    sent = None;
                    backoff_ms = 5_000;
                }
                Err(e) => {
                    debug!(
                        "Discord not available ({e}); retrying in {}s",
                        backoff_ms / 1000
                    );
                    next_connect_ms = now + backoff_ms;
                    backoff_ms = (backoff_ms * 2).min(60_000);
                }
            }
        }

        let desired = desired_presence(&state, &cfg.current, grok_version.as_deref(), now, paused);
        if ipc.is_connected()
            && sent.as_ref() != Some(&desired)
            && now.saturating_sub(last_send_ms) >= MIN_SEND_GAP_MS
        {
            last_send_ms = now;
            match ipc.set_activity(desired.as_ref()) {
                Ok(()) => {
                    debug!(
                        "presence → {}",
                        desired.as_ref().map_or("cleared".into(), Value::to_string)
                    );
                    sent = Some(desired);
                }
                Err(e) => {
                    warn!("SET_ACTIVITY failed: {e}");
                    if !ipc.is_connected() {
                        next_connect_ms = now + backoff_ms;
                    }
                }
            }
        }
    }

    if ipc.is_connected() {
        let _ = ipc.set_activity(None);
    }
    let _ = fs::remove_file(&sock);
    Ok(())
}

/// Socket server: each connection carries one hook event (raw JSON) or a `STATUS` request.
fn serve(listener: UnixListener, tx: Sender<Msg>) {
    for conn in listener.incoming() {
        let Ok(mut conn) = conn else { continue };
        if let Err(e) = handle_conn(&mut conn, &tx) {
            debug!("connection error: {e}");
        }
    }
}

fn handle_conn(conn: &mut UnixStream, tx: &Sender<Msg>) -> io::Result<()> {
    conn.set_read_timeout(Some(Duration::from_secs(1)))?;
    let mut buf = Vec::new();
    conn.take(16 << 20).read_to_end(&mut buf)?;
    match control_word(&buf) {
        Some("STATUS") => {
            let (rtx, rrx) = mpsc::channel();
            tx.send(Msg::Status(rtx)).ok();
            let text = rrx
                .recv_timeout(Duration::from_secs(3))
                .unwrap_or_else(|_| "daemon busy".into());
            conn.write_all(text.as_bytes())?;
            return Ok(());
        }
        Some(word @ ("PAUSE" | "RESUME")) => {
            let (rtx, rrx) = mpsc::channel();
            tx.send(Msg::SetPaused(word == "PAUSE", rtx)).ok();
            let text = rrx
                .recv_timeout(Duration::from_secs(3))
                .unwrap_or_else(|_| "daemon busy\n".into());
            conn.write_all(text.as_bytes())?;
            return Ok(());
        }
        _ => {}
    }
    match serde_json::from_slice::<Value>(&buf) {
        Ok(v) => {
            tx.send(Msg::Event(v)).ok();
        }
        Err(e) => warn!("ignoring malformed event ({} bytes): {e}", buf.len()),
    }
    Ok(())
}

fn control_word(buf: &[u8]) -> Option<&'static str> {
    let Ok(text) = std::str::from_utf8(buf) else {
        return None;
    };
    match text.trim() {
        "STATUS" => Some("STATUS"),
        "PAUSE" => Some("PAUSE"),
        "RESUME" => Some("RESUME"),
        _ => None,
    }
}

fn desired_presence(
    state: &State,
    cfg: &Compiled,
    grok_version: Option<&str>,
    now: u64,
    paused: bool,
) -> Option<Value> {
    if paused {
        return None;
    }
    let ctx = presence::Context {
        grok_version,
        now_ms: now,
    };
    presence::render(state, cfg, &ctx)
}

fn status_text(
    state: &State,
    cfg: &ConfigWatch,
    connected: bool,
    paused: bool,
    desired: Option<&Value>,
) -> String {
    let now = now_ms();
    let config_state = match (&cfg.error, cfg.path.exists()) {
        (Some(e), _) => format!("ERROR, using previous config: {e}"),
        (None, true) => "ok".into(),
        (None, false) => "not found, using defaults".into(),
    };
    let mut out = format!(
        "daemon:   running (pid {})\ndiscord:  {} (client_id {})\npaused:   {}\nconfig:   {} ({})\npreset:   {}\nsessions: {}\n",
        std::process::id(),
        if connected {
            "connected"
        } else {
            "not connected"
        },
        cfg.client_id(),
        if paused { "yes" } else { "no" },
        cfg.path.display(),
        config_state,
        cfg.current.cfg.preset,
        state.sessions.len()
    );
    let mut sessions: Vec<_> = state.sessions.values().collect();
    sessions.sort_by_key(|s| std::cmp::Reverse(s.last_event_ms));
    for s in sessions {
        out.push_str(&format!(
            "  - {} [{}] {} · model {} · {} edits, {} cmds · last event {}s ago\n",
            s.project_name(),
            &s.id[..s.id.len().min(8)],
            s.activity.label(),
            s.summary.model.as_deref().unwrap_or("?"),
            s.edits,
            s.commands,
            now.saturating_sub(s.last_event_ms) / 1000
        ));
    }
    out.push_str("presence: ");
    let presence = if paused {
        "(paused)".into()
    } else {
        desired.map_or("(cleared)".into(), |v| {
            serde_json::to_string_pretty(v).unwrap_or_default()
        })
    };
    out.push_str(&presence);
    out.push('\n');
    out
}
