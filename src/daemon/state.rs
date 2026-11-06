//! Per-session state machine driven by Grok Build hook events.

use super::enrich;
use serde_json::Value;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// How long a StopFailure error stays visible before falling back to Idle.
pub const ERROR_DISPLAY_MS: u64 = 30_000;
/// Backstop for sessions liveness cannot vouch for (file missing or unreadable,
/// or the session is not listed). Counted from the last hook event or the last
/// time `liveness::reconcile` saw the Grok process alive, whichever is later,
/// so a session left idle in an open Grok window is never dropped by this.
pub const STALE_SESSION_MS: u64 = 30 * 60_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolKind {
    Edit,
    Terminal,
    Read,
    Search,
    Web,
    Subagent,
    Plan,
    Question,
    Media,
    Mcp,
    Other,
}

impl ToolKind {
    pub fn label(self) -> &'static str {
        match self {
            ToolKind::Edit => "Editing code",
            ToolKind::Terminal => "Running a command",
            ToolKind::Read => "Reading files",
            ToolKind::Search => "Searching the codebase",
            ToolKind::Web => "Browsing the web",
            ToolKind::Subagent => "Delegating to a subagent",
            ToolKind::Plan => "Planning",
            ToolKind::Question => "Asking a question",
            ToolKind::Media => "Generating media",
            ToolKind::Mcp => "Using a tool",
            ToolKind::Other => "Working",
        }
    }

    pub fn icon(self) -> &'static str {
        match self {
            ToolKind::Edit => "edit",
            ToolKind::Terminal => "terminal",
            ToolKind::Read => "read",
            ToolKind::Search => "search",
            ToolKind::Web => "web",
            ToolKind::Subagent => "subagent",
            ToolKind::Plan | ToolKind::Other | ToolKind::Media => "thinking",
            ToolKind::Question => "approval",
            ToolKind::Mcp => "mcp",
        }
    }
}

/// Maps Grok's tool names to an activity kind.
pub fn classify_tool(name: &str) -> ToolKind {
    match name {
        "search_replace" | "write" | "hashline_edit" | "hashline_write" | "StrReplace"
        | "EditNotebook" => ToolKind::Edit,
        "run_terminal_command"
        | "get_command_or_subagent_output"
        | "kill_command_or_subagent"
        | "monitor"
        | "Shell" => ToolKind::Terminal,
        "read_file" => ToolKind::Read,
        "grep" | "list_dir" | "search_tool" => ToolKind::Search,
        "web_search" | "web_fetch" | "Fetch" => ToolKind::Web,
        "spawn_subagent" | "workflow" | "Task" => ToolKind::Subagent,
        "todo_write" | "enter_plan_mode" | "exit_plan_mode" | "scheduler_create"
        | "scheduler_delete" | "scheduler_list" => ToolKind::Plan,
        "ask_user_question" | "AskQuestion" => ToolKind::Question,
        "image_gen" | "image_edit" | "image_to_video" | "reference_to_video" => ToolKind::Media,
        n if n.starts_with("hashline_") => ToolKind::Edit,
        n if n.contains("__") || n == "use_tool" => ToolKind::Mcp,
        _ => ToolKind::Other,
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Activity {
    Idle,
    Thinking,
    /// `file` / `command` are always recorded; presence decides (privacy) whether to show them.
    Tool {
        kind: ToolKind,
        name: String,
        file: Option<String>,
        command: Option<String>,
    },
    AwaitingApproval,
    Compacting,
    Error {
        kind: String,
        until_ms: u64,
    },
}

impl Activity {
    pub fn label(&self) -> String {
        match self {
            Activity::Idle => "Idle".into(),
            Activity::Thinking => "Thinking…".into(),
            Activity::Tool {
                kind: ToolKind::Mcp,
                name,
                ..
            } => match name.split_once("__") {
                Some((server, _)) => format!("Using {server}"),
                None => ToolKind::Mcp.label().into(),
            },
            Activity::Tool { kind, .. } => kind.label().into(),
            Activity::AwaitingApproval => "Waiting for approval".into(),
            Activity::Compacting => "Compacting context".into(),
            Activity::Error { .. } => self.error_text().unwrap_or_default().into(),
        }
    }

    /// Key used by `[labels]` / `[display.states.<key>]` in the config.
    pub fn key(&self) -> &'static str {
        match self {
            Activity::Idle => "idle",
            Activity::Thinking => "thinking",
            Activity::AwaitingApproval => "approval",
            Activity::Compacting => "compacting",
            Activity::Error { .. } => "error",
            Activity::Tool { kind, .. } => match kind {
                ToolKind::Edit => "edit",
                ToolKind::Terminal => "terminal",
                ToolKind::Read => "read",
                ToolKind::Search => "search",
                ToolKind::Web => "web",
                ToolKind::Subagent => "subagent",
                ToolKind::Plan => "plan",
                ToolKind::Question => "question",
                ToolKind::Media => "media",
                ToolKind::Mcp => "mcp",
                ToolKind::Other => "other",
            },
        }
    }

    /// MCP server name for `server__tool` calls.
    pub fn mcp_server(&self) -> Option<&str> {
        match self {
            Activity::Tool {
                kind: ToolKind::Mcp,
                name,
                ..
            } => name.split_once("__").map(|(s, _)| s),
            _ => None,
        }
    }

    /// Human text for a StopFailure error class.
    pub fn error_text(&self) -> Option<&'static str> {
        match self {
            Activity::Error { kind, .. } => Some(match kind.as_str() {
                "rate_limit" => "Rate limited",
                "authentication_failed" => "Auth error",
                "server_error" => "Server error",
                "invalid_request" => "Request error",
                "max_output_tokens" => "Hit the output limit",
                _ => "Ran into an error",
            }),
            _ => None,
        }
    }

    pub fn icon(&self) -> &'static str {
        match self {
            Activity::Idle => "idle",
            Activity::Thinking => "thinking",
            Activity::Tool { kind, .. } => kind.icon(),
            Activity::AwaitingApproval => "approval",
            Activity::Compacting => "compact",
            Activity::Error { .. } => "error",
        }
    }
}

