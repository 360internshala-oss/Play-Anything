#!/usr/bin/env bash
# Install every build/runtime dependency into Play Anything's ignored .tools directory.
# End-user media setup never installs Rust. Portable runtimes are preferred on desktop Linux;
# Termux and musl systems use their native package managers automatically.
set -euo pipefail

ORIGINAL_PATH="$PATH"
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=scripts/env.sh
source "$SCRIPT_DIR/env.sh"

readonly MPV_RELEASE='v0.41.0@2026-10-07_1791382432'
readonly YTDLP_RELEASE='2026.10.21'
readonly FFMPEG_RELEASE='n8.1-latest'

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
    [[ "${PLAY_ANYTHING_OFFLINE:-0}" != '1' ]] || fail "Offline mode is enabled; missing $destination"
    mkdir -p "$(dirname "$destination")"
    log "Downloading $(basename "$destination")"
    curl -fL --retry 3 --connect-timeout 20 --progress-bar \
        "$url" -o "$destination.part"
    mv "$destination.part" "$destination"
}

is_termux() {
    [[ -n "${TERMUX_VERSION:-}" || "${PREFIX:-}" == *com.termux* ]]
}

is_musl() {
    if compgen -G '/lib/ld-musl-*' >/dev/null || compgen -G '/usr/lib/ld-musl-*' >/dev/null; then
        return 0
    fi
    command -v ldd >/dev/null 2>&1 && ldd --version 2>&1 | grep -qi musl
}

system_binary() {
    local name="$1"
    local directory absolute_directory
    local -a original_directories=()
    IFS=':' read -r -a original_directories <<<"$ORIGINAL_PATH"
    for directory in "${original_directories[@]}" \
        /usr/local/bin /usr/bin /bin /opt/homebrew/bin "${PREFIX:-}/bin"; do
        [[ -n "$directory" ]] || continue
        absolute_directory="$(cd "$directory" 2>/dev/null && pwd)" || continue
        [[ "$absolute_directory" != "$PLAY_ANYTHING_TOOLS/bin" ]] || continue
        if [[ -x "$absolute_directory/$name" ]]; then
            printf '%s\n' "$absolute_directory/$name"
            return 0
        fi
    done
    return 1
}

link_system_binary() {
    local name="$1"
    local binary
    binary="$(system_binary "$name")" || return 1
    ln -sfn "$binary" "$PLAY_ANYTHING_TOOLS/bin/$name"
}

run_as_root() {
    if [[ "$(id -u)" == '0' ]]; then
        "$@"
    elif command -v sudo >/dev/null 2>&1; then
        sudo "$@"
    else
        fail 'Media package installation needs root access or sudo. No Rust installation is needed.'
    fi
}

install_termux_tools() {
    command -v pkg >/dev/null 2>&1 || fail 'Termux pkg was not found. Run this installer inside the Termux app.'
    log 'Installing native Termux media backends (no AppImage or Rust)'
    pkg install -y mpv ffmpeg python-yt-dlp
    local name
    for name in mpv ffmpeg ffprobe yt-dlp; do
        link_system_binary "$name" || fail "Termux package installation did not provide $name"
    done
}

install_linux_system_tools() {
    log 'Installing native Linux media backends with the available package manager'
    if command -v apt-get >/dev/null 2>&1; then
        run_as_root apt-get update
        run_as_root env DEBIAN_FRONTEND=noninteractive apt-get install -y mpv ffmpeg
        run_as_root env DEBIAN_FRONTEND=noninteractive apt-get install -y yt-dlp || \
            log 'Distribution yt-dlp is unavailable; trying the managed standalone build'
    elif command -v dnf >/dev/null 2>&1; then
        run_as_root dnf install -y mpv ffmpeg
        run_as_root dnf install -y yt-dlp || true
    elif command -v yum >/dev/null 2>&1; then
        run_as_root yum install -y mpv ffmpeg
        run_as_root yum install -y yt-dlp || true
    elif command -v pacman >/dev/null 2>&1; then
        run_as_root pacman -S --needed --noconfirm mpv ffmpeg yt-dlp
    elif command -v zypper >/dev/null 2>&1; then
        run_as_root zypper --non-interactive install mpv ffmpeg
        run_as_root zypper --non-interactive install yt-dlp || true
    elif command -v apk >/dev/null 2>&1; then
        run_as_root apk add --no-cache mpv ffmpeg yt-dlp
    elif command -v xbps-install >/dev/null 2>&1; then
        run_as_root xbps-install -Sy mpv ffmpeg yt-dlp
    else
        fail 'No supported package manager found (apt, dnf, yum, pacman, zypper, apk, xbps, or Termux pkg).'
    fi

    local name
    for name in mpv ffmpeg ffprobe yt-dlp; do
        link_system_binary "$name" || true
    done
}

