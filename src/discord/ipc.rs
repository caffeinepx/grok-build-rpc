//! Discord local RPC over IPC (unix socket `discord-ipc-N`).
//!
//! Frame = little-endian `u32 opcode` + `u32 length` + JSON body.
//! Opcodes: 0 HANDSHAKE, 1 FRAME, 2 CLOSE, 3 PING, 4 PONG.

use serde_json::{Value, json};
use std::io::{self, Read, Write};
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::time::Duration;

pub const OP_HANDSHAKE: u32 = 0;
pub const OP_FRAME: u32 = 1;
pub const OP_CLOSE: u32 = 2;
pub const OP_PING: u32 = 3;
pub const OP_PONG: u32 = 4;

const MAX_FRAME: u32 = 1 << 20;

pub fn encode_frame(op: u32, body: &Value) -> Vec<u8> {
    let payload = serde_json::to_vec(body).expect("serializing a Value cannot fail");
    let mut out = Vec::with_capacity(8 + payload.len());
    out.extend_from_slice(&op.to_le_bytes());
    out.extend_from_slice(&(payload.len() as u32).to_le_bytes());
    out.extend_from_slice(&payload);
    out
}

pub fn read_frame<R: Read>(r: &mut R) -> io::Result<(u32, Value)> {
    let mut header = [0u8; 8];
    r.read_exact(&mut header)?;
    let op = u32::from_le_bytes(header[0..4].try_into().unwrap());
    let len = u32::from_le_bytes(header[4..8].try_into().unwrap());
    if len > MAX_FRAME {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("frame too large: {len}"),
        ));
    }
    let mut body = vec![0u8; len as usize];
    r.read_exact(&mut body)?;
    let value = if body.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&body)?
    };
    Ok((op, value))
}

/// Candidate socket paths, covering native, Flatpak, Snap and Vesktop installs.
pub fn candidate_paths() -> Vec<PathBuf> {
    let mut bases: Vec<PathBuf> = Vec::new();
    for var in ["XDG_RUNTIME_DIR", "TMPDIR", "TMP", "TEMP"] {
        if let Some(v) = std::env::var_os(var) {
            bases.push(PathBuf::from(v));
        }
    }
    bases.push(PathBuf::from("/tmp"));
    bases.dedup();

    const SUBDIRS: [&str; 6] = [
        "",
        "app/com.discordapp.Discord",
        "app/com.discordapp.DiscordCanary",
        "snap.discord",
        "snap.discord-canary",
        ".flatpak/dev.vencord.Vesktop/xdg-run",
    ];
    let mut out = Vec::new();
    for base in &bases {
        for sub in SUBDIRS {
            for i in 0..10 {
                out.push(base.join(sub).join(format!("discord-ipc-{i}")));
            }
        }
    }
    out
}

pub struct Ipc {
    client_id: String,
    stream: Option<UnixStream>,
    nonce: u64,
}

impl Ipc {
    pub fn new(client_id: impl Into<String>) -> Self {
        Self {
            client_id: client_id.into(),
            stream: None,
            nonce: 0,
        }
    }

    pub fn client_id(&self) -> &str {
        &self.client_id
    }

    pub fn is_connected(&self) -> bool {
        self.stream.is_some()
    }

    /// Try every known socket path; keep the first one that completes the handshake.
    pub fn connect(&mut self) -> io::Result<PathBuf> {
        let mut last_err = io::Error::new(io::ErrorKind::NotFound, "no Discord IPC socket found");
        for path in candidate_paths() {
            if !path.exists() {
                continue;
            }
            match self.connect_to(&path) {
                Ok(()) => return Ok(path),
                Err(e) => last_err = e,
            }
        }
        Err(last_err)
    }