#[derive(Debug, Clone)]
pub struct Session {
    pub id: String,
    pub workspace: PathBuf,
    pub session_dir: Option<PathBuf>,
    /// Unix seconds; when this session was started/resumed (for Discord's elapsed timer).
    pub started_s: u64,
    pub turn_started_s: Option<u64>,
    pub activity: Activity,
    pub last_event_ms: u64,
    pub subagents: u32,
    pub edits: u32,
    pub commands: u32,
    pub branch: Option<String>,
    pub repo: Option<enrich::Repo>,
    pub summary: enrich::Summary,
    pub usage: enrich::Usage,
    pub permission_mode: Option<String>,
    /// Last time liveness found this session's Grok process alive.
    pub alive_ms: Option<u64>,
}

impl Session {
    fn new(id: &str, workspace: PathBuf, now_ms: u64) -> Self {
        Session {
            id: id.to_owned(),
            workspace,
            session_dir: None,
            started_s: now_ms / 1000,
            turn_started_s: None,
            activity: Activity::Idle,
            last_event_ms: now_ms,
            subagents: 0,
            edits: 0,
            commands: 0,
            branch: None,
            repo: None,
            summary: Default::default(),
            usage: Default::default(),
            permission_mode: None,
            alive_ms: None,
        }
    }

    pub fn project_name(&self) -> String {
        self.workspace
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| self.workspace.display().to_string())
    }

    fn refresh(&mut self) {
        self.branch = enrich::git_branch(&self.workspace);
        self.repo = enrich::git_origin(&self.workspace);
        if let Some(dir) = self.session_dir.as_deref() {
            if let Some(s) = enrich::read_summary(dir) {
                self.summary = s;
            }
            if let Some(u) = enrich::read_usage(dir) {
                self.usage = u;
            }
        }
    }
}

#[derive(Debug)]
pub struct State {
    pub sessions: HashMap<String, Session>,
    grok_home: PathBuf,
}

fn str_field<'a>(ev: &'a Value, keys: &[&str]) -> Option<&'a str> {
    keys.iter()
        .find_map(|k| ev.get(*k).and_then(Value::as_str))
        .filter(|s| !s.is_empty())
}

