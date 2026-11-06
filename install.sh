#!/bin/sh
# grok-rpc installer: Discord Rich Presence for Grok Build.
#
#   curl -fsSL https://raw.githubusercontent.com/caffeinepx/grok-build-rpc/main/install.sh | sh
#   ... | sh -s -- --systemd | --no-systemd | --version v0.1.0 | --uninstall
#
# Installs for the current user only (no sudo): binary in ~/.local/bin, Grok hook in
# ~/.grok/hooks/grok-rpc.json, config in ~/.config/grok-rpc/. Running it again updates.
set -eu

REPO="caffeinepx/grok-build-rpc"
BASE_URL="${GROK_RPC_BASE_URL:-https://github.com/$REPO/releases}"
BIN_DIR="${GROK_RPC_BIN_DIR:-$HOME/.local/bin}"
BIN="$BIN_DIR/grok-rpc"
UNIT="${XDG_CONFIG_HOME:-$HOME/.config}/systemd/user/grok-rpc.service"

VERSION="latest"
SYSTEMD="ask"
ACTION="install"

say() { printf '\033[1;35m›\033[0m %s\n' "$*"; }
warn() { printf '\033[1;33m!\033[0m %s\n' "$*" >&2; }
die() { printf '\033[1;31m✗\033[0m %s\n' "$*" >&2; exit 1; }

while [ $# -gt 0 ]; do
    case "$1" in
        --systemd) SYSTEMD="yes" ;;
        --no-systemd) SYSTEMD="no" ;;
        --uninstall) ACTION="uninstall" ;;
        --version)
            [ $# -ge 2 ] || die "--version needs a value, e.g. --version v0.1.0"
            VERSION="$2"
            shift
            ;;
        --version=*) VERSION="${1#--version=}" ;;
        -h | --help)
            cat <<'USAGE'
grok-rpc installer (Linux, per user, no sudo)

  --systemd         also start grok-rpc at login (systemd user unit)
  --no-systemd      don't ask; the daemon starts with Grok (removes an existing unit)
  --version vX.Y.Z  install a specific release (default: latest)
  --uninstall       remove the binary, hook and unit (config is kept)

  GROK_RPC_BIN_DIR  install directory (default: ~/.local/bin)
USAGE
            exit 0
            ;;
        *) die "unknown option: $1 (try --help)" ;;
    esac
    shift
done

have() { command -v "$1" >/dev/null 2>&1; }
has_systemd() { have systemctl && systemctl --user show-environment >/dev/null 2>&1; }

# Stops a daemon that is not managed by systemd, so the next Grok event starts the new binary.
stop_daemon() {
    exe="$1"
    [ -x "$exe" ] || return 0
    pid=$("$exe" status 2>/dev/null | sed -n 's/^daemon: *running (pid \([0-9]*\)).*/\1/p') || true
    if [ -n "${pid:-}" ]; then
        kill "$pid" 2>/dev/null && say "stopped the running daemon (pid $pid)" || true
    fi
}

uninstall() {
    if [ -f "$UNIT" ] && has_systemd; then
        systemctl --user disable --now grok-rpc.service >/dev/null 2>&1 || true
    fi
    if [ -x "$BIN" ]; then
        stop_daemon "$BIN"
        "$BIN" uninstall --systemd || true
        has_systemd && systemctl --user daemon-reload || true
        rm -f "$BIN"
        say "removed $BIN"
    else
        warn "$BIN not found; remove ~/.grok/hooks/grok-rpc.json by hand if it exists"
    fi
    say "your config was kept: ${XDG_CONFIG_HOME:-$HOME/.config}/grok-rpc/"
    say "grok-rpc is uninstalled. Bye 🩷"
}

download() { # url dest
    if have curl; then
        curl -fsSL --retry 3 -o "$2" "$1"
    elif have wget; then
        wget -q -O "$2" "$1"
    else
        die "need curl or wget"
    fi
}

install_bin() {
    [ "$(uname -s)" = "Linux" ] || die "grok-rpc supports Linux only"
    case "$(uname -m)" in
        x86_64 | amd64) arch="x86_64" ;;
        aarch64 | arm64) arch="aarch64" ;;
        *) die "unsupported CPU: $(uname -m) (x86_64 and aarch64 only)" ;;
    esac
    asset="grok-rpc-$arch-linux"
    if [ "$VERSION" = "latest" ]; then
        url="$BASE_URL/latest/download/$asset"
    else
        url="$BASE_URL/download/$VERSION/$asset"
    fi

    tmp=$(mktemp -d)
    trap 'rm -rf "$tmp"' EXIT INT TERM
    say "downloading $asset ($VERSION)"
    download "$url" "$tmp/$asset" || die "download failed: $url"
    download "$url.sha256" "$tmp/$asset.sha256" || die "checksum download failed: $url.sha256"
    if have sha256sum; then
        (cd "$tmp" && sha256sum -c "$asset.sha256" >/dev/null) || die "checksum mismatch, not installing"
    else
        warn "sha256sum not found; skipping checksum verification"
    fi

    mkdir -p "$BIN_DIR"
    # Stop an autospawned daemon still running an older binary (systemd is restarted below).
    if ! { [ -f "$UNIT" ] && has_systemd && systemctl --user is-active --quiet grok-rpc.service; }; then
        stop_daemon "$BIN"
    fi
    install -m 755 "$tmp/$asset" "$BIN"
    say "installed $("$BIN" --version) to $BIN"
}

setup() {
    # --force: the hook file is ours; this repoints it at $BIN (e.g. after a move).
    "$BIN" install --force | sed 's/^/  /'

    want="$SYSTEMD"
    if [ -f "$UNIT" ] && [ "$want" = "ask" ]; then
        want="yes" # already using systemd: keep it, with the refreshed unit
    fi
    if [ "$want" = "ask" ]; then
        want="no"
        if has_systemd && [ -r /dev/tty ] && [ -w /dev/tty ]; then
            printf 'Start grok-rpc at login with systemd? Otherwise it starts with Grok. [y/N] ' >/dev/tty
            read -r answer </dev/tty || answer=""
            case "$answer" in y | Y | yes | YES) want="yes" ;; esac
        fi
    fi

    if [ "$want" = "yes" ]; then
        has_systemd || die "systemd user session not available; rerun with --no-systemd"
        "$BIN" install --systemd >/dev/null
        systemctl --user daemon-reload
        systemctl --user enable grok-rpc.service >/dev/null 2>&1
        systemctl --user restart grok-rpc.service
        say "systemd: grok-rpc.service enabled and running"
    elif [ "$SYSTEMD" = "no" ] && [ -f "$UNIT" ]; then
        if has_systemd; then
            systemctl --user disable --now grok-rpc.service >/dev/null 2>&1 || true
        fi
        rm -f "$UNIT"
        has_systemd && systemctl --user daemon-reload || true
        say "systemd: unit removed; the daemon now starts with Grok"
    fi

    case ":$PATH:" in
        *":$BIN_DIR:"*) ;;
        *) warn "$BIN_DIR is not on your PATH. Add it to use the grok-rpc command (the Grok hook works either way)." ;;
    esac
    [ -d "${GROK_HOME:-$HOME/.grok}" ] || warn "no ~/.grok found. Is Grok Build installed? The hook is in place for when it is."

    echo
    say "done! Open a new Grok session and check your Discord profile ✨"
    say "status: grok-rpc status · presets: grok-rpc preset · config: grok-rpc config path"
}

if [ "$ACTION" = "uninstall" ]; then
    uninstall
else
    install_bin
    setup
fi
