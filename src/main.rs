//! grok-rpc: Discord Rich Presence for Grok Build.

mod config;
mod daemon;
mod discord;
mod glob;
mod hook;
mod install;
mod log;
mod paths;
mod presence;
mod template;

use clap::{Parser, Subcommand};
use std::io::{Read, Write};
use std::net::Shutdown;
use std::os::unix::net::UnixStream;
use std::process::ExitCode;

#[derive(Parser)]
#[command(version, about = "Discord Rich Presence for Grok Build")]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Called by Grok Build hooks: forwards the event on stdin to the daemon. Always exits 0.
    Hook,
    /// Run the presence daemon in the foreground (normally auto-started by `hook`).
    Daemon {
        /// Keep running with no sessions instead of exiting after 10 min (systemd).
        #[arg(long)]
        no_idle_exit: bool,
    },
    /// Start the daemon in the background if it isn't running.
    Start,
    /// Show daemon, Discord connection, sessions and the current presence.
    Status,
    /// Hide Discord presence until `resume`. Session tracking keeps going.
    Pause,
    /// Show Discord presence again.
    Resume,
    /// Print the presence the current config would show for sample states.
    Preview,
    /// List activity text packs, or switch to one (`grok-rpc preset unhinged`).
    Preset {
        /// Pack name. Omit to list packs.
        name: Option<String>,
    },
    /// Write the global Grok hook and a default config if you don't have one.
    Install {
        /// Overwrite an existing hook file.
        #[arg(long)]
        force: bool,
        /// Also write a systemd user unit. Does not enable it.
        #[arg(long)]
        systemd: bool,
    },
    /// Remove the global Grok hook written by `install`.
    Uninstall {
        /// Also remove the systemd user unit.
        #[arg(long)]
        systemd: bool,
    },
    /// Manage the config file.
    #[command(subcommand)]
    Config(ConfigCmd),
}

#[derive(Subcommand)]
enum ConfigCmd {
    /// Print the config file path.
    Path,
    /// Write a documented default config file.
    Init {
        /// Overwrite an existing file.
        #[arg(long)]
        force: bool,
    },
    /// Validate the config and preview the presence for sample states.
    Check,
}

fn main() -> ExitCode {
    // Parse `hook` before clap so a bad argv can never make the hook exit non-zero.
    if std::env::args().nth(1).as_deref() == Some("hook") {
        hook::run();
        return ExitCode::SUCCESS;
    }
    match Cli::parse().cmd {
        Cmd::Hook => unreachable!(),
        Cmd::Daemon { no_idle_exit } => match daemon::run(!no_idle_exit) {
            Ok(()) => ExitCode::SUCCESS,
            Err(e) => {
                eprintln!("grok-rpc daemon: {e}");
                ExitCode::FAILURE
            }
        },
        Cmd::Pause => set_pause(true),
        Cmd::Resume => set_pause(false),
        Cmd::Preview => preview_cmd(),
        Cmd::Preset { name } => preset_cmd(name),
        Cmd::Install { force, systemd } => install_cmd(force, systemd),
        Cmd::Uninstall { systemd } => uninstall_cmd(systemd),
        Cmd::Start => {
            if UnixStream::connect(paths::socket_path()).is_ok() {
                println!("daemon already running");
                return ExitCode::SUCCESS;
            }
            match hook::spawn_daemon() {
                Ok(()) => {
                    println!("daemon started (log: {})", paths::log_path().display());
                    ExitCode::SUCCESS
                }
                Err(e) => {
                    eprintln!("failed to start daemon: {e}");
                    ExitCode::FAILURE
                }
            }
        }
        Cmd::Status => status(),
        Cmd::Config(c) => config_cmd(c),
    }
}

fn config_cmd(cmd: ConfigCmd) -> ExitCode {
    let path = config::config_path();
    match cmd {
        ConfigCmd::Path => {
            println!("{}", path.display());
            ExitCode::SUCCESS
        }
        ConfigCmd::Init { force } => {
            if path.exists() && !force {
                eprintln!(
                    "{} already exists (use --force to overwrite)",
                    path.display()
                );
                return ExitCode::FAILURE;
            }
            let res = path
                .parent()
                .map_or(Ok(()), std::fs::create_dir_all)
                .and_then(|()| std::fs::write(&path, config::DEFAULT_TOML));
            match res {
                Ok(()) => {
                    println!("wrote {}", path.display());
                    ExitCode::SUCCESS
                }
                Err(e) => {
                    eprintln!("failed to write {}: {e}", path.display());
                    ExitCode::FAILURE
                }
            }
        }
        ConfigCmd::Check => match config::load(&path) {
            Err(e) => {
                eprintln!("✗ {e}");
                ExitCode::FAILURE
            }
            Ok(cfg) => {
                if path.exists() {
                    println!("✓ {} is valid\n", path.display());
                } else {
                    println!("• {} not found; showing defaults\n", path.display());
                }
                preview(&cfg);
                ExitCode::SUCCESS
            }
        },
    }
}