    pub fn connect_to(&mut self, path: &Path) -> io::Result<()> {
        let mut s = UnixStream::connect(path)?;
        s.set_read_timeout(Some(Duration::from_secs(5)))?;
        s.set_write_timeout(Some(Duration::from_secs(5)))?;
        s.write_all(&encode_frame(
            OP_HANDSHAKE,
            &json!({ "v": 1, "client_id": self.client_id }),
        ))?;
        let (op, body) = read_frame(&mut s)?;
        if op == OP_CLOSE || body.get("evt").and_then(Value::as_str) != Some("READY") {
            return Err(io::Error::new(
                io::ErrorKind::ConnectionRefused,
                format!("handshake rejected (op {op}): {body}"),
            ));
        }
        self.stream = Some(s);
        Ok(())
    }

    /// Set (Some) or clear (None) the activity. An IO error drops the connection;
    /// a Discord-level ERROR reply is returned as `Err` but keeps the connection.
    pub fn set_activity(&mut self, activity: Option<&Value>) -> io::Result<()> {
        self.nonce += 1;
        let nonce = format!("grok-rpc-{}", self.nonce);
        let payload = json!({
            "cmd": "SET_ACTIVITY",
            "args": { "pid": std::process::id(), "activity": activity },
            "nonce": nonce,
        });
        let result = self.request(&payload, &nonce);
        if let Err(e) = &result
            && e.kind() != io::ErrorKind::Other
        {
            self.stream = None;
        }
        result
    }

    fn request(&mut self, payload: &Value, nonce: &str) -> io::Result<()> {
        let s = self
            .stream
            .as_mut()
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotConnected, "not connected"))?;
        s.write_all(&encode_frame(OP_FRAME, payload))?;
        // Read until the reply carrying our nonce (skip unrelated dispatches, answer pings).
        for _ in 0..8 {
            let (op, body) = read_frame(s)?;
            match op {
                OP_PING => s.write_all(&encode_frame(OP_PONG, &body))?,
                OP_CLOSE => {
                    return Err(io::Error::new(
                        io::ErrorKind::ConnectionAborted,
                        format!("closed: {body}"),
                    ));
                }
                _ if body.get("nonce").and_then(Value::as_str) == Some(nonce) => {
                    if body.get("evt").and_then(Value::as_str) == Some("ERROR") {
                        return Err(io::Error::other(format!("Discord error: {}", body["data"])));
                    }
                    return Ok(());
                }
                _ => {}
            }
        }
        Err(io::Error::new(
            io::ErrorKind::TimedOut,
            "no reply for SET_ACTIVITY",
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::net::UnixListener;

    #[test]
    fn frame_roundtrip() {
        let v = json!({"a": 1});
        let bytes = encode_frame(OP_FRAME, &v);
        assert_eq!(&bytes[0..4], &1u32.to_le_bytes());
        let (op, back) = read_frame(&mut &bytes[..]).unwrap();
        assert_eq!((op, back), (OP_FRAME, v));
    }

    #[test]
    fn handshake_and_set_activity_against_mock_discord() {
        let dir = std::env::temp_dir().join(format!("grok-rpc-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("discord-ipc-0");
        let _ = std::fs::remove_file(&path);
        let listener = UnixListener::bind(&path).unwrap();

        let server = std::thread::spawn(move || {
            let (mut s, _) = listener.accept().unwrap();
            let (op, hs) = read_frame(&mut s).unwrap();
            assert_eq!(op, OP_HANDSHAKE);
            assert_eq!(hs["client_id"], "123");
            s.write_all(&encode_frame(
                OP_FRAME,
                &json!({"cmd": "DISPATCH", "evt": "READY"}),
            ))
            .unwrap();
            let (_, req) = read_frame(&mut s).unwrap();
            assert_eq!(req["cmd"], "SET_ACTIVITY");
            let state = req["args"]["activity"]["state"].clone();
            s.write_all(&encode_frame(
                OP_FRAME,
                &json!({"cmd": "SET_ACTIVITY", "nonce": req["nonce"]}),
            ))
            .unwrap();
            state
        });

        let mut ipc = Ipc::new("123");
        ipc.connect_to(&path).unwrap();
        ipc.set_activity(Some(&json!({"state": "Thinking…"})))
            .unwrap();
        assert_eq!(server.join().unwrap(), json!("Thinking…"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
