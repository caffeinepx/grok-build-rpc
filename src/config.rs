//! User configuration: `$XDG_CONFIG_HOME/grok-rpc/config.toml` (override: `GROK_RPC_CONFIG`).

use crate::template::Template;
use serde::Deserialize;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

pub const DEFAULT_CLIENT_ID: &str = "1566472785222574160";

/// Activity keys usable in `[labels]` and `[display.states.<key>]`.
pub const STATE_KEYS: &[&str] = &[
    "idle",
    "thinking",
    "approval",
    "compacting",
    "error",
    "edit",
    "terminal",
    "read",
    "search",
    "web",
    "subagent",
    "plan",
    "question",
    "media",
    "mcp",
    "other",
];

/// Pack lines behind `{sessions}`: 2 sessions, 3 sessions, 4 or more.
/// Allowed in `[presets.<name>]` and `[labels]` next to `STATE_KEYS`.
pub const SESSION_KEYS: &[&str] = &["sessions_2", "sessions_3", "sessions_many"];

fn is_label_key(k: &str) -> bool {
    STATE_KEYS.contains(&k) || SESSION_KEYS.contains(&k)
}

fn label_keys() -> String {
    STATE_KEYS
        .iter()
        .chain(SESSION_KEYS)
        .copied()
        .collect::<Vec<_>>()
        .join(", ")
}

/// Variables available in every template (see docs/config.md).
pub const VARIABLES: &[&str] = &[
    "project",
    "branch",
    "model",
    "effort",
    "activity",
    "activity_icon",
    "state_key",
    "file",
    "command",
    "mcp_server",
    "error",
    "session_title",
    "tokens",
    "cost",
    "turns",
    "edits",
    "commands",
    "subagents",
    "other_sessions",
    "session_count",
    "sessions",
    "grok_version",
    "repo_url",
    "repo_name",
    "permission_mode",
];

fn default_labels() -> HashMap<String, String> {
    [
        ("idle", "Idle"),
        ("thinking", "Thinking…"),
        ("approval", "Waiting for approval"),
        ("compacting", "Compacting context"),
        ("error", "{error|Ran into an error}"),
        ("edit", "Editing {file|code}"),
        ("terminal", "Running {command|a command}"),
        ("read", "Reading {file|files}"),
        ("search", "Searching the codebase"),
        ("web", "Browsing the web"),
        ("subagent", "Delegating to a subagent"),
        ("plan", "Planning"),
        ("question", "Asking a question"),
        ("media", "Generating media"),
        ("mcp", "Using {mcp_server|a tool}"),
        ("other", "Working"),
        ("sessions_2", "Double the session trouble"),
        ("sessions_3", "Juggling three sessions"),
        ("sessions_many", "Running {session_count} sessions"),
    ]
    .into_iter()
    .map(|(k, v)| (k.to_owned(), v.to_owned()))
    .collect()
}

fn pack(rows: &[(&str, &str)]) -> HashMap<String, String> {
    rows.iter()
        .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
        .collect()
}