impl State {
    pub fn new(grok_home: impl Into<PathBuf>) -> Self {
        State {
            sessions: HashMap::new(),
            grok_home: grok_home.into(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.sessions.is_empty()
    }

    /// Most recently active session (presence::visible_sessions applies the idle timeout on top).
    #[cfg(test)]
    pub fn primary(&self) -> Option<&Session> {
        self.sessions.values().max_by_key(|s| s.last_event_ms)
    }

    /// Applies one hook event (the JSON Grok sends on stdin).
    pub fn apply(&mut self, ev: &Value, now_ms: u64) {
        let Some(event) = str_field(ev, &["hook_event_name"])
            .map(str::to_owned)
            .or_else(|| str_field(ev, &["hookEventName"]).map(snake_to_pascal))
        else {
            return;
        };
        let Some(id) = str_field(ev, &["sessionId", "session_id"]).map(str::to_owned) else {
            return;
        };
        let is_subagent = ev.get("subagentType").is_some_and(|v| !v.is_null());

        // Events that must never create a session.
        match event.as_str() {
            "SessionEnd" => {
                if !is_subagent {
                    self.sessions.remove(&id);
                }
                return;
            }
            // A second Stop fires at shutdown *after* SessionEnd (reason channel_closed/shutdown).
            "Stop" if str_field(ev, &["reason"]) != Some("end_turn") => return,
            _ => {}
        }
        if is_subagent && !self.sessions.contains_key(&id) {
            // Child-session events: don't create a phantom top-level session.
            return;
        }

        let workspace = str_field(ev, &["workspaceRoot", "cwd"])
            .map(PathBuf::from)
            .unwrap_or_default();
        let grok_home = self.grok_home.clone();
        let session = self.sessions.entry(id.clone()).or_insert_with(|| {
            let mut s = Session::new(&id, workspace.clone(), now_ms);
            s.refresh();
            s
        });
        session.last_event_ms = now_ms;
        if let Some(mode) = str_field(ev, &["permissionMode", "permission_mode"]) {
            session.permission_mode = Some(mode.to_owned());
        }
        if session.session_dir.is_none() {
            session.session_dir = str_field(ev, &["transcriptPath", "transcript_path"])
                .and_then(|p| Path::new(p).parent().map(Path::to_path_buf))
                .or_else(|| {
                    let cwd = str_field(ev, &["cwd"])?;
                    enrich::guess_session_dir(&grok_home, cwd, &id)
                });
        }

        match event.as_str() {
            "SessionStart" => {
                session.started_s = now_ms / 1000;
                session.activity = Activity::Idle;
                session.subagents = 0;
                session.refresh();
            }
            "UserPromptSubmit" => {
                session.activity = Activity::Thinking;
                session.turn_started_s = Some(now_ms / 1000);
                session.refresh();
            }
            "PreToolUse" => {
                let name = str_field(ev, &["toolName", "tool_name"])
                    .unwrap_or("")
                    .to_owned();
                let kind = classify_tool(&name);
                match kind {
                    ToolKind::Edit => session.edits += 1,
                    ToolKind::Terminal if name == "run_terminal_command" => session.commands += 1,
                    _ => {}
                }
                let input = ev.get("toolInput").or_else(|| ev.get("tool_input"));
                // Only edit/read tools target a file. grep/list_dir `path` is a folder.
                let file = matches!(kind, ToolKind::Edit | ToolKind::Read)
                    .then(|| input.and_then(file_from_input))
                    .flatten();
                let command = input
                    .and_then(|i| i.get("command"))
                    .and_then(Value::as_str)
                    .and_then(command_program);
                session.activity = Activity::Tool {
                    kind,
                    name,
                    file,
                    command,
                };
            }
            "PostToolUse" | "PostToolUseFailure" | "PostCompact" | "PermissionDenied" => {
                session.activity = Activity::Thinking;
            }
            "Notification" => {
                match str_field(
                    ev,
                    &["notificationType", "notification_type", "type", "matcher"],
                ) {
                    Some("permission_prompt") => session.activity = Activity::AwaitingApproval,
                    Some("idle_prompt") => session.activity = Activity::Idle,
                    _ => {}
                }
            }
            "Stop" | "StopCancelled" => {
                session.activity = Activity::Idle;
                session.turn_started_s = None;
                session.subagents = 0;
                session.refresh();
            }
            "StopFailure" => {
                let kind = str_field(ev, &["error"]).unwrap_or("unknown").to_owned();
                session.activity = Activity::Error {
                    kind,
                    until_ms: now_ms + ERROR_DISPLAY_MS,
                };
                session.turn_started_s = None;
            }
            "PreCompact" => session.activity = Activity::Compacting,
            "SubagentStart" => session.subagents += 1,
            "SubagentStop" | "SubagentEnd" => {
                session.subagents = session.subagents.saturating_sub(1)
            }
            _ => {}
        }
    }

    /// Time-based transitions: expire errors, drop stale sessions.
    pub fn tick(&mut self, now_ms: u64) {
        for s in self.sessions.values_mut() {
            if let Activity::Error { until_ms, .. } = s.activity
                && now_ms >= until_ms
            {
                s.activity = Activity::Idle;
            }
        }
        self.sessions.retain(|_, s| {
            let last = s.alive_ms.unwrap_or(0).max(s.last_event_ms);
            now_ms.saturating_sub(last) < STALE_SESSION_MS
        });
    }
}

/// Basename of the file a tool targets.
fn file_from_input(input: &Value) -> Option<String> {
    [
        "file_path",
        "path",
        "target_file",
        "target_notebook",
        "notebook_path",
    ]
    .iter()
    .find_map(|k| input.get(*k).and_then(Value::as_str))
    .and_then(|p| Path::new(p).file_name())
    .map(|n| n.to_string_lossy().into_owned())
    .filter(|n| !n.is_empty())
}

/// The program a shell command runs, e.g. `cd x && FOO=1 sudo cargo test` → `cargo`.
/// Never returns arguments: they can contain secrets.
pub fn command_program(cmd: &str) -> Option<String> {
    const WRAPPERS: &[&str] = &[
        "sudo", "env", "time", "nice", "nohup", "exec", "command", "doas",
    ];
    for segment in cmd.split(['&', ';', '|', '\n']) {
        let words = segment.split_whitespace().skip_while(|w| {
            w.contains('=') && !w.starts_with('=') || WRAPPERS.contains(w) || w.starts_with('(')
        });
        let Some(first) = words.into_iter().next() else {
            continue;
        };
        let prog = first
            .rsplit('/')
            .next()
            .unwrap_or(first)
            .trim_matches(|c| c == '"' || c == '\'' || c == '(' || c == ')');
        if prog.is_empty() || prog == "cd" || prog == "pushd" {
            continue;
        }
        return Some(prog.chars().take(32).collect());
    }
    None
}

fn snake_to_pascal(s: &str) -> String {
    s.split('_')
        .map(|w| {
            let mut c = w.chars();
            c.next()
                .map(|f| f.to_ascii_uppercase().to_string() + c.as_str())
                .unwrap_or_default()
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn fixture(name: &str) -> Value {
        let p = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/events")
            .join(name);
        serde_json::from_str(&std::fs::read_to_string(p).unwrap()).unwrap()
    }

    fn state() -> State {
        State::new("/nonexistent-grok-home")
    }

    const SID: &str = "01a197b2-a078-7f83-9cd2-1ac8bae6e13e";

    #[test]
    fn replays_real_captured_session() {
        let p = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/events/session_edit_sequence.jsonl");
        let events: Vec<Value> = std::fs::read_to_string(p)
            .unwrap()
            .lines()
            .map(|l| serde_json::from_str(l).unwrap())
            .collect();
        let names: Vec<&str> = events
            .iter()
            .map(|e| e["hook_event_name"].as_str().unwrap())
            .collect();
        assert_eq!(
            names,
            [
                "SessionStart",
                "UserPromptSubmit",
                "PreToolUse",
                "PostToolUse",
                "Stop",
                "SessionEnd",
                "Stop"
            ]
        );

        let mut st = state();
        let mut seen = Vec::new();
        for (i, ev) in events.iter().enumerate() {
            st.apply(ev, 1_000 * (i as u64 + 1));
            seen.push(st.sessions.get(SID).map(|s| s.activity.label()));
        }
        assert_eq!(
            seen,
            [
                Some("Idle".into()),
                Some("Thinking…".into()),
                Some("Editing code".into()),
                Some("Thinking…".into()),
                Some("Idle".into()),
                None, // SessionEnd removes it
                None, // trailing shutdown Stop must not resurrect it
            ]
        );
    }

    #[test]
    fn session_start_new_sets_workspace() {
        let mut st = state();
        st.apply(&fixture("session_start_new.json"), 5_000);
        let s = st.primary().unwrap();
        assert_eq!(s.project_name(), "grok-build-rpc");
        assert_eq!(s.started_s, 5);
    }

    #[test]
    fn terminal_permission_and_rate_limit() {
        let mut st = state();
        st.apply(&fixture("session_start_load.json"), 1_000);
        st.apply(&fixture("pre_tool_use_terminal.json"), 2_000);
        assert_eq!(st.primary().unwrap().activity.label(), "Running a command");
        assert_eq!(st.primary().unwrap().commands, 1);
        st.apply(&fixture("notification_permission.json"), 3_000);
        assert_eq!(st.primary().unwrap().activity, Activity::AwaitingApproval);
        st.apply(&fixture("stop_failure_rate_limit.json"), 4_000);
        assert_eq!(st.primary().unwrap().activity.label(), "Rate limited");
        st.tick(4_000 + ERROR_DISPLAY_MS);
        assert_eq!(st.primary().unwrap().activity, Activity::Idle);
    }

    #[test]
    fn subagent_counting() {
        let mut st = state();
        st.apply(&fixture("session_start_load.json"), 1_000);
        st.apply(&fixture("subagent_start.json"), 2_000);
        assert_eq!(st.primary().unwrap().subagents, 1);
        st.apply(&fixture("stop_end_turn.json"), 3_000);
        assert_eq!(st.primary().unwrap().subagents, 0);
    }

    #[test]
    fn unknown_subagent_session_is_ignored() {
        let mut st = state();
        st.apply(&json!({"hook_event_name": "PreToolUse", "sessionId": "child", "subagentType": "explore",
                         "toolName": "read_file", "cwd": "/x"}), 1_000);
        assert!(st.is_empty());
    }

    #[test]
    fn mcp_tool_label() {
        let a = Activity::Tool {
            kind: classify_tool("linear__save_issue"),
            name: "linear__save_issue".into(),
            file: None,
            command: None,
        };
        assert_eq!(a.label(), "Using linear");
        assert_eq!(a.mcp_server(), Some("linear"));
        assert_eq!(a.key(), "mcp");
    }

    #[test]
    fn records_file_and_program() {
        let mut st = state();
        st.apply(&fixture("pre_tool_use.json"), 1_000);
        match &st.primary().unwrap().activity {
            Activity::Tool { file, .. } => assert_eq!(file.as_deref(), Some("groktest-output.txt")),
            a => panic!("{a:?}"),
        }
        st.apply(&fixture("pre_tool_use_terminal.json"), 2_000);
        match &st.primary().unwrap().activity {
            Activity::Tool { command, .. } => assert_eq!(command.as_deref(), Some("cargo")),
            a => panic!("{a:?}"),
        }
    }

    #[test]
    fn command_program_extraction() {
        let c = |s| command_program(s);
        assert_eq!(c("cargo test --release"), Some("cargo".into()));
        assert_eq!(c("cd /repo && npm run build"), Some("npm".into()));
        assert_eq!(
            c("FOO=1 BAR=2 sudo /usr/bin/python3 x.py"),
            Some("python3".into())
        );
        assert_eq!(c("  "), None);
        assert_eq!(c("curl -H 'Authorization: secret' x"), Some("curl".into()));
    }

    #[test]
    fn stale_sessions_dropped() {
        let mut st = state();
        st.apply(&fixture("session_start_load.json"), 0);
        st.tick(STALE_SESSION_MS + 1);
        assert!(st.is_empty());
    }

    #[test]
    fn camel_only_event_name_is_understood() {
        let mut st = state();
        st.apply(
            &json!({"hookEventName": "user_prompt_submit", "sessionId": "s", "cwd": "/p/proj"}),
            1,
        );
        assert_eq!(st.primary().unwrap().activity, Activity::Thinking);
    }
}
