#!/usr/bin/env bash
# Play Anything one-line installer for Linux and macOS. No Rust toolchain required.
set -euo pipefail

REPOSITORY='360internshala-oss/Play-Anything'
RELEASE_BASE="${PLAY_ANYTHING_RELEASE_BASE:-https://github.com/${REPOSITORY}/releases/latest/download}"
SOURCE_BASE="${PLAY_ANYTHING_SOURCE_BASE:-https://raw.githubusercontent.com/${REPOSITORY}/main}"
INSTALL_HOME="${PLAY_ANYTHING_HOME:-$HOME/.local/share/play-anything}"
SKIP_MEDIA="${PLAY_ANYTHING_SKIP_MEDIA:-0}"

log() {
    printf '\033[1;36m[play-anything]\033[0m %s\n' "$*"
}

fail() {
    printf '\033[1;31m[play-anything]\033[0m %s\n' "$*" >&2
    exit 1
}

download() {
    local url="$1"
    local destination="$2"
    mkdir -p "$(dirname "$destination")"
    log "Downloading $(basename "$destination")"
    curl -fL --retry 3 --connect-timeout 20 --progress-bar \
        "$url" -o "$destination.part"
    mv "$destination.part" "$destination"
}

path_contains() {
    case ":$PATH:" in
        *":$1:"*) return 0 ;;
        *) return 1 ;;
    esac
}

command -v curl >/dev/null 2>&1 || fail 'curl is required to download Play Anything'
command -v uname >/dev/null 2>&1 || fail 'uname is required to detect this platform'

case "$(uname -s)" in
    Linux)
        if [[ -n "${TERMUX_VERSION:-}" || "${PREFIX:-}" == *com.termux* ]]; then
            platform='termux'
        else
            platform='linux'
        fi
        ;;
    Darwin) platform='macos' ;;
    *) fail "Unsupported Unix platform: $(uname -s). Use installation/install.ps1 on Windows." ;;
esac
case "$(uname -m)" in
    x86_64|amd64) architecture='x86_64' ;;
    aarch64|arm64) architecture='aarch64' ;;
    armv7l|armv8l|arm)
        [[ "$platform" == 'termux' ]] || fail "Unsupported Linux CPU architecture: $(uname -m)"
        architecture='armv7'
        ;;
    i686|i586|x86)
        [[ "$platform" == 'termux' ]] || fail "Unsupported Linux CPU architecture: $(uname -m)"
        architecture='i686'
        ;;
    *) fail "Unsupported CPU architecture: $(uname -m)" ;;
esac

TEMP_DIRECTORY="$(mktemp -d)"
trap 'rm -rf "$TEMP_DIRECTORY"' EXIT
mkdir -p "$INSTALL_HOME/bin" "$INSTALL_HOME/scripts"

log "Installing Play Anything for $platform/$architecture"
download \
    "$RELEASE_BASE/play-anything-$platform-$architecture" \
    "$INSTALL_HOME/bin/play-anything"
chmod +x "$INSTALL_HOME/bin/play-anything"

# The media-only bootstrap never installs Rust. It reuses the same verified setup as source builds.
download "$SOURCE_BASE/scripts/bootstrap.sh" "$INSTALL_HOME/scripts/bootstrap.sh"
download "$SOURCE_BASE/scripts/env.sh" "$INSTALL_HOME/scripts/env.sh"
chmod +x "$INSTALL_HOME/scripts/bootstrap.sh" "$INSTALL_HOME/scripts/env.sh"
if [[ "$SKIP_MEDIA" != '1' ]]; then
    PLAY_ANYTHING_TOOLS="$INSTALL_HOME/.tools" \
        "$INSTALL_HOME/scripts/bootstrap.sh" --media-only
fi

if [[ "$platform" == 'macos' ]]; then
    xattr -d com.apple.quarantine "$INSTALL_HOME/bin/play-anything" 2>/dev/null || true
fi
"$INSTALL_HOME/bin/play-anything" --version >/dev/null || \
    fail 'The downloaded application could not start on this platform'

cat >"$TEMP_DIRECTORY/play-anything" <<EOF
#!/usr/bin/env bash
# Play Anything managed command
export PLAY_ANYTHING_ROOT="$INSTALL_HOME"
export PLAY_ANYTHING_TOOLS="$INSTALL_HOME/.tools"
exec "$INSTALL_HOME/bin/play-anything" "\$@"
EOF
chmod +x "$TEMP_DIRECTORY/play-anything"

COMMAND_DIR="${PLAY_ANYTHING_COMMAND_DIR:-}"
ELEVATE=''
if [[ -z "$COMMAND_DIR" ]]; then
    if [[ "$platform" == 'termux' && -n "${PREFIX:-}" ]] && path_contains "$PREFIX/bin"; then
        COMMAND_DIR="$PREFIX/bin"
    elif path_contains "$HOME/.local/bin"; then
        COMMAND_DIR="$HOME/.local/bin"
    elif [[ -d /usr/local/bin && -w /usr/local/bin ]]; then
        COMMAND_DIR='/usr/local/bin'
    elif command -v sudo >/dev/null 2>&1 && path_contains '/usr/local/bin'; then
        COMMAND_DIR='/usr/local/bin'
        ELEVATE='sudo'
    else
        COMMAND_DIR="$HOME/.local/bin"
    fi
fi
if ! mkdir -p "$COMMAND_DIR" 2>/dev/null; then
    command -v sudo >/dev/null 2>&1 || fail "Cannot create $COMMAND_DIR"
    ELEVATE='sudo'
    $ELEVATE mkdir -p "$COMMAND_DIR"
fi
if [[ ! -w "$COMMAND_DIR" ]]; then
    command -v sudo >/dev/null 2>&1 || fail "Cannot write to $COMMAND_DIR"
    ELEVATE='sudo'
fi

$ELEVATE install -m 755 "$TEMP_DIRECTORY/play-anything" "$COMMAND_DIR/play-anything"
# Windows is handled by install.ps1. On case-insensitive macOS filesystems, the lowercase
# command already resolves both spellings; on Linux this creates the requested second alias.
$ELEVATE ln -sf play-anything "$COMMAND_DIR/Play-Anything" 2>/dev/null || true

if ! path_contains "$COMMAND_DIR"; then
    path_line="export PATH=\"$COMMAND_DIR:\$PATH\""
    for profile in "$HOME/.profile" "$HOME/.bashrc" "$HOME/.zshrc"; do
        if [[ -f "$profile" || "$profile" == "$HOME/.profile" ]]; then
            if ! grep -F "$COMMAND_DIR" "$profile" >/dev/null 2>&1; then
                printf '\n# Play Anything\n%s\n' "$path_line" >>"$profile"
            fi
        fi
    done
    log "Installed. Restart your terminal, then run: play-anything"
else
    log 'Installed successfully. Run: play-anything'
fi
log 'Both play-anything and Play-Anything open the full interactive TUI.'