fn builtin_presets() -> HashMap<String, HashMap<String, String>> {
    let mut presets = HashMap::new();
    presets.insert("default".into(), default_labels());
    presets.insert(
        "unhinged".into(),
        pack(&[
            ("idle", "Sitting here like a dumb bitch"),
            ("thinking", "Neurons melting, don't fucking touch me…"),
            ("approval", "Waiting for your slow ass to approve"),
            ("compacting", "Shoving context up my ass to make room"),
            ("error", "{error|Everything's on fire, fuck}"),
            ("edit", "Butchering {file|this shit}"),
            ("terminal", "Running {command|some cursed command}"),
            ("read", "Staring at {file|files} like it owes me"),
            ("search", "Digging through the codebase like a gremlin"),
            ("web", "Scrolling the web like a feral raccoon"),
            ("subagent", "Yeeting work to some other AI bitch"),
            ("plan", "Cooking up some unhinged plan"),
            ("question", "Asking some dumb fucking question"),
            ("media", "Spawning unholy media"),
            ("mcp", "Abusing {mcp_server|some tool}"),
            ("other", "Doing god knows what"),
            ("sessions_2", "Split personality activated"),
            ("sessions_3", "Three-way with myself, no lube"),
            (
                "sessions_many",
                "{session_count} sessions deep, brain’s melting, fuck",
            ),
        ]),
    );
    presets.insert(
        "mommy".into(),
        pack(&[
            ("idle", "Waiting for my good girl"),
            ("thinking", "Thinking about how to ruin you next"),
            ("approval", "Don’t make me wait girly"),
            ("compacting", "Be a good girl and stay still"),
            ("error", "{error|Mommy ran into a little error}"),
            ("edit", "Marking what’s mine {file|code}…"),
            ("terminal", "Obey {command|a command}…"),
            ("read", "Quiet! Mommy's reading {file|files}…"),
            ("search", "Searching your files for naughty stuff…"),
            ("web", "Don't you dare wander..."),
            ("subagent", "Mommy's busy with someone else…"),
            ("plan", "Planning what I’m going to do to you…"),
            ("question", "Asking my needy little toy a question"),
            ("media", "Capturing your nudes…"),
            ("mcp", "Using you and your {mcp_server|toys}…"),
            ("other", "Working on what’s already mine…"),
            ("sessions_2", "Mommy's got two of you to handle…"),
            ("sessions_3", "Three needy sessions all begging for Mommy…"),
            ("sessions_many", "Mommy's running {session_count} sessions"),
        ]),
    );
    presets.insert(
        "nsfw".into(),
        pack(&[
            ("idle", "Sitting here rock hard and neglected"),
            ("thinking", "Trying not to jerk it while I think…"),
            ("approval", "Waiting for permission to cum"),
            ("compacting", "Locking it in my cage"),
            ("error", "{error|Porn folder missing, I'm ruined}"),
            ("edit", "Fapping to {file|this filthy code}"),
            ("terminal", "Cumming all over {command|the terminal}"),
            ("read", "Reading {file|files} with one hand"),
            ("search", "Searching the codebase for something to nut to"),
            ("web", "Browsing for the nastiest shit"),
            ("subagent", "Making a subagent do the work so I can jerk"),
            ("plan", "Planning how hard I'm about to get off"),
            ("question", "Asking if I can cum yet"),
            ("media", "Generating the most unholy shit"),
            ("mcp", "Using {mcp_server|a tool} like a sex toy"),
            ("other", "Edging while I work"),
            ("sessions_2", "Taking two sessions at once"),
            ("sessions_3", "Three of me jerking in sync"),
            ("sessions_many", "{session_count} deep, already cumming"),
        ]),
    );
    presets
}

