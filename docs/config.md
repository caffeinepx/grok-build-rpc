# Configuring grok-rpc

Config file: `~/.config/grok-rpc/config.toml`. You can point to a different file with `GROK_RPC_CONFIG`.

- `grok-rpc config init` writes a fully commented default file.
- `grok-rpc config check` validates the file and previews the presence for idle, thinking, edit, terminal, approval and error, using sample data from the current directory.
- `grok-rpc config path` prints the path.

The running daemon reloads the file about 0.5 s after you save it. If a save is invalid, the daemon keeps the last valid config, and `grok-rpc status` shows the error.

## Top-level settings
| Key | Values | Default |
|---|---|---|
| `client_id` | Discord application ID (its name is what shows after "Playing") | Grok app |
| `activity_type` | `playing` `listening` `watching` `competing` | `playing` |
| `status_display` | what the member list shows: `name` `state` `details` | `name` |
| `timestamp` | elapsed timer: `session` `turn` `none` | `session` |
| `idle_timeout_minutes` | minutes before an idle session times out (`0` = never) | `15` |
| `idle_behavior` | after the timeout: `clear` hides the session, `keep` keeps showing it | `clear` |

## Templates
- `{var}` inserts a variable.
- `{var|fallback}` uses the fallback text when the variable is empty.
- `[ … ]` is an optional section. It's dropped if any `{var}` directly inside it is empty.
- `{file}` and `{command}` are skipped when that same name is already on the line. `{activity}[ {file}]` stays `Editing main.rs` instead of `Editing main.rsmain.rs`. A name the activity sentence does not already contain is still appended.
- `\{ \} \[ \] \\` are literal characters. Inside "double-quoted" TOML strings, write the backslash twice (`\\[`).

`[display]` holds the fields `details`, `state`, `large_image`, `large_text`, `small_image` and `small_text`. A field that renders empty is left out.
Images can be asset keys uploaded to the Discord app (Rich Presence → Art Assets) or `https://` URLs.

`[display.states.<state>]` overrides any of those fields for one state. `[presets.<name>]` is a named pack of `{activity}` lines (`Idle`, `Reading {file|files}`, and the rest). `preset = "default"` selects the pack. `grok-rpc preset` lists them, and `grok-rpc preset <name>` switches. `[labels]` overrides one line of the active pack. A pack can also set `sessions_2`, `sessions_3` and `sessions_many`, the lines behind `{sessions}` (`sessions_many = "Running {session_count} sessions"`). Lines a pack leaves out use the `default` wording. Those lines are templates, but they can't use `{activity}` or `{sessions}`. `{tokens}`, `{cost}` and `{turns}` stay empty until `show_stats` is on. There is no context-window variable.

States: `idle thinking approval compacting error edit terminal read search web subagent plan question media mcp other`

## Variables
| Variable | Value | Privacy switch |
|---|---|---|
| `project` | workspace folder name (`hidden_project_name` for hidden projects) | `show_project` |
| `branch` | git branch | `show_branch` |
| `model`, `effort` | e.g. `grok-4.7`, `high` | `show_model` |
| `activity` | the label for the current state | – |
| `activity_icon`, `state_key` | e.g. `edit` | – |
| `file` | name of the file being edited or read (name only, no path) | `show_file_names` (off) |
| `command` | the program being run, e.g. `cargo`. Arguments are never shown | `show_commands` (off) |
| `mcp_server` | server name for MCP tools | – |
| `error` | "Rate limited", "Server error", … | – |
| `session_title` | Grok's generated session title | `show_session_title` (off) |
| `tokens`, `cost`, `turns` | session totals (`1.3M`, `$0.31`, `4`). The cost unit is still unverified | `show_stats` (off) |
| `edits`, `commands` | counts for this session | – |
| `subagents` | number of running subagents (empty when 0) | – |
| `other_sessions`, `session_count` | number of other open sessions (empty when 0), and the total | – |
| `sessions` | the active pack's `sessions_2` / `sessions_3` / `sessions_many` line (2, 3, 4+ open sessions); empty with one session, so `[ · {sessions}]` hides itself | – |
| `grok_version` | e.g. `1.0.70` | – |
| `repo_url`, `repo_name` | from the `origin` remote, as an https URL and `owner/name` | hidden for hidden projects |
| `permission_mode` | `default` `auto` `plan` `bypassPermissions` | – |

A project listed in `hidden_projects` always hides `branch`, `file`, `command`, `session_title`, `repo_url` and `repo_name`, whatever the switches above say. A plain path hides that folder and everything inside it. Globs can use `*`, `**` and `?`.

## Buttons
Add up to 2 `[[buttons]]` with `label` and `url` (both are templates). A button whose URL doesn't render to an `http(s)://` URL is skipped. Discord only shows buttons to other people, never to you.

## Example
```toml
activity_type = "watching"

[display]
details = "{project}[ ({branch})]"
state = "{activity}[ · {tokens} tokens]"

[display.states.idle]
state = "AFK"
small_image = ""

[labels]
terminal = "Running {command|a command}"

[privacy]
show_commands = true
show_stats = true
hidden_projects = ["~/work"]

[[buttons]]
label = "View {repo_name}"
url = "{repo_url}"
```

## Commands
`grok-rpc pause` hides the presence until `grok-rpc resume`. The daemon keeps tracking the session, and the choice survives a restart.
`grok-rpc preview` prints the rendered presence for sample states, the same way `config check` does.
`grok-rpc preset` lists the activity text packs. `grok-rpc preset <name>` switches the active one. The daemon reloads it on its own.
`grok-rpc install` writes `~/.grok/hooks/grok-rpc.json` (the absolute path of the binary you ran, `args: ["hook"]`, timeout 2 seconds). If you have no config yet, it also writes the default config. It does not enable anything by itself. Open a new Grok session afterwards, in a trusted folder. `install --force` overwrites the hook file. `install --systemd` also writes `~/.config/systemd/user/grok-rpc.service` and prints the `systemctl --user` lines. The hook already starts the daemon, so the unit is optional. A clean exit after 10 minutes with no sessions is not restarted.
`grok-rpc uninstall` removes the hook file and leaves your config alone. `uninstall --systemd` also removes the unit.
