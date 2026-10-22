#[cfg(not(unix))]
compile_error!("play-anything currently supports Linux and macOS (mpv Unix-socket IPC)");

#[cfg(unix)]
use std::{
    env,
    fs,
    io::{BufRead, BufReader, Write},
    os::unix::net::UnixStream,
    path::PathBuf,
    process::{Child, Command, Stdio},
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        mpsc::{self, Receiver},
        Arc, Mutex,
    },
    thread,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

#[cfg(unix)]
use anyhow::{bail, Context, Result};
use serde_json::{json, Value};

#[derive(Debug, Clone)]
pub enum PlayerEvent {
    Property { name: String, value: Value },
    StartFile,
    FileLoaded,
    EndFile { reason: String },
    Disconnected(String),
}

/// A persistent mpv process controlled through its JSON IPC protocol.
///
/// mpv deliberately runs without a video window. It remains the audio/playback clock while the
/// separate decoder in `video.rs` supplies frames to the terminal UI.
#[cfg(unix)]
pub struct MpvPlayer {
    child: Child,
    writer: Mutex<UnixStream>,
    socket_path: PathBuf,
    request_id: AtomicU64,
    shutting_down: Arc<AtomicBool>,
    pub events: Receiver<PlayerEvent>,
}

#[cfg(unix)]
impl MpvPlayer {
    pub fn spawn(binary: &str) -> Result<Self> {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |duration| duration.as_nanos());
        let socket_path = env::temp_dir().join(format!(
            "play-anything-{}-{unique}.sock",
            std::process::id()
        ));
        let socket = socket_path.to_string_lossy().into_owned();

        let mut child = Command::new(binary)
            .args([
                "--idle=yes",
                "--no-config",
                "--no-terminal",
                "--force-window=no",
                "--input-default-bindings=no",
                "--input-vo-keyboard=no",
                "--osc=no",
                "--vid=no",
                "--audio-display=no",
                "--cache=yes",
                "--ytdl=yes",
                &format!("--input-ipc-server={socket}"),
            ])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .with_context(|| {
                format!(
                    "Could not start '{binary}'. Install mpv and make sure it is on PATH \
                     (https://mpv.io/installation/)."
                )
            })?;

        let mut connected = None;
        let mut last_error = None;
        for _ in 0..160 {
            match UnixStream::connect(&socket_path) {
                Ok(stream) => {
                    connected = Some(stream);
                    break;
                }
                Err(error) => last_error = Some(error),
            }
            if let Some(status) = child.try_wait().context("Could not inspect mpv process")? {
                let _ = fs::remove_file(&socket_path);
                bail!("mpv exited before accepting IPC connections ({status})");
            }
            thread::sleep(Duration::from_millis(25));
        }

        let Some(stream) = connected else {
            let _ = child.kill();
            let _ = child.wait();
            let _ = fs::remove_file(&socket_path);
            bail!(
                "Timed out connecting to mpv IPC at {}: {}",
                socket_path.display(),
                last_error
                    .map(|error| error.to_string())
                    .unwrap_or_else(|| "unknown error".to_string())
            );
        };
        stream
            .set_write_timeout(Some(Duration::from_secs(2)))
            .context("Could not configure mpv IPC")?;

        let reader = stream.try_clone().context("Could not clone mpv IPC stream")?;
        let (event_tx, events) = mpsc::channel();
        let shutting_down = Arc::new(AtomicBool::new(false));
        let reader_shutdown = Arc::clone(&shutting_down);
        thread::Builder::new()
            .name("mpv-ipc-reader".to_string())
            .spawn(move || {
                let reader = BufReader::new(reader);
                for line in reader.lines() {
                    match line {
                        Ok(line) => {
                            if let Some(event) = parse_event(&line) {
                                if event_tx.send(event).is_err() {
                                    return;
                                }
                            }
                        }
                        Err(error) => {
                            if !reader_shutdown.load(Ordering::Relaxed) {
                                let _ = event_tx.send(PlayerEvent::Disconnected(error.to_string()));
                            }
                            return;
                        }
                    }
                }
                if !reader_shutdown.load(Ordering::Relaxed) {
                    let _ = event_tx.send(PlayerEvent::Disconnected(
                        "The mpv process closed its IPC connection".to_string(),
                    ));
                }
            })
            .context("Could not start mpv event reader")?;

