mod app;
mod media;
mod player;
mod theme;
mod ui;
mod util;
mod video;

use std::{
    env,
    io::{self, IsTerminal, Stdout},
    path::{Path, PathBuf},
    process::{Command, ExitCode},
    time::{Duration, Instant},
};

use anyhow::{bail, Context, Result};
use app::{App, AppConfig};
use clap::Parser;
use crossterm::{
    event::{
        self, DisableBracketedPaste, DisableMouseCapture, EnableBracketedPaste, EnableMouseCapture,
        Event,
    },
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use media::discover;
use player::MpvPlayer;
use ratatui::{backend::CrosstermBackend, Terminal};

#[derive(Debug, Parser)]
#[command(
    name = "play-anything",
    version,
    about = "A beautiful terminal audio and video player powered by mpv",
    long_about = "Play local audio/video, folders, playlists, direct streams, and supported web URLs in a modern terminal UI. mpv provides playback; FFmpeg enables in-terminal video."
)]
struct Cli {
    /// Files, directories, playlists, or URLs to add to the queue
    #[arg(value_name = "PATH_OR_URL")]
    inputs: Vec<String>,

    /// Recursively discover media inside directories
    #[arg(short, long)]
    recursive: bool,

    /// Disable terminal video decoding (audio and visualizer still work)
    #[arg(long)]
    no_video: bool,

    /// Terminal video frame rate (lower values use less terminal bandwidth)
    #[arg(long, default_value_t = 15, value_parser = clap::value_parser!(u8).range(1..=30))]
    video_fps: u8,

    /// mpv executable
    #[arg(long, default_value = "mpv", value_name = "BINARY")]
    mpv: String,

    /// FFmpeg executable
    #[arg(long, default_value = "ffmpeg", value_name = "BINARY")]
    ffmpeg: String,

    /// yt-dlp executable used to resolve web-video previews
    #[arg(long, default_value = "yt-dlp", value_name = "BINARY")]
    ytdlp: String,

    /// Check media dependencies and terminal capabilities, then exit
    #[arg(long)]
    doctor: bool,
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("play-anything: {error:#}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<()> {
    let mut cli = Cli::parse();
    resolve_managed_tools(&mut cli);
    if cli.doctor {
        print_doctor(&cli);
        return Ok(());
    }
    if !io::stdout().is_terminal() || !io::stdin().is_terminal() {
        bail!("standard input and output must be attached to an interactive terminal");
    }

    let inputs = if cli.inputs.is_empty() {
        vec![".".to_string()]
    } else {
        cli.inputs.clone()
    };
    let discovery = discover(&inputs, cli.recursive, 1);
    let player = MpvPlayer::spawn(&cli.mpv)?;
    let app = App::new(
        player,
        discovery,
        AppConfig {
            recursive: cli.recursive,
            video_enabled: !cli.no_video,
            ffmpeg: cli.ffmpeg.clone(),
            resolver: cli.ytdlp.clone(),
            video_fps: cli.video_fps,
        },
    );

    install_panic_hook();
    enable_raw_mode().context("Could not enable terminal raw mode")?;
    if let Err(error) = execute!(
        io::stdout(),
        EnterAlternateScreen,
        EnableMouseCapture,
        EnableBracketedPaste
    ) {
        restore_terminal();
        return Err(error).context("Could not initialize the terminal display");
    }

    let backend = CrosstermBackend::new(io::stdout());
    let result = Terminal::new(backend)
        .context("Could not create the terminal renderer")
        .and_then(|mut terminal| run_event_loop(&mut terminal, app));

    restore_terminal();
    result
}

fn resolve_managed_tools(cli: &mut Cli) {
    cli.mpv = managed_binary(&cli.mpv, "mpv");
    cli.ffmpeg = managed_binary(&cli.ffmpeg, "ffmpeg");
    cli.ytdlp = managed_binary(&cli.ytdlp, "yt-dlp");
}

fn managed_binary(requested: &str, name: &str) -> String {
    if requested != name {
        return requested.to_string();
    }

    let mut roots = Vec::new();
    if let Some(tools) = env::var_os("PLAY_ANYTHING_TOOLS") {
        roots.push(PathBuf::from(tools));
    }
    roots.push(Path::new(env!("CARGO_MANIFEST_DIR")).join(".tools"));
    if let Ok(executable) = env::current_exe() {
        if let Some(root) = executable.parent().and_then(Path::parent) {
            roots.push(root.join(".tools"));
        }
    }

    let executable = format!("{name}{}", env::consts::EXE_SUFFIX);
    roots
        .into_iter()
        .flat_map(|root| {
            let runtime = root.join(format!("{name}-runtime")).join(&executable);
            [root.join("bin").join(&executable), runtime]
        })
        .find(|path| path.is_file())
        .map(|path| path.to_string_lossy().into_owned())
        .unwrap_or_else(|| requested.to_string())
}

fn run_event_loop(terminal: &mut Terminal<CrosstermBackend<Stdout>>, mut app: App) -> Result<()> {
    const FRAME_TIME: Duration = Duration::from_millis(33);
    terminal.clear().context("Could not clear the terminal")?;

    loop {
        let frame_started = Instant::now();
        terminal
            .draw(|frame| ui::render(frame, &mut app))
            .context("Could not render the interface")?;
        app.tick();

        if event::poll(FRAME_TIME.saturating_sub(frame_started.elapsed()))
            .context("Could not wait for terminal input")?
        {
            match event::read().context("Could not read terminal input")? {
                Event::Key(key) => {
                    if app.handle_key(key) {
                        break;
                    }
                }
                Event::Mouse(mouse) => app.handle_mouse(mouse),
                Event::Paste(text) => app.handle_paste(text),
                Event::Resize(_, _) => {
                    // The next draw recalculates every region; tick then restarts the decoder at
                    // the new terminal resolution if necessary.
                }
                _ => {}
            }
        }
    }
    Ok(())
}

fn install_panic_hook() {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |information| {
        restore_terminal();
        previous(information);
    }));
}

