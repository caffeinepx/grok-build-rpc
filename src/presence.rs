//! Turns daemon state + config into a Discord activity payload.
//!
//! Privacy is enforced here, in one place: variables the user hasn't enabled are
//! never computed, so no template can leak them.

use crate::config::{Compiled, IdleBehavior, StatusDisplay, TimestampMode};
use crate::daemon::state::{Activity, Session, State};
use crate::glob;
use serde_json::{Map, Value, json};
use std::collections::HashMap;

pub struct Context<'a> {
    pub grok_version: Option<&'a str>,
    pub now_ms: u64,
}

/// Truncates to Discord's limit (with an ellipsis) and pads 1-char strings (min length is 2).
pub fn clamp_len(s: &str, max: usize) -> String {
    let count = s.chars().count();
    let mut out: String = if count > max {
        s.chars().take(max - 1).collect::<String>() + "…"
    } else {
        s.to_owned()
    };
    while out.chars().count() < 2 {
        out.push('\u{200b}');
    }
    out
}

pub fn clamp(s: &str) -> String {
    clamp_len(s, 128)
}

pub fn human_count(n: u64) -> String {
    let f = n as f64;
    let (v, unit) = match n {
        0..1_000 => return n.to_string(),
        1_000..1_000_000 => (f / 1e3, "K"),
        1_000_000..1_000_000_000 => (f / 1e6, "M"),
        _ => (f / 1e9, "B"),
    };
    let s = format!("{v:.1}");
    format!("{}{unit}", s.strip_suffix(".0").unwrap_or(&s))
}

/// `costUsdTicks` → dollars. Unit assumed 1e-10 USD (unverified).
pub fn format_cost(ticks: u64) -> String {
    let usd = ticks as f64 / 1e10;
    if ticks == 0 {
        String::new()
    } else if usd < 0.01 {
        "<$0.01".into()
    } else {
        format!("${usd:.2}")
    }
}

/// Sessions eligible for display, most recently active first.
pub fn visible_sessions<'a>(state: &'a State, cfg: &Compiled, now_ms: u64) -> Vec<&'a Session> {
    let timeout_ms = cfg.cfg.idle_timeout_minutes.saturating_mul(60_000);
    let mut v: Vec<&Session> = state
        .sessions
        .values()
        .filter(|s| {
            !(cfg.cfg.idle_behavior == IdleBehavior::Clear
                && timeout_ms > 0
                && s.activity == Activity::Idle
                && now_ms.saturating_sub(s.last_event_ms) >= timeout_ms)
        })
        .collect();
    v.sort_by_key(|s| std::cmp::Reverse(s.last_event_ms));
    v
}

