#!/usr/bin/env bash
# Offline installer/uninstaller regression tests using local release and source URLs.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

if [[ "$(uname -s)" != 'Linux' ]]; then
    echo 'Installer regression tests currently require Linux'
    exit 0
fi
case "$(uname -m)" in
    x86_64|amd64) architecture='x86_64' ;;
    aarch64|arm64) architecture='aarch64' ;;
    *) echo "Unsupported test architecture: $(uname -m)" >&2; exit 1 ;;
esac

mkdir -p "$WORK/release" "$WORK/home" "$WORK/commands"
cp /bin/true "$WORK/release/play-anything-linux-$architecture"
cp /bin/true "$WORK/release/play-anything-termux-$architecture"
printf 'keep this profile line\n' >"$WORK/home/.profile"

HOME="$WORK/home" \
PLAY_ANYTHING_HOME="$WORK/home/.local/share/play-anything" \
PLAY_ANYTHING_COMMAND_DIR="$WORK/commands" \
PLAY_ANYTHING_RELEASE_BASE="file://$WORK/release" \
PLAY_ANYTHING_SOURCE_BASE="file://$ROOT" \
PLAY_ANYTHING_SKIP_MEDIA=1 \
    bash "$ROOT/installation/install.sh"

[[ -x "$WORK/commands/play-anything" ]]
[[ -L "$WORK/commands/Play-Anything" ]]
"$WORK/commands/play-anything" --version
"$WORK/commands/Play-Anything" --version
grep -F "$WORK/commands" "$WORK/home/.profile" >/dev/null

HOME="$WORK/home" \
PLAY_ANYTHING_HOME="$WORK/home/.local/share/play-anything" \
PLAY_ANYTHING_COMMAND_DIR="$WORK/commands" \
    bash "$ROOT/installation/uninstall.sh"

[[ ! -e "$WORK/commands/play-anything" ]]
[[ ! -L "$WORK/commands/Play-Anything" ]]
[[ ! -e "$WORK/home/.local/share/play-anything" ]]
grep -F 'keep this profile line' "$WORK/home/.profile" >/dev/null
if grep -F "$WORK/commands" "$WORK/home/.profile" >/dev/null; then
    echo 'Uninstaller left its command directory in .profile' >&2
    exit 1
fi

TERMUX_HOME="$WORK/termux-home"
TERMUX_PREFIX="$WORK/com.termux/files/usr"
mkdir -p "$TERMUX_HOME" "$TERMUX_PREFIX/bin"
HOME="$TERMUX_HOME" \
PREFIX="$TERMUX_PREFIX" \
TERMUX_VERSION='test' \
PATH="$TERMUX_PREFIX/bin:$PATH" \
PLAY_ANYTHING_HOME="$TERMUX_HOME/.local/share/play-anything" \
PLAY_ANYTHING_RELEASE_BASE="file://$WORK/release" \
PLAY_ANYTHING_SOURCE_BASE="file://$ROOT" \
PLAY_ANYTHING_SKIP_MEDIA=1 \
    bash "$ROOT/installation/install.sh"

[[ -x "$TERMUX_PREFIX/bin/play-anything" ]]
[[ -L "$TERMUX_PREFIX/bin/Play-Anything" ]]
"$TERMUX_PREFIX/bin/play-anything" --version

# Fake Termux's native packages to verify that media-only setup uses pkg and never AppImage.
CUSTOM_MEDIA_BIN="$WORK/native media/bin"
mkdir -p "$CUSTOM_MEDIA_BIN"
cat >"$CUSTOM_MEDIA_BIN/termux-media" <<'EOF'
#!/usr/bin/env bash
printf '%s fake-termux-package\n' "$(basename "$0")"
EOF
cat >"$TERMUX_PREFIX/bin/pkg" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
for name in mpv ffmpeg ffprobe yt-dlp; do
    ln -sfn "$TERMUX_TEST_MEDIA_BIN/termux-media" "$TERMUX_TEST_MEDIA_BIN/$name"
done
EOF
chmod +x "$TERMUX_PREFIX/bin/pkg" "$CUSTOM_MEDIA_BIN/termux-media"
HOME="$TERMUX_HOME" \
PREFIX="$TERMUX_PREFIX" \
TERMUX_VERSION='test' \
TERMUX_TEST_MEDIA_BIN="$CUSTOM_MEDIA_BIN" \
PATH="$CUSTOM_MEDIA_BIN:$TERMUX_PREFIX/bin:$PATH" \
PLAY_ANYTHING_TOOLS="$TERMUX_HOME/.local/share/play-anything/.tools" \
    bash "$ROOT/scripts/bootstrap.sh" --media-only
for name in mpv ffmpeg ffprobe yt-dlp; do
    [[ -x "$TERMUX_HOME/.local/share/play-anything/.tools/bin/$name" ]]
    [[ "$(readlink "$TERMUX_HOME/.local/share/play-anything/.tools/bin/$name")" == \
        "$CUSTOM_MEDIA_BIN/$name" ]]
done
if find "$TERMUX_HOME/.local/share/play-anything/.tools" -name '*.AppImage' | grep . >/dev/null; then
    echo 'Termux bootstrap unexpectedly downloaded an AppImage' >&2
    exit 1
fi

HOME="$TERMUX_HOME" \
PREFIX="$TERMUX_PREFIX" \
PATH="$TERMUX_PREFIX/bin:$PATH" \
PLAY_ANYTHING_HOME="$TERMUX_HOME/.local/share/play-anything" \
    bash "$ROOT/installation/uninstall.sh"

[[ ! -e "$TERMUX_PREFIX/bin/play-anything" ]]
[[ ! -L "$TERMUX_PREFIX/bin/Play-Anything" ]]
[[ ! -e "$TERMUX_HOME/.local/share/play-anything" ]]

echo 'Installer tests passed: desktop Linux + Termux + aliases + PATH + uninstall'