ensure_rust() {
    if ! command -v cargo >/dev/null 2>&1 || ! command -v rustc >/dev/null 2>&1; then
        command -v curl >/dev/null 2>&1 || fail 'curl is required for the one-time Rust bootstrap'
        log 'Installing a project-local Rust toolchain (stable, minimal profile)'
        export CARGO_HOME="$PLAY_ANYTHING_TOOLS/cargo-home"
        export RUSTUP_HOME="$PLAY_ANYTHING_TOOLS/rustup"
        mkdir -p "$CARGO_HOME" "$RUSTUP_HOME" "$PLAY_ANYTHING_TOOLS/downloads"
        download 'https://sh.rustup.rs' "$PLAY_ANYTHING_TOOLS/downloads/rustup-init.sh"
        sh "$PLAY_ANYTHING_TOOLS/downloads/rustup-init.sh" \
            -y --no-modify-path --profile minimal --default-toolchain stable
        export PATH="$CARGO_HOME/bin:$PATH"
    fi
    log "Rust ready: $(rustc --version)"
}

linux_arch() {
    case "$(uname -m)" in
        x86_64|amd64) printf 'x86_64' ;;
        aarch64|arm64) printf 'aarch64' ;;
        *) fail "This portable media runtime supports Linux x86_64/aarch64; use a native package manager on $(uname -m)." ;;
    esac
}

install_mpv_linux() {
    local arch release_url escaped_release archive runtime
    arch="$(linux_arch)"
    escaped_release="${MPV_RELEASE/@/%40}"
    release_url="https://github.com/pkgforge-dev/mpv-AppImage/releases/download/${escaped_release}/mpv-${MPV_RELEASE%%@*}-anylinux-${arch}.AppImage"
    archive="$PLAY_ANYTHING_TOOLS/downloads/mpv-${arch}.AppImage"
    runtime="$PLAY_ANYTHING_TOOLS/mpv-runtime"

    [[ -f "$archive" ]] || download "$release_url" "$archive"
    chmod +x "$archive"
    log 'Unpacking project-local mpv'
    rm -rf "$runtime.new"
    mkdir -p "$runtime.new"
    if ! (cd "$runtime.new" && "$archive" --appimage-extract >/dev/null); then
        rm -rf "$runtime.new"
        log 'This system cannot execute the mpv AppImage; using native Linux packages instead'
        install_linux_system_tools
        return
    fi
    rm -rf "$runtime"
    mv "$runtime.new" "$runtime"

    cat >"$PLAY_ANYTHING_TOOLS/bin/mpv" <<'EOF'
#!/usr/bin/env bash
TOOLS="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
export APPDIR="$TOOLS/mpv-runtime/squashfs-root"
export PATH="$APPDIR/bin:$TOOLS/bin:$PATH"
exec "$APPDIR/bin/mpv" "$@"
EOF
    chmod +x "$PLAY_ANYTHING_TOOLS/bin/mpv"
}

install_ffmpeg_linux() {
    local arch asset archive runtime
    arch="$(linux_arch)"
    case "$arch" in
        x86_64) asset='linux64' ;;
        aarch64) asset='linuxarm64' ;;
    esac
    archive="$PLAY_ANYTHING_TOOLS/downloads/ffmpeg-${arch}.tar.xz"
    runtime="$PLAY_ANYTHING_TOOLS/ffmpeg-runtime"
    local url="https://github.com/BtbN/FFmpeg-Builds/releases/download/latest/ffmpeg-${FFMPEG_RELEASE}-${asset}-lgpl-shared-8.1.tar.xz"

    [[ -f "$archive" ]] || download "$url" "$archive"
    log 'Unpacking project-local FFmpeg'
    rm -rf "$runtime.new"
    mkdir -p "$runtime.new"
    tar -xJf "$archive" -C "$runtime.new" --strip-components=1
    rm -rf "$runtime"
    mv "$runtime.new" "$runtime"
    ln -sfn ../ffmpeg-runtime/bin/ffmpeg "$PLAY_ANYTHING_TOOLS/bin/ffmpeg"
    ln -sfn ../ffmpeg-runtime/bin/ffprobe "$PLAY_ANYTHING_TOOLS/bin/ffprobe"
}