/// All template variables for `s`, already filtered by the privacy settings.
pub fn variables(
    s: &Session,
    visible: usize,
    cfg: &Compiled,
    ctx: &Context,
) -> HashMap<&'static str, String> {
    let p = &cfg.cfg.privacy;
    let hidden = glob::path_matches_any(&p.hidden_projects, &s.workspace);
    let mut v: HashMap<&'static str, String> = HashMap::new();
    fn put(v: &mut HashMap<&'static str, String>, k: &'static str, val: Option<String>) {
        if let Some(val) = val.filter(|x| !x.is_empty()) {
            v.insert(k, val);
        }
    }

    put(
        &mut v,
        "project",
        if hidden {
            Some(p.hidden_project_name.clone())
        } else if p.show_project {
            Some(s.project_name())
        } else {
            None
        },
    );
    let open = !hidden;
    put(
        &mut v,
        "branch",
        (open && p.show_branch).then(|| s.branch.clone()).flatten(),
    );
    put(
        &mut v,
        "model",
        p.show_model.then(|| s.summary.model.clone()).flatten(),
    );
    put(
        &mut v,
        "effort",
        p.show_model.then(|| s.summary.effort.clone()).flatten(),
    );
    let (file, command) = match &s.activity {
        Activity::Tool { file, command, .. } => (file.clone(), command.clone()),
        _ => (None, None),
    };
    put(
        &mut v,
        "file",
        (open && p.show_file_names).then_some(file).flatten(),
    );
    put(
        &mut v,
        "command",
        (open && p.show_commands).then_some(command).flatten(),
    );
    put(
        &mut v,
        "session_title",
        (open && p.show_session_title)
            .then(|| s.summary.title.clone())
            .flatten(),
    );
    put(
        &mut v,
        "mcp_server",
        s.activity.mcp_server().map(str::to_owned),
    );
    put(&mut v, "error", s.activity.error_text().map(str::to_owned));
    if p.show_stats {
        put(
            &mut v,
            "tokens",
            (s.usage.total_tokens > 0).then(|| human_count(s.usage.total_tokens)),
        );
        put(&mut v, "cost", Some(format_cost(s.usage.cost_ticks)));
        put(
            &mut v,
            "turns",
            (s.usage.turns > 0).then(|| s.usage.turns.to_string()),
        );
    }
    put(&mut v, "edits", Some(s.edits.to_string()));
    put(&mut v, "commands", Some(s.commands.to_string()));
    put(
        &mut v,
        "subagents",
        (s.subagents > 0).then(|| s.subagents.to_string()),
    );
    put(
        &mut v,
        "other_sessions",
        (visible > 1).then(|| (visible - 1).to_string()),
    );
    put(&mut v, "session_count", Some(visible.max(1).to_string()));
    // `{sessions}`: the active pack's line for 2, 3, or 4+ open sessions.
    let sessions_key = match visible {
        0 | 1 => None,
        2 => Some("sessions_2"),
        3 => Some("sessions_3"),
        _ => Some("sessions_many"),
    };
    let sessions = sessions_key
        .and_then(|k| cfg.labels.get(k))
        .map(|t| t.render(&|name| v.get(name).cloned()));
    put(&mut v, "sessions", sessions);
    put(&mut v, "grok_version", ctx.grok_version.map(str::to_owned));
    put(
        &mut v,
        "repo_url",
        open.then(|| s.repo.as_ref().map(|r| r.url.clone()))
            .flatten(),
    );
    put(
        &mut v,
        "repo_name",
        open.then(|| s.repo.as_ref().map(|r| r.name.clone()))
            .flatten(),
    );
    put(&mut v, "permission_mode", s.permission_mode.clone());
    put(&mut v, "state_key", Some(s.activity.key().to_owned()));
    put(&mut v, "activity_icon", Some(s.activity.icon().to_owned()));

    let label = cfg
        .labels
        .get(s.activity.key())
        .map(|t| t.render(&|name| v.get(name).cloned()));
    put(
        &mut v,
        "activity",
        label.or_else(|| Some(s.activity.label())),
    );
    v
}

