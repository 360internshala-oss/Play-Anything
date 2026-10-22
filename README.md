# ▶ Play Anything

**A professional, true-color terminal audio and video player written in Rust.**

Play local music, movies, folders, playlists, radio streams, and supported web URLs without leaving your terminal. `mpv` supplies the mature playback engine and audio clock; FFmpeg frames are rendered directly inside the TUI with Unicode half-blocks and 24-bit color.

## Highlights

- **Audio + actual in-terminal video** — not a player launcher or a static TUI mockup.
- **Broad format support** through mpv/FFmpeg: MP3, FLAC, Ogg, Opus, WAV, AAC, MP4, MKV, WebM, AVI, MOV, and many more.
- **Local and network media** — files, directories, direct URLs, Icecast/Shoutcast streams, and sites supported by `yt-dlp`.
- **Playlist import** — M3U, extended M3U/M3U8, and PLS, including relative paths and `EXTINF` metadata.
- **Complete transport** — pause, exact seek, next/previous, stop, volume, mute, 0.25×–4× speed, and embedded audio-track switching.
- **Queue tools** — live filtering, recursive discovery, duplicate removal, remove/clear, repeat off/all/one, and reversible shuffle.
- **Responsive modern interface** — true-color video, ambient visualizer, track metadata, clickable progress/transport controls, mouse wheel, toast messages, and focus mode.
- **Clean terminal lifecycle** — raw mode, mouse capture, and alternate screen are restored on exit and panic.

## One-command setup and run

On Linux x86_64 or aarch64, **this is the only command you need**:

```bash
git clone https://github.com/360internshala-oss/Play-Anything.git
cd Play-Anything
./play
```

The first launch automatically installs everything the application needs and then builds and opens the player:

- A stable Rust toolchain, if Rust is not already available
- mpv for playback, audio output, streams, metadata, and the media clock
- FFmpeg for in-terminal video frames
- yt-dlp for supported web-site playback and preview resolution
- All locked Cargo dependencies

There is **no sudo, manual media installation, or PATH editing**. Tools and runtime data live in the Git-ignored `.tools/` directory inside Play Anything. Subsequent launches reuse them. A Unicode terminal and standard `curl`/`tar` utilities are needed for the initial download; 24-bit color is recommended.

To prepare everything without opening the player:

```bash
./scripts/bootstrap.sh
```

To prevent any network access after setup:

```bash
PLAY_ANYTHING_OFFLINE=1 ./play
```

macOS uses the same project-local Rust bootstrap and installs media backends through Homebrew when available. The player itself supports Linux and macOS through mpv's Unix-socket JSON IPC. Linux media setup is fully project-local and does not depend on a distribution package manager.

With no media arguments, Play Anything scans supported media in the current directory without entering subdirectories. An empty queue is fine: press **`o`** to open anything.

For development, the same environment can be loaded directly:

```bash
source scripts/env.sh
cargo run --locked --release
```

### Examples

```bash
# Play one file
./play ~/Music/song.flac

# Build a queue from several inputs and autoplay the first
./play song.mp3 movie.mkv https://example.com/radio

# Recursively discover a music or video library
./play --recursive ~/Music ~/Videos

# Import a playlist
./play ./favorites.m3u

# Play a site supported by yt-dlp
./play "https://www.youtube.com/watch?v=VIDEO_ID"

# Audio/visualizer only, useful on slow connections or small terminals
./play --no-video movie.mkv

# Smoother video on fast terminals (default is 15 fps)
./play --video-fps 24 movie.mkv

# Check dependencies and terminal capabilities
./play --doctor

# Every CLI option
./play --help
```

> Quote URLs in your shell so `&`, `?`, and other characters are passed literally. Site URLs require `yt-dlp`; direct media URLs generally work with mpv/FFmpeg alone.

## Controls

