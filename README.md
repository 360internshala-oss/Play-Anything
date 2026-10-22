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

## Requirements

| Component | Why | Required |
| --- | --- | --- |
| Rust 1.75+ | Build the application | Build only |
| [mpv](https://mpv.io/) | Playback, audio output, streams, metadata, and the media clock | **Yes** |
| [FFmpeg](https://ffmpeg.org/) | Decode/scaled RGB frames for terminal video | Video only |
| [yt-dlp](https://github.com/yt-dlp/yt-dlp) | Web-site playback and preview URL resolution | Supported web sites |
| A Unicode terminal | Rounded panels, icons, and half-block video | **Yes** |
| 24-bit color | Best visual quality | Recommended |

Linux and macOS are supported. The player currently uses mpv's Unix-socket JSON IPC.

### Install media dependencies

**Debian / Ubuntu**

```bash
sudo apt update
sudo apt install mpv ffmpeg yt-dlp
```

**Fedora**

```bash
sudo dnf install mpv ffmpeg yt-dlp
```

**Arch Linux**

```bash
sudo pacman -S mpv ffmpeg yt-dlp
```

**macOS with Homebrew**

```bash
brew install mpv ffmpeg yt-dlp
```

Some distribution repositories package an older or unavailable `yt-dlp`; see the [yt-dlp installation guide](https://github.com/yt-dlp/yt-dlp#installation) when needed.

## Build and run

```bash
git clone https://github.com/360internshala-oss/Play-Anything.git
cd Play-Anything
cargo run --release
```

With no arguments, Play Anything scans supported media in the current directory (without entering subdirectories). An empty queue is fine: press **`o`** to open anything.

Install the binary for repeated use:

```bash
cargo install --path .
play-anything
```

### Examples

```bash
# Play one file
play-anything ~/Music/song.flac

# Build a queue from several inputs and autoplay the first
play-anything song.mp3 movie.mkv https://example.com/radio

# Recursively discover a music or video library
play-anything --recursive ~/Music ~/Videos

# Import a playlist
play-anything ./favorites.m3u

# Play a site supported by yt-dlp
play-anything "https://www.youtube.com/watch?v=VIDEO_ID"

# Audio/visualizer only, useful on slow connections or small terminals
play-anything --no-video movie.mkv

# Smoother video on fast terminals (default is 15 fps)
play-anything --video-fps 24 movie.mkv

# Check dependencies and terminal capabilities
play-anything --doctor

# Every CLI option
play-anything --help
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

Install mpv and confirm `mpv --version` works. You can select a nonstandard executable with `--mpv /path/to/mpv`.

### Audio works but the stage says video is unavailable

Install FFmpeg, check `ffmpeg -version`, and run `play-anything --doctor`. Audio-only files intentionally show the visualizer. Some web services expose audio to mpv but do not provide a compatible combined preview format; playback still works.

### Web URLs do not play

Install/update `yt-dlp` and quote the URL. A site can also be unsupported, geo-restricted, or require authentication. This application does not bypass those restrictions.

### Colors or glyphs look wrong

Use a current terminal such as WezTerm, Kitty, Alacritty, Ghostty, iTerm2, or Windows Terminal with WSL. Set `COLORTERM=truecolor` when appropriate and choose a Unicode-capable font. The minimum UI size is **60×18**; larger is better for video.

### Playback is slow over SSH/tmux

Use `--video-fps 8` or `--no-video`, enlarge the terminal only as needed, and enable true-color support in both tmux and the outer terminal. Terminal video necessarily produces more output than an audio-only interface.

## Development

```bash
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test --all-targets
cargo build --release

# On Linux with mpv, FFmpeg, and util-linux installed:
./scripts/smoke-test.sh
```

The unit test suite does not need a running mpv server or media files. The optional smoke test generates a short synthetic movie, starts real mpv/FFmpeg processes in a pseudo-terminal, exercises pause/resume/quit, and checks clean shutdown. Runtime playback requires the external dependencies described above.

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