pub fn render(state: &State, cfg: &Compiled, ctx: &Context) -> Option<Value> {
    let sessions = visible_sessions(state, cfg, ctx.now_ms);
    let s = *sessions.first()?;
    let vars = variables(s, sessions.len(), cfg, ctx);
    let lookup = |name: &str| vars.get(name).cloned();
    let t = cfg.display_for(s.activity.key());

    let mut activity = Map::new();
    activity.insert("type".into(), json!(cfg.cfg.activity_type.code()));
    match cfg.cfg.status_display {
        StatusDisplay::Name => {}
        StatusDisplay::State => {
            activity.insert("status_display_type".into(), json!(1));
        }
        StatusDisplay::Details => {
            activity.insert("status_display_type".into(), json!(2));
        }
    }
    for (key, tpl) in [("details", &t.details), ("state", &t.state)] {
        let text = tpl.render(&lookup);
        if !text.is_empty() {
            activity.insert(key.into(), json!(clamp(&text)));
        }
    }

    let start = match cfg.cfg.timestamp {
        TimestampMode::Session => Some(s.started_s),
        TimestampMode::Turn => s.turn_started_s,
        TimestampMode::None => None,
    };
    if let Some(start) = start {
        activity.insert("timestamps".into(), json!({ "start": start }));
    }

    let mut assets = Map::new();
    for (key, tpl, max) in [
        ("large_image", &t.large_image, 256),
        ("large_text", &t.large_text, 128),
        ("small_image", &t.small_image, 256),
        ("small_text", &t.small_text, 128),
    ] {
        let text = tpl.render(&lookup);
        if !text.is_empty() {
            assets.insert(key.into(), json!(clamp_len(&text, max)));
        }
    }
    if !assets.is_empty() {
        activity.insert("assets".into(), Value::Object(assets));
    }

    let buttons: Vec<Value> = cfg
        .buttons
        .iter()
        .filter_map(|(label, url)| {
            let (label, url) = (label.render(&lookup), url.render(&lookup));
            let valid =
                !label.is_empty() && (url.starts_with("https://") || url.starts_with("http://"));
            valid.then(|| json!({ "label": clamp_len(&label, 32), "url": clamp_len(&url, 512) }))
        })
        .collect();
    if !buttons.is_empty() {
        activity.insert("buttons".into(), Value::Array(buttons));
    }

    Some(Value::Object(activity))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::daemon::enrich::Repo;

    fn ctx(now_ms: u64) -> Context<'static> {
        Context {
            grok_version: Some("1.0.70"),
            now_ms,
        }
    }

    fn edit_state() -> State {
        let mut st = State::new("/nonexistent");
        st.apply(
            &json!({"hook_event_name": "PreToolUse", "sessionId": "a", "workspaceRoot": "/w/proj",
                    "toolName": "write", "toolInput": {"file_path": "/w/proj/secret.rs"}}),
            1_000,
        );
        let s = st.sessions.get_mut("a").unwrap();
        s.branch = Some("main".into());
        s.summary.model = Some("grok-4.7".into());
        s.summary.title = Some("Fix the secret thing".into());
        s.repo = Some(Repo {
            url: "https://github.com/o/proj".into(),
            name: "o/proj".into(),
        });
        s.usage.total_tokens = 1_288_793;
        s.usage.cost_ticks = 3_122_063_600;
        st
    }

    fn cfg(toml: &str) -> Compiled {
        Compiled::from_toml(toml).unwrap()
    }

    #[test]
    fn clamp_pads_and_truncates() {
        assert_eq!(clamp("x").chars().count(), 2);
        let c = clamp(&"a".repeat(300));
        assert_eq!(c.chars().count(), 128);
        assert!(c.ends_with('…'));
    }

    #[test]
    fn number_formatting() {
        assert_eq!(human_count(999), "999");
        assert_eq!(human_count(61_005), "61K");
        assert_eq!(human_count(1_288_793), "1.3M");
        assert_eq!(format_cost(202_497_200), "$0.02");
        assert_eq!(format_cost(0), "");
        assert_eq!(format_cost(10), "<$0.01");
    }

    #[test]
    fn defaults_show_project_and_activity_only() {
        let st = edit_state();
        let v = render(&st, &Compiled::defaults(), &ctx(2_000)).unwrap();
        assert_eq!(v["details"], "proj · main");
        assert_eq!(v["state"], "Editing code");
        assert_eq!(v["assets"]["large_text"], "Grok Build 1.0.70 · grok-4.7");
        assert_eq!(v["assets"]["small_image"], "edit");
        assert_eq!(v["type"], 0);
        assert!(v.get("buttons").is_none() && v.get("status_display_type").is_none());
        let text = v.to_string();
        for secret in ["secret.rs", "Fix the secret", "1.3M", "$0.31", "github.com"] {
            assert!(!text.contains(secret), "leaked {secret}: {text}");
        }
    }

    #[test]
    fn privacy_opt_ins() {
        let st = edit_state();
        let c = cfg(r#"
            [privacy]
            show_file_names = true
            show_stats = true
            show_session_title = true
            [display]
            details = "{session_title}"
            state = "{activity}[ · {tokens} tokens][ · {cost}]"
        "#);
        let v = render(&st, &c, &ctx(2_000)).unwrap();
        assert_eq!(v["details"], "Fix the secret thing");
        assert_eq!(v["state"], "Editing secret.rs · 1.3M tokens · $0.31");
    }

    #[test]
    fn hidden_projects_mask_everything_identifying() {
        let st = edit_state();
        let c = cfg(r#"
            [privacy]
            hidden_projects = ["/w"]
            show_file_names = true
            show_session_title = true
            [display]
            details = "{project}[ · {branch}][ · {session_title}]"
            [[buttons]]
            label = "Repo"
            url = "{repo_url}"
        "#);
        let v = render(&st, &c, &ctx(2_000)).unwrap();
        assert_eq!(v["details"], "a private project");
        assert_eq!(v["state"], "Editing code");
        assert!(v.get("buttons").is_none());
    }

    #[test]
    fn state_overrides_labels_buttons_and_modes() {
        let st = edit_state();
        let c = cfg(r#"
            activity_type = "watching"
            status_display = "state"
            timestamp = "none"
            [labels]
            edit = "Hacking on {file|something}"
            [display.states.edit]
            small_image = "https://example.com/pen.png"
            [[buttons]]
            label = "View {repo_name}"
            url = "{repo_url}"
        "#);
        let v = render(&st, &c, &ctx(2_000)).unwrap();
        assert_eq!(v["type"], 3);
        assert_eq!(v["status_display_type"], 1);
        assert!(v.get("timestamps").is_none());
        assert_eq!(v["state"], "Hacking on something");
        assert_eq!(v["assets"]["small_image"], "https://example.com/pen.png");
        assert_eq!(
            v["buttons"],
            json!([{"label": "View o/proj", "url": "https://github.com/o/proj"}])
        );
    }

    #[test]
    fn activity_line_does_not_repeat_file_or_command() {
        let st = edit_state();
        let c = cfg(r#"
            [privacy]
            show_file_names = true
            show_commands = true
            [display]
            details = "{activity}[{ file}][ {command}]"
            [labels]
            edit = "Fapping to {file}"
            terminal = "Cumming all over {command}"
        "#);
        let v = render(&st, &c, &ctx(2_000)).unwrap();
        assert_eq!(v["details"], "Fapping to secret.rs");
        assert_eq!(v["assets"]["small_text"], "Fapping to secret.rs");

        let mut st = State::new("/nonexistent");
        st.apply(
            &json!({"hook_event_name": "PreToolUse", "sessionId": "a", "workspaceRoot": "/w/proj",
                    "toolName": "run_terminal_command", "toolInput": {"command": "cargo test --offline"}}),
            1_000,
        );
        let v = render(&st, &c, &ctx(2_000)).unwrap();
        assert_eq!(v["details"], "Cumming all over cargo");
    }

    #[test]
    fn empty_templates_omit_fields() {
        let st = edit_state();
        let c = cfg("[display]\ndetails = \"\"\nsmall_image = \"\"\nsmall_text = \"\"");
        let v = render(&st, &c, &ctx(2_000)).unwrap();
        assert!(v.get("details").is_none());
        assert!(v["assets"].get("small_image").is_none());
    }

    #[test]
    fn sessions_line_follows_count_and_pack() {
        let mut st = edit_state();
        let add = |st: &mut State, id: &str| {
            st.apply(
                &json!({"hook_event_name": "UserPromptSubmit", "sessionId": id,
                        "workspaceRoot": "/w/other"}),
                500,
            )
        };
        let def = cfg("[display]\nstate = \"{project}[ · {sessions}]\"");
        let nsfw = cfg("preset = \"nsfw\"\n[display]\nstate = \"{project}[ · {sessions}]\"");
        let state = |st: &State, c: &Compiled| render(st, c, &ctx(2_000)).unwrap()["state"].clone();

        assert_eq!(state(&st, &def), "proj", "one session hides the section");
        add(&mut st, "b");
        assert_eq!(state(&st, &def), "proj · Double the session trouble");
        assert_eq!(state(&st, &nsfw), "proj · Taking two sessions at once");
        add(&mut st, "c");
        assert_eq!(state(&st, &def), "proj · Juggling three sessions");
        assert_eq!(state(&st, &nsfw), "proj · Three of me jerking in sync");
        add(&mut st, "d");
        add(&mut st, "e");
        assert_eq!(state(&st, &def), "proj · Running 5 sessions");
        assert_eq!(state(&st, &nsfw), "proj · 5 deep, already cumming");

        // A custom pack without session lines uses the default wording.
        let mine = cfg(
            "preset = \"mine\"\n[presets.mine]\nidle = \"zzz\"\n[display]\nstate = \"{sessions}\"",
        );
        assert_eq!(state(&st, &mine), "Running 5 sessions");
        // Labels may not reference {sessions} (or {activity}).
        assert!(Compiled::from_toml("[labels]\nsessions_2 = \"{sessions}\"").is_err());
        assert!(Compiled::from_toml("[labels]\nidle = \"{sessions}\"").is_err());
    }

    #[test]
    fn multiple_sessions_and_idle_timeout() {
        let mut st = edit_state();
        st.apply(
            &json!({"hook_event_name": "Stop", "reason": "end_turn", "sessionId": "b",
                         "workspaceRoot": "/w/other"}),
            500,
        );
        // b is idle and older; a is shown, b counted.
        let v = render(&st, &Compiled::defaults(), &ctx(2_000)).unwrap();
        assert_eq!(v["state"], "Editing code · +1 more");
        // After 15 idle minutes b is dropped (default idle_behavior = clear).
        let later = 500 + 15 * 60_000;
        let v = render(&st, &Compiled::defaults(), &ctx(later)).unwrap();
        assert_eq!(v["state"], "Editing code");
        // keep → still counted
        let keep = cfg("idle_behavior = \"keep\"");
        assert_eq!(
            render(&st, &keep, &ctx(later)).unwrap()["state"],
            "Editing code · +1 more"
        );
        // A lone idle session past the timeout clears the presence entirely.
        st.sessions.remove("a");
        assert!(render(&st, &Compiled::defaults(), &ctx(later)).is_none());
    }
}
