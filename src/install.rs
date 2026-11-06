//! `install` / `uninstall`: the global Grok hook, and an optional systemd user unit.
//!
//! The hook file is the one exception to "never write under `~/.grok`". The user
//! runs the command. The old project hook is parked as `.grok/hooks/dev.json.off`.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

/// Events the design subscribes to. `PostToolUse` is omitted so a tool call
/// starts one process, not two.
pub const HOOK_EVENTS: &[&str] = &[
    "SessionStart",
    "SessionEnd",
    "UserPromptSubmit",
    "PreToolUse",
    "Stop",
    "StopFailure",
    "StopCancelled",
    "Notification",
    "SubagentStart",
    "SubagentStop",
    "PreCompact",
    "PostCompact",
];

pub fn hook_json(command: &str) -> String {
    let group = serde_json::json!([{
        "hooks": [{
            "type": "command",
            "command": command,
            "args": ["hook"],
            "timeout": 2
        }]
    }]);
    let mut events = serde_json::Map::new();
    for name in HOOK_EVENTS {
        events.insert((*name).to_owned(), group.clone());
    }
    let mut root = serde_json::Map::new();
    root.insert("hooks".into(), serde_json::Value::Object(events));
    serde_json::to_string_pretty(&serde_json::Value::Object(root)).unwrap() + "\n"
}

/// `Ok(false)` when the file already holds exactly this hook (nothing written),
/// so `install --systemd` works on a machine that already has the hook.
pub fn write_hook(path: &Path, command: &str, force: bool) -> Result<bool, String> {
    let text = hook_json(command);
    if fs::read_to_string(path).is_ok_and(|old| old == text) {
        return Ok(false);
    }
    if path.exists() && !force {
        return Err(format!(
            "{} already exists (pass --force to overwrite)",
            path.display()
        ));
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("{}: {e}", parent.display()))?;
    }
    fs::write(path, text)
        .map(|()| true)
        .map_err(|e| format!("{}: {e}", path.display()))
}

pub fn systemd_unit_path() -> PathBuf {
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(std::env::var_os("HOME").unwrap_or_else(|| "/tmp".into())).join(".config")
        });
    base.join("systemd/user/grok-rpc.service")
}

pub fn unit_text(exe: &str) -> String {
    let exe = systemd_quote(exe);
    format!(
        "\
[Unit]
Description=Grok Build Discord Rich Presence
After=default.target

[Service]
Type=simple
ExecStart={exe} daemon --no-idle-exit
Restart=on-failure
RestartSec=3

[Install]
WantedBy=default.target
"
    )
}

pub fn write_unit(path: &Path, exe: &str) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("{}: {e}", parent.display()))?;
    }
    fs::write(path, unit_text(exe)).map_err(|e| format!("{}: {e}", path.display()))
}

/// `Ok(false)` when the path was already absent.
pub fn remove_path(path: &Path) -> Result<bool, String> {
    match fs::remove_file(path) {
        Ok(()) => Ok(true),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(false),
        Err(e) => Err(format!("{}: {e}", path.display())),
    }
}

fn systemd_quote(exe: &str) -> String {
    if exe.contains([' ', '\t', '"', '\\']) {
        format!("\"{}\"", exe.replace('\\', "\\\\").replace('"', "\\\""))
    } else {
        exe.to_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    #[test]
    fn hook_json_covers_every_event_without_a_matcher() {
        let v: Value = serde_json::from_str(&hook_json("/usr/local/bin/grok-rpc")).unwrap();
        let hooks = v["hooks"].as_object().unwrap();
        assert_eq!(hooks.len(), HOOK_EVENTS.len());
        for name in HOOK_EVENTS {
            let group = &hooks[*name][0];
            assert!(group.get("matcher").is_none(), "{name}");
            let handler = &group["hooks"][0];
            assert_eq!(handler["type"], "command");
            assert_eq!(handler["command"], "/usr/local/bin/grok-rpc");
            assert_eq!(handler["args"][0], "hook");
            assert_eq!(handler["timeout"], 2);
        }
    }

    #[test]
    fn write_hook_refuses_to_overwrite_unless_forced() {
        let dir = std::env::temp_dir().join(format!("grok-rpc-install-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let path = dir.join("hooks/grok-rpc.json");
        assert!(write_hook(&path, "/bin/grok-rpc", false).unwrap());
        assert!(
            !write_hook(&path, "/bin/grok-rpc", false).unwrap(),
            "same hook again is a no-op, not an error"
        );
        assert!(write_hook(&path, "/bin/other", false).is_err());
        assert!(write_hook(&path, "/bin/other", true).unwrap());
        let text = fs::read_to_string(&path).unwrap();
        assert!(text.contains("/bin/other"));
        assert!(remove_path(&path).unwrap());
        assert!(!remove_path(&path).unwrap());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn unit_quotes_a_path_with_spaces() {
        let text = unit_text("/home/me/My Tools/grok-rpc");
        assert!(text.contains("ExecStart=\"/home/me/My Tools/grok-rpc\" daemon --no-idle-exit\n"));
        assert!(text.contains("Restart=on-failure"));
    }
}