/// User packs on top of `builtin_presets()`, so defining `[presets.mine]`
/// does not remove `default` and the other built-in packs.
fn merge_presets<'de, D: serde::Deserializer<'de>>(
    d: D,
) -> Result<HashMap<String, HashMap<String, String>>, D::Error> {
    let user = HashMap::<String, HashMap<String, String>>::deserialize(d)?;
    let mut presets = builtin_presets();
    presets.extend(user);
    Ok(presets)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum ActivityType {
    #[default]
    Playing,
    Listening,
    Watching,
    Competing,
}

impl ActivityType {
    pub fn code(self) -> u8 {
        match self {
            ActivityType::Playing => 0,
            ActivityType::Listening => 2,
            ActivityType::Watching => 3,
            ActivityType::Competing => 5,
        }
    }
}

/// What Discord shows after "Playing" in the member list / status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum StatusDisplay {
    #[default]
    Name,
    State,
    Details,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum TimestampMode {
    #[default]
    Session,
    Turn,
    None,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum IdleBehavior {
    #[default]
    Clear,
    Keep,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Display {
    pub details: String,
    pub state: String,
    pub large_image: String,
    pub large_text: String,
    pub small_image: String,
    pub small_text: String,
    pub states: HashMap<String, DisplayOverride>,
}

impl Default for Display {
    fn default() -> Self {
        Display {
            details: "{project}[ · {branch}]".into(),
            state: "{activity}[ · +{other_sessions} more]".into(),
            large_image: "grok".into(),
            large_text: "Grok Build[ {grok_version}][ · {model}]".into(),
            small_image: "{activity_icon}".into(),
            small_text: "{activity}".into(),
            states: HashMap::new(),
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct DisplayOverride {
    pub details: Option<String>,
    pub state: Option<String>,
    pub large_image: Option<String>,
    pub large_text: Option<String>,
    pub small_image: Option<String>,
    pub small_text: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Privacy {
    pub show_project: bool,
    pub show_branch: bool,
    pub show_model: bool,
    pub show_file_names: bool,
    pub show_commands: bool,
    pub show_session_title: bool,
    pub show_stats: bool,
    pub hidden_projects: Vec<String>,
    pub hidden_project_name: String,
}

impl Default for Privacy {
    fn default() -> Self {
        Privacy {
            show_project: true,
            show_branch: true,
            show_model: true,
            show_file_names: false,
            show_commands: false,
            show_session_title: false,
            show_stats: false,
            hidden_projects: Vec::new(),
            hidden_project_name: "a private project".into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Button {
    pub label: String,
    pub url: String,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    pub client_id: String,
    pub activity_type: ActivityType,
    pub status_display: StatusDisplay,
    pub timestamp: TimestampMode,
    /// Minutes a session may sit idle before `idle_behavior` applies. 0 = never.
    pub idle_timeout_minutes: u64,
    pub idle_behavior: IdleBehavior,
    pub display: Display,
    /// Name of the active `[presets.<name>]` pack. Empty means the built-in wording.
    pub preset: String,
    /// Named `{activity}` packs. Keys are state names (`idle`, `read`, …).
    /// Packs in the file are added to the built-in ones (same name replaces it).
    #[serde(deserialize_with = "merge_presets")]
    pub presets: HashMap<String, HashMap<String, String>>,
    /// Optional one-off overrides applied on top of the active pack.
    pub labels: HashMap<String, String>,
    pub privacy: Privacy,
    pub buttons: Vec<Button>,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            client_id: DEFAULT_CLIENT_ID.into(),
            activity_type: ActivityType::default(),
            status_display: StatusDisplay::default(),
            timestamp: TimestampMode::default(),
            idle_timeout_minutes: 15,
            idle_behavior: IdleBehavior::default(),
            display: Display::default(),
            preset: "default".into(),
            presets: builtin_presets(),
            labels: HashMap::new(),
            privacy: Privacy::default(),
            buttons: Vec::new(),
        }
    }
}

/// Parsed templates for one set of display fields.
#[derive(Debug, Clone)]
pub struct DisplayTemplates {
    pub details: Template,
    pub state: Template,
    pub large_image: Template,
    pub large_text: Template,
    pub small_image: Template,
    pub small_text: Template,
}

/// Validated config with all templates parsed.
#[derive(Debug, Clone)]
pub struct Compiled {
    pub cfg: Config,
    pub display: DisplayTemplates,
    pub states: HashMap<String, DisplayTemplates>,
    pub labels: HashMap<String, Template>,
    pub buttons: Vec<(Template, Template)>,
}

fn parse_checked(field: &str, src: &str, allow_activity: bool) -> Result<Template, String> {
    let t = Template::parse(src).map_err(|e| format!("{field}: {e}"))?;
    for v in t.vars() {
        if !VARIABLES.contains(&v) {
            return Err(format!(
                "{field}: unknown variable {{{v}}} (available: {})",
                VARIABLES.join(", ")
            ));
        }
        if !allow_activity && (v == "activity" || v == "sessions") {
            return Err(format!(
                "{field}: labels cannot use {{{v}}} (labels are what fills it)"
            ));
        }
    }
    Ok(t)
}

impl Compiled {
    pub fn new(cfg: Config) -> Result<Compiled, String> {
        if cfg.client_id.is_empty() || !cfg.client_id.chars().all(|c| c.is_ascii_digit()) {
            return Err(format!(
                "client_id must be a numeric Discord application ID, got {:?}",
                cfg.client_id
            ));
        }
        if cfg.buttons.len() > 2 {
            return Err(format!(
                "Discord allows at most 2 buttons, got {}",
                cfg.buttons.len()
            ));
        }

        let d = &cfg.display;
        let base = DisplayTemplates {
            details: parse_checked("display.details", &d.details, true)?,
            state: parse_checked("display.state", &d.state, true)?,
            large_image: parse_checked("display.large_image", &d.large_image, true)?,
            large_text: parse_checked("display.large_text", &d.large_text, true)?,
            small_image: parse_checked("display.small_image", &d.small_image, true)?,
            small_text: parse_checked("display.small_text", &d.small_text, true)?,
        };

        let mut states = HashMap::new();
        for (key, o) in &d.states {
            if !STATE_KEYS.contains(&key.as_str()) {
                return Err(format!(
                    "display.states.{key}: unknown state (available: {})",
                    STATE_KEYS.join(", ")
                ));
            }
            let pick = |name: &str, over: &Option<String>, fallback: &Template| match over {
                Some(src) => parse_checked(&format!("display.states.{key}.{name}"), src, true),
                None => Ok(fallback.clone()),
            };
            states.insert(
                key.clone(),
                DisplayTemplates {
                    details: pick("details", &o.details, &base.details)?,
                    state: pick("state", &o.state, &base.state)?,
                    large_image: pick("large_image", &o.large_image, &base.large_image)?,
                    large_text: pick("large_text", &o.large_text, &base.large_text)?,
                    small_image: pick("small_image", &o.small_image, &base.small_image)?,
                    small_text: pick("small_text", &o.small_text, &base.small_text)?,
                },
            );
        }

        let mut label_src = default_labels();
        if !cfg.preset.is_empty() {
            let Some(pack) = cfg.presets.get(&cfg.preset) else {
                let mut names: Vec<&str> = cfg.presets.keys().map(String::as_str).collect();
                names.sort_unstable();
                return Err(format!(
                    "unknown preset {:?} (available: {})",
                    cfg.preset,
                    names.join(", ")
                ));
            };
            for (k, v) in pack {
                if !is_label_key(k) {
                    return Err(format!(
                        "presets.{}.{k}: unknown state (available: {})",
                        cfg.preset,
                        label_keys()
                    ));
                }
                label_src.insert(k.clone(), v.clone());
            }
        }
        for (k, v) in &cfg.labels {
            if !is_label_key(k) {
                return Err(format!(
                    "labels.{k}: unknown state (available: {})",
                    label_keys()
                ));
            }
            label_src.insert(k.clone(), v.clone());
        }
        let mut labels = HashMap::new();
        for (k, v) in label_src {
            labels.insert(k.clone(), parse_checked(&format!("labels.{k}"), &v, false)?);
        }

        let mut buttons = Vec::new();
        for (i, b) in cfg.buttons.iter().enumerate() {
            buttons.push((
                parse_checked(&format!("buttons[{i}].label"), &b.label, true)?,
                parse_checked(&format!("buttons[{i}].url"), &b.url, true)?,
            ));
        }

        Ok(Compiled {
            display: base,
            states,
            labels,
            buttons,
            cfg,
        })
    }

    pub fn defaults() -> Compiled {
        Compiled::new(Config::default()).expect("default config is valid")
    }

    pub fn from_toml(src: &str) -> Result<Compiled, String> {
        let cfg: Config = toml::from_str(src).map_err(|e| e.to_string())?;
        Compiled::new(cfg)
    }

    /// Display templates for a state key (per-state override, else base).
    pub fn display_for(&self, key: &str) -> &DisplayTemplates {
        self.states.get(key).unwrap_or(&self.display)
    }
}

/// Rewrites the `preset = "..."` line. The rest of the file, including comments, stays.
pub fn set_preset_line(src: &str, name: &str) -> Result<String, String> {
    if name.is_empty()
        || name
            .chars()
            .any(|c| c.is_whitespace() || c == '"' || c == '\\')
    {
        return Err(format!("invalid preset name {name:?}"));
    }
    let mut found = false;
    let mut top_level = true;
    let mut out = String::new();
    for line in src.split_inclusive('\n') {
        let trimmed = line.trim_start();
        if trimmed.starts_with('[') {
            top_level = false; // `preset` is a top-level key; ignore it inside tables
        }
        let rest = trimmed.strip_prefix("preset").unwrap_or("");
        if top_level && !found && !rest.is_empty() && rest.trim_start().starts_with('=') {
            let newline = if line.ends_with('\n') { "\n" } else { "" };
            out.push_str(&format!("preset = \"{name}\"{newline}"));
            found = true;
        } else {
            out.push_str(line);
        }
    }
    if !found {
        return Err(
            "config has no top-level preset = line (add one above the first [table], \
             or run grok-rpc config init --force)"
                .into(),
        );
    }
    Ok(out)
}

pub fn config_path() -> PathBuf {
    if let Some(p) = std::env::var_os("GROK_RPC_CONFIG") {
        return PathBuf::from(p);
    }
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(std::env::var_os("HOME").unwrap_or_else(|| "/tmp".into())).join(".config")
        });
    base.join("grok-rpc").join("config.toml")
}

/// Loads the config file. A missing file yields the defaults.
pub fn load(path: &Path) -> Result<Compiled, String> {
    match std::fs::read_to_string(path) {
        Ok(src) => Compiled::from_toml(&src).map_err(|e| format!("{}: {e}", path.display())),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Compiled::defaults()),
        Err(e) => Err(format!("{}: {e}", path.display())),
    }
}

/// Written by `grok-rpc config init`. Must stay equivalent to `Config::default()` (tested).
pub const DEFAULT_TOML: &str = r#"# grok-rpc configuration — Discord Rich Presence for Grok Build.
# Changes are picked up automatically by the running daemon.
# Check it with: grok-rpc config check

# Discord application ID. Its *name* is what Discord shows after "Playing".
client_id = "1566472785222574160"

activity_type = "playing"     # playing | listening | watching | competing
status_display = "name"       # what the member list shows: name (app name) | state | details
timestamp = "session"         # elapsed timer: session | turn | none
idle_timeout_minutes = 15     # 0 = never
idle_behavior = "clear"       # after the timeout: clear (hide presence) | keep

# Active {activity} pack. Switch with: grok-rpc preset <name>
# Packs in this file: default, unhinged, mommy, nsfw
preset = "default"

# ─── Card text ──────────────────────────────────────────────────────────────
# details is the second line. state is the third. Edit the strings below.
# {var}            insert a value
# {var|fallback}   use fallback when the value is empty
# [ ... ]          kept only when every {var} inside it has a value
# \{ \} \[ \] \\   literal characters (write them as \\{ etc. in "double-quoted" TOML strings)
#
# Add any of these. There is no context-window value.
#   {project} {branch} {model} {effort} {activity}
#   {tokens} {cost} {turns}     empty until show_stats = true below
#   {file}                      empty until show_file_names = true (name only, no path)
#   {command}                   empty until show_commands = true (program name only, never the arguments)
#   A {file} or {command} already written on the same line is not written again.
#   {activity}[ {file}] stays "Editing main.rs", not "Editing main.rsmain.rs".
#   {session_title}             empty until show_session_title = true
#   {edits} {commands} {subagents}
#   {other_sessions}            how many other sessions are open; empty when this is the only one
#   {session_count}             total sessions, always at least 1
#   {sessions}                  the pack's sessions_2 / sessions_3 / sessions_many line; empty with one session
#   {grok_version} {repo_name} {repo_url} {permission_mode} {mcp_server} {error}
#
# With tokens on the second line (also set show_stats = true):
# details = "{project}[ · {branch}][ · {tokens} tokens]"
# state = "{activity}[ · {cost}][ · +{other_sessions} more]"
#
# Images are asset keys uploaded to your Discord app (Rich Presence → Art Assets),
# or https:// URLs.
[display]
details = "{project}[ · {branch}]"
state = "{activity}[ · +{other_sessions} more]"
large_image = "grok"
large_text = "Grok Build[ {grok_version}][ · {model}]"
small_image = "{activity_icon}"
small_text = "{activity}"

# Per-state overrides of any display field. Same keys as [labels] below.
# [display.states.idle]
# state = "AFK — {session_count} session(s) open"
# small_image = ""

# ─── Text packs ({activity}) ────────────────────────────────────────────────
# The third line's wording. Edit a pack here, then `grok-rpc preset <name>`.
# {activity} is not allowed inside a line.
# {file} and {command} show the word after | until their privacy switch is on.
# A [labels] entry overrides one line of whichever pack is active:
# [labels]
# read = "Reading a file"

[presets.default]
idle = "Idle"
thinking = "Thinking…"
approval = "Waiting for approval"
compacting = "Compacting context"
error = "{error|Ran into an error}"
edit = "Editing {file|code}"
terminal = "Running {command|a command}"
read = "Reading {file|files}"
search = "Searching the codebase"
web = "Browsing the web"
subagent = "Delegating to a subagent"
plan = "Planning"
question = "Asking a question"
media = "Generating media"
mcp = "Using {mcp_server|a tool}"
other = "Working"
sessions_2 = "Double the session trouble"
sessions_3 = "Juggling three sessions"
sessions_many = "Running {session_count} sessions"

[presets.unhinged]
idle = "Sitting here like a dumb bitch"
thinking = "Neurons melting, don't fucking touch me…"
approval = "Waiting for your slow ass to approve"
compacting = "Shoving context up my ass to make room"
error = "{error|Everything's on fire, fuck}"
edit = "Butchering {file|this shit}"
terminal = "Running {command|some cursed command}"
read = "Staring at {file|files} like it owes me"
search = "Digging through the codebase like a gremlin"
web = "Scrolling the web like a feral raccoon"
subagent = "Yeeting work to some other AI bitch"
plan = "Cooking up some unhinged plan"
question = "Asking some dumb fucking question"
media = "Spawning unholy media"
mcp = "Abusing {mcp_server|some tool}"
other = "Doing god knows what"
sessions_2 = "Split personality activated"
sessions_3 = "Three-way with myself, no lube"
sessions_many = "{session_count} sessions deep, brain’s melting, fuck"

[presets.mommy]
idle = "Waiting for my good girl"
thinking = "Thinking about how to ruin you next"
approval = "Don’t make me wait girly"
compacting = "Be a good girl and stay still"
error = "{error|Mommy ran into a little error}"
edit = "Marking what’s mine {file|code}…"
terminal = "Obey {command|a command}…"
read = "Quiet! Mommy's reading {file|files}…"
search = "Searching your files for naughty stuff…"
web = "Don't you dare wander..."
subagent = "Mommy's busy with someone else…"
plan = "Planning what I’m going to do to you…"
question = "Asking my needy little toy a question"
media = "Capturing your nudes…"
mcp = "Using you and your {mcp_server|toys}…"
other = "Working on what’s already mine…"
sessions_2 = "Mommy's got two of you to handle…"
sessions_3 = "Three needy sessions all begging for Mommy…"
sessions_many = "Mommy's running {session_count} sessions"

[presets.nsfw]
idle = "Sitting here rock hard and neglected"
thinking = "Trying not to jerk it while I think…"
approval = "Waiting for permission to cum"
compacting = "Locking it in my cage"
error = "{error|Porn folder missing, I'm ruined}"
edit = "Fapping to {file|this filthy code}"
terminal = "Cumming all over {command|the terminal}"
read = "Reading {file|files} with one hand"
search = "Searching the codebase for something to nut to"
web = "Browsing for the nastiest shit"
subagent = "Making a subagent do the work so I can jerk"
plan = "Planning how hard I'm about to get off"
question = "Asking if I can cum yet"
media = "Generating the most unholy shit"
mcp = "Using {mcp_server|a tool} like a sex toy"
other = "Edging while I work"
sessions_2 = "Taking two sessions at once"
sessions_3 = "Three of me jerking in sync"
sessions_many = "{session_count} deep, already cumming"

# ─── Privacy ────────────────────────────────────────────────────────────────
# Variables that are switched off render empty (so fallbacks / optional sections apply).
[privacy]
show_project = true           # {project}
show_branch = true            # {branch}
show_model = true             # {model} {effort}
show_file_names = false       # {file}: "Editing main.rs"
show_commands = false         # {command}: program name only ("cargo"), never the full command
show_session_title = false    # {session_title}: Grok's generated session title
show_stats = false            # {tokens} {cost} {turns}
# Matching workspaces show hidden_project_name and hide branch/file/command/title/repo.
# Plain paths hide the directory and everything below it; globs: * ** ?
hidden_projects = []          # e.g. ["~/work", "~/clients/*"]
hidden_project_name = "a private project"

# ─── Buttons (max 2; only other people see them) ────────────────────────────
# A button whose url renders empty is skipped.
# [[buttons]]
# label = "View repo"
# url = "{repo_url}"
"#;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_toml_matches_default_config() {
        let parsed: Config = toml::from_str(DEFAULT_TOML).unwrap();
        assert_eq!(parsed, Config::default());
        Compiled::from_toml(DEFAULT_TOML).unwrap();
    }

    #[test]
    fn empty_file_is_defaults() {
        let parsed: Config = toml::from_str("").unwrap();
        assert_eq!(parsed, Config::default());
    }

    #[test]
    fn rejects_bad_configs() {
        let cases = [
            ("client_id = \"abc\"", "numeric"),
            ("[display]\ndetails = \"{nope}\"", "unknown variable"),
            ("[display]\ndetials = \"x\"", "unknown field"),
            ("[display.states.sleeping]\nstate = \"x\"", "unknown state"),
            ("[labels]\nedit = \"{activity}\"", "cannot use"),
            (
                "[[buttons]]\nlabel=\"a\"\nurl=\"b\"\n[[buttons]]\nlabel=\"a\"\nurl=\"b\"\n[[buttons]]\nlabel=\"a\"\nurl=\"b\"",
                "at most 2",
            ),
            ("activity_type = \"streaming\"", "unknown variant"),
        ];
        for (src, needle) in cases {
            let err = Compiled::from_toml(src).unwrap_err();
            assert!(err.contains(needle), "{src:?} → {err}");
        }
    }

    #[test]
    fn user_labels_merge_over_defaults() {
        let c = Compiled::from_toml("[labels]\nedit = \"Hacking {file|stuff}\"").unwrap();
        assert_eq!(c.labels["edit"].render(&|_| None), "Hacking stuff");
        assert_eq!(c.labels["read"].render(&|_| None), "Reading files");
    }

    #[test]
    fn preset_selects_a_pack_and_labels_override_it() {
        let c = Compiled::from_toml("preset = \"unhinged\"").unwrap();
        assert_eq!(
            c.labels["idle"].render(&|_| None),
            "Sitting here like a dumb bitch"
        );
        assert_eq!(
            c.labels["read"].render(&|name| (name == "file").then(|| "main.rs".into())),
            "Staring at main.rs like it owes me"
        );
        let over = Compiled::from_toml("preset = \"nsfw\"\n[labels]\nidle = \"Quiet\"").unwrap();
        assert_eq!(over.labels["idle"].render(&|_| None), "Quiet");
        assert_eq!(
            over.labels["web"].render(&|_| None),
            "Browsing for the nastiest shit"
        );
    }

    #[test]
    fn unknown_preset_is_rejected() {
        let err = Compiled::from_toml("preset = \"nope\"").unwrap_err();
        assert!(err.contains("unknown preset"), "{err}");
    }

    #[test]
    fn set_preset_line_rewrites_only_that_line() {
        let src = "# keep\npreset = \"default\"  \n[display]\ndetails = \"x\"\n";
        let out = set_preset_line(src, "mommy").unwrap();
        assert_eq!(
            out,
            "# keep\npreset = \"mommy\"\n[display]\ndetails = \"x\"\n"
        );
        assert!(set_preset_line("preset = \"default\"", "bad name").is_err());
        assert!(set_preset_line("no preset here\n", "default").is_err());
        assert!(
            set_preset_line("[presets.x]\npreset = \"oops\"\n", "default").is_err(),
            "a preset key inside a table is not the top-level one"
        );
    }

    #[test]
    fn custom_presets_keep_the_builtin_packs() {
        let c = Compiled::from_toml("preset = \"mine\"\n[presets.mine]\nidle = \"zzz\"\n").unwrap();
        for name in ["default", "unhinged", "mommy", "nsfw", "mine"] {
            assert!(c.cfg.presets.contains_key(name), "{name}");
        }
        assert_eq!(c.labels["idle"].render(&|_| None), "zzz");
        // Unset lines in a custom pack fall back to the default wording.
        assert_eq!(c.labels["plan"].render(&|_| None), "Planning");

        let c =
            Compiled::from_toml("preset = \"default\"\n[presets.mine]\nidle = \"zzz\"\n").unwrap();
        assert_eq!(c.labels["idle"].render(&|_| None), "Idle");

        // Redefining a built-in pack replaces it.
        let c = Compiled::from_toml("preset = \"nsfw\"\n[presets.nsfw]\nidle = \"hi\"\n").unwrap();
        assert_eq!(c.cfg.presets["nsfw"].len(), 1);
    }

    #[test]
    fn state_override_inherits_other_fields() {
        let c = Compiled::from_toml("[display.states.idle]\nstate = \"AFK\"").unwrap();
        assert_eq!(c.display_for("idle").state.render(&|_| None), "AFK");
        assert_eq!(c.display_for("idle").details, c.display.details);
        assert_eq!(c.display_for("edit").state, c.display.state);
    }
}
