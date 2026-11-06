<div align="center">

<picture>
  <source media="(prefers-color-scheme: dark)" srcset="docs/logo/logo-white.png">
  <source media="(prefers-color-scheme: light)" srcset="docs/logo/logo-black.png">
  <img src="docs/logo/logo-black.png" width="120" alt="Grok Build RPC logo">
</picture>

# Grok Build RPC

**Show what [Grok Build](https://x.ai) is doing on your Discord profile.**

<a href="https://github.com/caffeinepx/grok-build-rpc/actions/workflows/ci.yml"><img alt="CI" src="https://img.shields.io/github/actions/workflow/status/caffeinepx/grok-build-rpc/ci.yml?branch=main&label=ci"></a>
<a href="https://github.com/caffeinepx/grok-build-rpc/releases/latest"><img alt="Latest release" src="https://img.shields.io/github/v/release/caffeinepx/grok-build-rpc?label=release"></a>
<a href="LICENSE"><img alt="MIT license" src="https://img.shields.io/github/license/caffeinepx/grok-build-rpc"></a>
<img alt="Linux" src="https://img.shields.io/badge/platform-Linux-FCC624?logo=linux&logoColor=black">
<img alt="Rust" src="https://img.shields.io/badge/built%20with-Rust-dea584?logo=rust&logoColor=white">

<br><br>

<img src="docs/screenshots/thinking-unhinged.png" width="400" alt="Thinking (unhinged preset)">
<img src="docs/screenshots/compacting-unhinged.png" width="400" alt="Compacting (unhinged preset)">
<img src="docs/screenshots/reading-default.png" width="400" alt="Reading files (default preset)">
<img src="docs/screenshots/searching-nsfw.png" width="400" alt="Searching with two sessions (nsfw preset)">

</div>

Your project, branch and what Grok is doing right now (thinking, editing, running a command…) show up live on Discord. It clears itself when Grok exits.

## Install

Needs Linux (x86_64 or arm64), Grok Build and the Discord desktop app.

```sh
curl -fsSL https://raw.githubusercontent.com/caffeinepx/grok-build-rpc/main/install.sh | sh
```

Then open a new Grok session. That's it.

The script installs to `~/.local/bin` without sudo and asks whether to start at login with systemd. Add `sh -s -- --systemd`, `--version v0.1.0` or `--uninstall` to skip the question or do something else. You can also grab a binary from [Releases](https://github.com/caffeinepx/grok-build-rpc/releases) or build it with `cargo install --git https://github.com/caffeinepx/grok-build-rpc`, then run `grok-rpc install`.

## Presets

Four text packs come built in: `default`, `unhinged`, `mommy` and `nsfw`. The last three are 18+, and `default` stays on until you switch.

```sh
grok-rpc preset           # list packs
grok-rpc preset unhinged  # switch (applies immediately)
```

## Configure

Edit `~/.config/grok-rpc/config.toml`. Changes apply when you save.

```toml
[display]
details = "{activity}[ · {sessions}]"
state   = "{project}[ · {branch}]"

[privacy]
show_file_names = true   # "Editing main.rs" instead of "Editing code"
```

By default only the project, branch, model and kind of activity are shown. File names, command names, token stats and session titles are opt-in. Full commands and prompts are never sent.

See **[docs/config.md](docs/config.md)** for every variable, custom presets, buttons and hidden projects.

## Commands

```sh
grok-rpc status          # what's running and what Discord is showing
grok-rpc pause / resume  # hide or show the presence
grok-rpc preview         # see every state with your config
grok-rpc config check    # validate the config
```

Nothing showing up? Run `grok-rpc status`, and check Discord's *Settings → Activity Privacy → Share your detected activities*. Logs are in `~/.local/state/grok-rpc/daemon.log`.

## License

[MIT](LICENSE). Icons from [Lucide](https://lucide.dev) (ISC). Not affiliated with xAI or Discord.