/// Renders the presence for a sample session in the current directory across several states.
fn preview(cfg: &config::Compiled) {
    use serde_json::json;
    let cwd = std::env::current_dir()
        .unwrap_or_default()
        .display()
        .to_string();
    let grok_home = paths::grok_home();
    let version = daemon::enrich::grok_version(&grok_home);
    let base =
        |name: &str| json!({"hook_event_name": name, "sessionId": "preview", "workspaceRoot": cwd});
    let mut tool = base("PreToolUse");
    tool["toolName"] = json!("search_replace");
    tool["toolInput"] = json!({"file_path": format!("{cwd}/src/main.rs")});
    let mut term = base("PreToolUse");
    term["toolName"] = json!("run_terminal_command");
    term["toolInput"] = json!({"command": "cargo test --release"});
    let mut fail = base("StopFailure");
    fail["error"] = json!("rate_limit");
    let mut idle = base("Stop");
    idle["reason"] = json!("end_turn");
    let samples = [
        ("idle", idle),
        ("thinking", base("UserPromptSubmit")),
        ("edit", tool),
        ("terminal", term),
        ("approval", {
            let mut n = base("Notification");
            n["notificationType"] = json!("permission_prompt");
            n
        }),
        ("error", fail),
    ];

    let mut state = daemon::state::State::new(grok_home);
    let now = daemon::now_ms();
    for (key, ev) in samples {
        state.apply(&ev, now);
        if let Some(s) = state.sessions.get_mut("preview") {
            s.summary.model.get_or_insert_with(|| "grok-4.7".into());
            s.summary.effort.get_or_insert_with(|| "high".into());
            s.summary
                .title
                .get_or_insert_with(|| "Sample session title".into());
            s.usage.total_tokens = s.usage.total_tokens.max(1_288_793);
            s.usage.cost_ticks = s.usage.cost_ticks.max(3_122_063_600);
            s.usage.turns = s.usage.turns.max(4);
        }
        let ctx = presence::Context {
            grok_version: version.as_deref(),
            now_ms: now,
        };
        println!("── {key} ──");
        match presence::render(&state, cfg, &ctx) {
            None => println!("  (presence cleared)"),
            Some(v) => {
                let g = |k: &str| v.get(k).and_then(|x| x.as_str()).unwrap_or("");
                let a = |k: &str| v["assets"].get(k).and_then(|x| x.as_str()).unwrap_or("");
                println!("  details:     {}", g("details"));
                println!("  state:       {}", g("state"));
                println!("  large image: {}  ({})", a("large_image"), a("large_text"));
                println!("  small image: {}  ({})", a("small_image"), a("small_text"));
                if let Some(b) = v.get("buttons") {
                    println!("  buttons:     {b}");
                }
            }
        }
    }
}

fn preset_cmd(name: Option<String>) -> ExitCode {
    let path = config::config_path();
    let src = match std::fs::read_to_string(&path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("{}: {e}", path.display());
            return ExitCode::FAILURE;
        }
    };
    let cfg: config::Config = match toml::from_str(&src) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("{}: {e}", path.display());
            return ExitCode::FAILURE;
        }
    };
    let Some(name) = name else {
        println!("active: {}", cfg.preset);
        let mut names: Vec<&String> = cfg.presets.keys().collect();
        names.sort();
        for n in names {
            println!("  {n}");
        }
        return ExitCode::SUCCESS;
    };
    if !cfg.presets.contains_key(&name) {
        eprintln!("unknown preset {name:?}");
        let mut names: Vec<&String> = cfg.presets.keys().collect();
        names.sort();
        eprintln!(
            "available: {}",
            names
                .iter()
                .map(|s| s.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        );
        return ExitCode::FAILURE;
    }
    match config::set_preset_line(&src, &name) {
        Ok(out) => {
            if let Err(e) = std::fs::write(&path, out) {
                eprintln!("{}: {e}", path.display());
                return ExitCode::FAILURE;
            }
            println!("preset: {name}");
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("{e}");
            ExitCode::FAILURE
        }
    }
}

