//! `grok-rpc hook`: invoked by Grok Build for every hook event.
//!
//! Contract (Grok runs hooks in the foreground):
//!   * finish in a few milliseconds,
//!   * never write to stdout (Grok parses it),
//!   * always exit 0, whatever happens.

use crate::paths;
use std::fs::{self, OpenOptions};
use std::io::{self, Read, Write};
use std::net::Shutdown;
use std::os::unix::net::UnixStream;
use std::os::unix::process::CommandExt;
use std::process::{Command, Stdio};
use std::time::Duration;

const MAX_EVENT_BYTES: u64 = 8 << 20;

pub fn run() {
    // Silence panic output and swallow panics: the caller exits 0 regardless.
    std::panic::set_hook(Box::new(|_| {}));
    let _ = std::panic::catch_unwind(|| {
        let _ = forward();
    });
}

fn forward() -> io::Result<()> {
    let mut buf = Vec::new();
    io::stdin()
        .lock()
        .take(MAX_EVENT_BYTES)
        .read_to_end(&mut buf)?;
    if buf.iter().all(u8::is_ascii_whitespace) {
        return Ok(());
    }
    let mut stream = connect_or_spawn()?;
    stream.set_write_timeout(Some(Duration::from_millis(500)))?;
    stream.write_all(&buf)?;
    stream.shutdown(Shutdown::Write)?;
    Ok(())
}

fn connect_or_spawn() -> io::Result<UnixStream> {
    let sock = paths::socket_path();
    if let Ok(s) = UnixStream::connect(&sock) {
        return Ok(s);
    }
    if std::env::var_os("GROK_RPC_NO_SPAWN").is_some() {
        return Err(io::Error::new(
            io::ErrorKind::NotConnected,
            "daemon not running",
        ));
    }
    spawn_daemon()?;
    // Wait briefly for the new daemon to bind (≤ ~400 ms, well under the hook timeout).
    let mut last = io::Error::new(io::ErrorKind::NotConnected, "daemon did not start");
    for _ in 0..20 {
        std::thread::sleep(Duration::from_millis(20));
        match UnixStream::connect(&sock) {
            Ok(s) => return Ok(s),
            Err(e) => last = e,
        }
    }
    Err(last)
}

/// Starts `grok-rpc daemon` fully detached: own process group, no inherited pipes
/// (Grok may wait for the hook's stdout/stderr to close), stderr → log file.
pub fn spawn_daemon() -> io::Result<()> {
    let exe = std::env::current_exe()?;
    let log = paths::log_path();
    if let Some(dir) = log.parent() {
        fs::create_dir_all(dir)?;
    }
    let log_file = OpenOptions::new().create(true).append(true).open(&log)?;
    Command::new(exe)
        .arg("daemon")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::from(log_file))
        .process_group(0)
        .spawn()?;
    Ok(())
}