install_ytdlp_linux() {
    local arch executable
    arch="$(linux_arch)"
    case "$arch" in
        x86_64) executable='yt-dlp_linux' ;;
        aarch64) executable='yt-dlp_linux_aarch64' ;;
    esac
    download \
        "https://github.com/yt-dlp/yt-dlp/releases/download/${YTDLP_RELEASE}/${executable}" \
        "$PLAY_ANYTHING_TOOLS/bin/yt-dlp"
    chmod +x "$PLAY_ANYTHING_TOOLS/bin/yt-dlp"
}

install_macos_tools() {
    if ! command -v brew >/dev/null 2>&1; then
        log 'Installing Homebrew for the macOS media backends'
        NONINTERACTIVE=1 /bin/bash -c \
            "$(curl -fsSL https://raw.githubusercontent.com/Homebrew/install/HEAD/install.sh)"
        if [[ -x /opt/homebrew/bin/brew ]]; then
            eval "$(/opt/homebrew/bin/brew shellenv)"
        elif [[ -x /usr/local/bin/brew ]]; then
            eval "$(/usr/local/bin/brew shellenv)"
        fi
    fi
    command -v brew >/dev/null 2>&1 || fail 'Could not initialize Homebrew'
    log 'Installing media backends with Homebrew'
    brew install mpv ffmpeg yt-dlp
    ln -sfn "$(brew --prefix mpv)/bin/mpv" "$PLAY_ANYTHING_TOOLS/bin/mpv"
    ln -sfn "$(brew --prefix ffmpeg)/bin/ffmpeg" "$PLAY_ANYTHING_TOOLS/bin/ffmpeg"
    ln -sfn "$(brew --prefix ffmpeg)/bin/ffprobe" "$PLAY_ANYTHING_TOOLS/bin/ffprobe"
    ln -sfn "$(brew --prefix yt-dlp)/bin/yt-dlp" "$PLAY_ANYTHING_TOOLS/bin/yt-dlp"
}

tool_works() {
    local name="$1"
    shift
    [[ -x "$PLAY_ANYTHING_TOOLS/bin/$name" ]] && \
        "$PLAY_ANYTHING_TOOLS/bin/$name" "$@" >/dev/null 2>&1
}

media_ready() {
    tool_works mpv --version && tool_works ffmpeg -version && tool_works yt-dlp --version
}

ensure_media() {
    case "$(uname -s)" in
        Linux)
            if is_termux; then
                media_ready || install_termux_tools
            elif is_musl; then
                media_ready || install_linux_system_tools
            else
                tool_works mpv --version || install_mpv_linux
                tool_works ffmpeg -version || install_ffmpeg_linux
                tool_works yt-dlp --version || install_ytdlp_linux
                if ! media_ready; then
                    install_linux_system_tools
                fi
                # Older distributions may not package yt-dlp. Their glibc systems can still
                # use the managed standalone executable after mpv/FFmpeg come from packages.
                if ! tool_works yt-dlp --version && ! is_musl; then
                    install_ytdlp_linux
                fi
            fi
            ;;
        Darwin)
            if ! media_ready; then
                install_macos_tools
            fi
            ;;
        *) fail "Unsupported operating system: $(uname -s)" ;;
    esac

    tool_works mpv --version || fail 'mpv is installed but could not start'
    tool_works ffmpeg -version || fail 'FFmpeg is installed but could not start'
    tool_works yt-dlp --version || fail 'yt-dlp is installed but could not start'
    log "mpv ready: $(mpv --version | sed -n '1p')"
    log "FFmpeg ready: $(ffmpeg -version | sed -n '1p')"
    log "yt-dlp ready: $(yt-dlp --version)"
}

install_rust=1
case "${1:-}" in
    '') ;;
    --media-only) install_rust=0 ;;
    *) fail "Unknown bootstrap option: $1" ;;
esac

mkdir -p "$PLAY_ANYTHING_TOOLS"/{bin,downloads}
if [[ "$install_rust" == 1 ]]; then
    mkdir -p "$PLAY_ANYTHING_TOOLS/cargo-home"
    ensure_rust
fi
ensure_media
log "Dependencies are installed inside $PLAY_ANYTHING_TOOLS"