fn preview_cmd() -> ExitCode {
    match config::load(&config::config_path()) {
        Err(e) => {
            eprintln!("✗ {e}");
            ExitCode::FAILURE
        }
        Ok(cfg) => {
            preview(&cfg);
            ExitCode::SUCCESS
        }
    }
}

fn set_pause(on: bool) -> ExitCode {
    if let Err(e) = daemon::set_paused_flag(on) {
        eprintln!("failed to save pause state: {e}");
        return ExitCode::FAILURE;
    }
    let sock = paths::socket_path();
    let Ok(mut stream) = UnixStream::connect(&sock) else {
        println!(
            "daemon not running; presence will be {} when it starts",
            if on { "hidden" } else { "shown" }
        );
        return ExitCode::SUCCESS;
    };
    let word = if on { "PAUSE" } else { "RESUME" };
    let mut out = String::new();
    let ok = stream.write_all(word.as_bytes()).is_ok()
        && stream.shutdown(Shutdown::Write).is_ok()
        && stream.read_to_string(&mut out).is_ok();
    if ok {
        println!("presence {}", if on { "paused" } else { "resumed" });
        ExitCode::SUCCESS
    } else {
        eprintln!("daemon did not confirm; the saved flag applies the next time it starts");
        ExitCode::FAILURE
    }
}

fn install_cmd(force: bool, systemd: bool) -> ExitCode {
    let exe = match std::env::current_exe() {
        Ok(p) => p.display().to_string(),
        Err(e) => {
            eprintln!("cannot find this executable: {e}");
            return ExitCode::FAILURE;
        }
    };
    let hook = paths::grok_home().join("hooks/grok-rpc.json");
    match install::write_hook(&hook, &exe, force) {
        Ok(true) => println!("wrote {}", hook.display()),
        Ok(false) => println!("hook already installed ({})", hook.display()),
        Err(e) => {
            eprintln!("{e}");
            return ExitCode::FAILURE;
        }
    }

    let cfg = config::config_path();
    if !cfg.exists() {
        let wrote = cfg
            .parent()
            .map_or(Ok(()), std::fs::create_dir_all)
            .and_then(|()| std::fs::write(&cfg, config::DEFAULT_TOML));
        if let Err(e) = wrote {
            eprintln!("hook installed, but config was not written: {e}");
            return ExitCode::FAILURE;
        }
        println!("wrote {}", cfg.display());
    }

    if systemd {
        let unit = install::systemd_unit_path();
        if let Err(e) = install::write_unit(&unit, &exe) {
            eprintln!("{e}");
            return ExitCode::FAILURE;
        }
        println!("wrote {}", unit.display());
        println!("enable it with:");
        println!("  systemctl --user daemon-reload");
        println!("  systemctl --user enable --now grok-rpc.service");
    }

    println!("open a new Grok session to load the hook. The folder has to be trusted.");
    ExitCode::SUCCESS
}

fn uninstall_cmd(systemd: bool) -> ExitCode {
    let hook = paths::grok_home().join("hooks/grok-rpc.json");
    match install::remove_path(&hook) {
        Ok(true) => println!("removed {}", hook.display()),
        Ok(false) => println!("hook not installed ({})", hook.display()),
        Err(e) => {
            eprintln!("{e}");
            return ExitCode::FAILURE;
        }
    }
    if systemd {
        let unit = install::systemd_unit_path();
        let wants = unit
            .parent()
            .map(|d| d.join("default.target.wants/grok-rpc.service"));
        if let Some(wants) = wants {
            match install::remove_path(&wants) {
                Ok(true) => println!("removed {}", wants.display()),
                Ok(false) => {}
                Err(e) => {
                    eprintln!("{e}");
                    return ExitCode::FAILURE;
                }
            }
        }
        match install::remove_path(&unit) {
            Ok(true) => {
                println!("removed {}", unit.display());
                println!("run: systemctl --user daemon-reload");
            }
            Ok(false) => println!("systemd unit not installed ({})", unit.display()),
            Err(e) => {
                eprintln!("{e}");
                return ExitCode::FAILURE;
            }
        }
    }
    ExitCode::SUCCESS
}

fn status() -> ExitCode {
    let sock = paths::socket_path();
    let Ok(mut s) = UnixStream::connect(&sock) else {
        println!("daemon:   not running (socket {})", sock.display());
        return ExitCode::FAILURE;
    };
    let mut out = String::new();
    let ok = s.write_all(b"STATUS").is_ok()
        && s.shutdown(Shutdown::Write).is_ok()
        && s.read_to_string(&mut out).is_ok();
    print!("{out}");
    if ok {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}