fn restore_terminal() {
    let _ = disable_raw_mode();
    let _ = execute!(
        io::stdout(),
        DisableMouseCapture,
        DisableBracketedPaste,
        LeaveAlternateScreen
    );
}

fn print_doctor(cli: &Cli) {
    println!("Play Anything · dependency check\n");
    let mpv = check_binary(&cli.mpv, "--version");
    let ffmpeg = check_binary(&cli.ffmpeg, "-version");
    let resolver = check_binary(&cli.ytdlp, "--version");
    print_check("mpv (required for playback)", &mpv);
    print_check("FFmpeg (terminal video)", &ffmpeg);
    print_check("yt-dlp (web-video previews)", &resolver);

    let true_color = env::var("COLORTERM")
        .map(|value| {
            let value = value.to_ascii_lowercase();
            value.contains("truecolor") || value.contains("24bit")
        })
        .unwrap_or(false);
    println!(
        "  {} True-color terminal ({})",
        if true_color { "✓" } else { "!" },
        if true_color {
            "24-bit color advertised".to_string()
        } else {
            "set COLORTERM=truecolor if colors look limited".to_string()
        }
    );
    println!("\nPlatform: {}", env::consts::OS);
    println!(
        "Terminal: {}",
        env::var("TERM").unwrap_or_else(|_| "unknown".to_string())
    );
    println!(
        "\nAudio uses mpv. The ./play launcher manages FFmpeg for video and yt-dlp for site URLs."
    );
}

fn check_binary(binary: &str, version_argument: &str) -> Result<String, String> {
    let output = Command::new(binary)
        .arg(version_argument)
        .output()
        .map_err(|error| error.to_string())?;
    if !output.status.success() {
        return Err(format!("exited with {}", output.status));
    }
    let text = if output.stdout.is_empty() {
        output.stderr
    } else {
        output.stdout
    };
    Ok(String::from_utf8_lossy(&text)
        .lines()
        .next()
        .unwrap_or("available")
        .trim()
        .to_string())
}

fn print_check(label: &str, result: &Result<String, String>) {
    match result {
        Ok(version) => println!("  ✓ {label:<34} {version}"),
        Err(error) => println!("  ✗ {label:<34} {error}"),
    }
}