| Key | Action |
| --- | --- |
| `Space` | Play / pause; starts the selected item when idle |
| `Enter` | Play selected item |
| `j` / `k` or `↓` / `↑` | Move through the filtered queue |
| `PgUp` / `PgDn` | Move by ten items |
| `Home` / `End` | First / last item |
| `←` / `→` | Seek 5 seconds |
| `Shift+←` / `Shift+→` | Seek 30 seconds |
| `n` / `p` / `x` | Next / previous / stop |
| `+` / `-` | Volume up / down |
| `m` | Mute / unmute |
| `[` / `]` | Decrease / increase playback speed |
| `\` | Reset speed to 1.0× |
| `a` | Cycle embedded audio tracks |
| `o` or `u` | Open a file, folder, playlist, or URL |
| `/` | Live-filter the queue; `Enter` accepts, `Esc` cancels |
| `Esc` | Clear an active filter or leave focus mode |
| `d` | Remove selected item |
| `C` | Clear the entire queue |
| `r` | Cycle repeat off → all → one |
| `s` | Shuffle queue / restore original order |
| `f` | Toggle stage focus mode |
| `v` | Cycle auto → video → visualizer |
| `?` or `F1` | Toggle keyboard help |
| `q` | Quit |
| `Ctrl+C` | Quit from anywhere |

**Mouse:** click to select, double-click to play, scroll the queue, click the progress bar to seek, or use the on-screen previous/play/next and volume buttons.

Open prompts support character navigation, `Home`, `End`, `Delete`, `Backspace`, `Ctrl+U`, and terminal paste.

## How terminal video works

1. A persistent mpv process plays audio and exposes properties/events over JSON IPC.
2. A background FFmpeg process decodes the first video stream into fixed-size RGB frames.
3. A bounded channel backpressures FFmpeg, preventing unbounded memory use.
4. Frames carry presentation timestamps and are held or dropped against mpv's clock.
5. The renderer maps two vertical pixels into each terminal cell using `▀`, with independent foreground/background RGB colors.

Seeks and terminal resizes restart only the lightweight decoder, not the whole application. Audio continues if a video preview cannot be decoded. Use `--no-video` to eliminate decoder overhead, or lower `--video-fps` for remote terminals.

The visualizer is intentionally an **ambient animation**, not a claim of real-time audio-spectrum analysis.

## Troubleshooting

### `Could not start 'mpv'`

Run `./scripts/bootstrap.sh`, then `./play --doctor`. The launcher will repair a missing project-local mpv automatically. You can select a custom executable with `--mpv /path/to/mpv`.

### Audio works but the stage says video is unavailable

Run `./scripts/bootstrap.sh` and then `./play --doctor` to verify the managed FFmpeg installation. Audio-only files intentionally show the visualizer. Some web services expose audio to mpv but do not provide a compatible combined preview format; playback still works.

### Web URLs do not play

Quote the URL and run `./play --doctor` to check the managed `yt-dlp`. A site can also be unsupported, geo-restricted, or require authentication. This application does not bypass those restrictions.

### Colors or glyphs look wrong

Use a current terminal such as WezTerm, Kitty, Alacritty, Ghostty, iTerm2, or Windows Terminal with WSL. Set `COLORTERM=truecolor` when appropriate and choose a Unicode-capable font. The minimum UI size is **60×18**; larger is better for video.

### Playback is slow over SSH/tmux

Use `--video-fps 8` or `--no-video`, enlarge the terminal only as needed, and enable true-color support in both tmux and the outer terminal. Terminal video necessarily produces more output than an audio-only interface.

## Development

```bash
source scripts/env.sh
cargo fmt --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked --all-targets
cargo build --locked --release

# On Linux, after ./scripts/bootstrap.sh (util-linux provides `script`):
./scripts/smoke-test.sh
```

The unit test suite does not need a running mpv server or media files. The optional smoke test generates a short synthetic movie, starts real mpv/FFmpeg processes in a pseudo-terminal, exercises pause/resume/quit, and checks clean shutdown. The normal launcher manages all media dependencies described above.

### Source layout

```text
src/main.rs    CLI, terminal setup, event loop, and diagnostics
src/app.rs     Queue/playback state, controls, input, and synchronization
src/player.rs  Persistent mpv process and JSON IPC protocol
src/video.rs   Bounded FFmpeg decoder and presentation timestamps
src/media.rs   Discovery, media classification, URLs, and playlists
src/ui.rs      Responsive Ratatui widgets and RGB frame rendering
src/theme.rs   Shared visual language
src/util.rs    Formatting and dependency-free utilities
```

## License

Licensed under the [Apache License 2.0](LICENSE).