        let player = Self {
            child,
            writer: Mutex::new(stream),
            socket_path,
            request_id: AtomicU64::new(1),
            shutting_down,
            events,
        };
        player.observe_properties()?;
        Ok(player)
    }

    fn next_request_id(&self) -> u64 {
        self.request_id.fetch_add(1, Ordering::Relaxed)
    }

    fn send_value(&self, mut value: Value) -> Result<()> {
        value["request_id"] = json!(self.next_request_id());
        let encoded = serde_json::to_vec(&value).context("Could not encode mpv command")?;
        let mut writer = self
            .writer
            .lock()
            .map_err(|_| anyhow::anyhow!("mpv IPC writer lock was poisoned"))?;
        writer.write_all(&encoded).context("Could not write to mpv")?;
        writer.write_all(b"\n").context("Could not finish mpv command")?;
        writer.flush().context("Could not flush mpv command")?;
        Ok(())
    }

    fn command(&self, command: Value) -> Result<()> {
        self.send_value(json!({ "command": command }))
    }

    fn observe_property(&self, id: u64, name: &str) -> Result<()> {
        self.command(json!(["observe_property", id, name]))
    }

    fn observe_properties(&self) -> Result<()> {
        const PROPERTIES: &[&str] = &[
            "time-pos",
            "duration",
            "pause",
            "volume",
            "mute",
            "speed",
            "idle-active",
            "media-title",
            "metadata",
            "audio-codec-name",
            "video-codec-name",
            "video-params/w",
            "video-params/h",
            "audio-params/samplerate",
            "audio-params/channels",
            "track-list/count",
        ];
        for (index, name) in PROPERTIES.iter().enumerate() {
            self.observe_property(index as u64 + 1, name)?;
        }
        Ok(())
    }

    pub fn load(&self, source: &str) -> Result<()> {
        self.command(json!(["loadfile", source, "replace"]))?;
        self.set_paused(false)
    }

    pub fn stop(&self) -> Result<()> {
        self.command(json!(["stop"]))
    }

    pub fn set_paused(&self, paused: bool) -> Result<()> {
        self.command(json!(["set_property", "pause", paused]))
    }

    pub fn set_volume(&self, volume: f64) -> Result<()> {
        self.command(json!(["set_property", "volume", volume]))
    }

    pub fn set_muted(&self, muted: bool) -> Result<()> {
        self.command(json!(["set_property", "mute", muted]))
    }

    pub fn set_speed(&self, speed: f64) -> Result<()> {
        self.command(json!(["set_property", "speed", speed]))
    }

    pub fn seek_relative(&self, seconds: f64) -> Result<()> {
        self.command(json!(["seek", seconds, "relative+exact"]))
    }

    pub fn seek_absolute(&self, seconds: f64) -> Result<()> {
        self.command(json!(["seek", seconds, "absolute+exact"]))
    }

    pub fn cycle_audio_track(&self) -> Result<()> {
        self.command(json!(["cycle", "audio"]))
    }
}

#[cfg(unix)]
impl Drop for MpvPlayer {
    fn drop(&mut self) {
        self.shutting_down.store(true, Ordering::Relaxed);
        let _ = self.command(json!(["quit"]));
        let _ = self.child.kill();
        let _ = self.child.wait();
        let _ = fs::remove_file(&self.socket_path);
    }
}

fn parse_event(line: &str) -> Option<PlayerEvent> {
    let value: Value = serde_json::from_str(line).ok()?;
    if value.get("request_id").is_some() && value.get("event").is_none() {
        let name = value.get("property")?.as_str()?.to_string();
        return Some(PlayerEvent::Property {
            name,
            value: value.get("data").cloned().unwrap_or(Value::Null),
        });
    }

    match value.get("event")?.as_str()? {
        "property-change" => Some(PlayerEvent::Property {
            name: value.get("name")?.as_str()?.to_string(),
            value: value.get("data").cloned().unwrap_or(Value::Null),
        }),
        "start-file" => Some(PlayerEvent::StartFile),
        "file-loaded" => Some(PlayerEvent::FileLoaded),
        "end-file" => Some(PlayerEvent::EndFile {
            reason: value
                .get("reason")
                .and_then(Value::as_str)
                .unwrap_or("unknown")
                .to_string(),
        }),
        "shutdown" => Some(PlayerEvent::Disconnected("mpv shut down".to_string())),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_property_events() {
        let event = parse_event(r#"{"event":"property-change","name":"time-pos","data":12.5}"#);
        assert!(matches!(
            event,
            Some(PlayerEvent::Property { name, value }) if name == "time-pos" && value == 12.5
        ));
    }

    #[test]
    fn parses_end_of_file_reason() {
        let event = parse_event(r#"{"event":"end-file","reason":"eof"}"#);
        assert!(matches!(
            event,
            Some(PlayerEvent::EndFile { reason }) if reason == "eof"
        ));
    }

    #[test]
    fn ignores_unrelated_ipc_messages() {
        assert!(parse_event("not json").is_none());
        assert!(parse_event(r#"{"event":"tick"}"#).is_none());
    }
}
