#!/usr/bin/env bash
# End-to-end smoke test for Linux. Requires the built binary, mpv, FFmpeg, and util-linux.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
BINARY="${1:-$ROOT/target/release/play-anything}"
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

if [[ ! -x "$BINARY" ]]; then
    echo "Build the release binary first: cargo build --release" >&2
    exit 1
fi
for dependency in ffmpeg mpv script timeout; do
    command -v "$dependency" >/dev/null || {
        echo "Missing smoke-test dependency: $dependency" >&2
        exit 1
    }
done

# CI runners have no audio device. A wrapper keeps production defaults unchanged while selecting
# mpv's null audio output for this headless test.
cat >"$WORK/mpv-null" <<'EOF'
#!/usr/bin/env sh
exec mpv --ao=null "$@"
EOF
chmod +x "$WORK/mpv-null"

ffmpeg -hide_banner -loglevel error \
    -f lavfi -i 'testsrc2=size=320x240:rate=15' \
    -f lavfi -i 'sine=frequency=440:sample_rate=44100' \
    -t 8 -c:v mpeg4 -c:a aac -shortest "$WORK/sample.mp4"

printf -v command '%q ' \
    "$BINARY" \
    --mpv "$WORK/mpv-null" \
    --video-fps 5 \
    "$WORK/sample.mp4"

# `script` provides a real pseudo-terminal. Exercise pause/resume before requesting a clean quit.
set +e
(
    sleep 3
    printf ' '
    sleep 1
    printf ' '
    sleep 1
    printf 'q'
) | timeout 25 script -q -e -c "stty rows 32 cols 110; $command" "$WORK/terminal.log" >/dev/null 2>&1
status=$?
set -e

if [[ $status -ne 0 ]]; then
    echo "Runtime smoke test failed with status $status" >&2
    tail -80 "$WORK/terminal.log" >&2 || true
    exit "$status"
fi
if grep -qiE 'panicked at|player disconnected|could not play' "$WORK/terminal.log"; then
    echo "Runtime smoke test found an application error" >&2
    tail -80 "$WORK/terminal.log" >&2 || true
    exit 1
fi

echo "Runtime smoke test passed: mpv + FFmpeg + TUI + input + clean shutdown"
