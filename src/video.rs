use std::{
    io::{ErrorKind, Read},
    process::{Child, Command, Stdio},
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver, SyncSender, TryRecvError},
        Arc, Mutex,
    },
    thread,
    time::{Duration, Instant},
};

use crate::media::{MediaItem, MediaKind};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VideoStatus {
    Idle,
    Loading,
    Ready,
    Unavailable,
}

#[derive(Debug, Clone)]
pub struct VideoFrame {
    pub width: usize,
    pub height: usize,
    pub pixels: Vec<u8>,
    pub pts: f64,
}

#[derive(Debug)]
enum VideoMessage {
    Frame(VideoFrame),
    Notice(String),
    Failed(String),
    Finished,
}

/// Background FFmpeg decoder with a bounded frame queue.
///
/// Each frame carries a synthetic presentation timestamp based on the decoder's fixed output
/// frame rate. `update` compares that timestamp with mpv's clock, dropping late frames and
/// holding early ones. A bounded channel naturally applies backpressure to FFmpeg's stdout.
pub struct VideoDecoder {
    ffmpeg: String,
    resolver: String,
    fps: u8,
    receiver: Receiver<VideoMessage>,
    cancel: Option<Arc<AtomicBool>>,
    child: Option<Arc<Mutex<Option<Child>>>>,
    latest: Option<VideoFrame>,
    pending: Option<VideoFrame>,
    status: VideoStatus,
    status_detail: Option<String>,
    signature: Option<(u64, u16, u16)>,
}

impl VideoDecoder {
    pub fn new(ffmpeg: String, resolver: String, fps: u8) -> Self {
        let (_sender, receiver) = mpsc::sync_channel(1);
        Self {
            ffmpeg,
            resolver,
            fps,
            receiver,
            cancel: None,
            child: None,
            latest: None,
            pending: None,
            status: VideoStatus::Idle,
            status_detail: None,
            signature: None,
        }
    }

    pub fn status(&self) -> VideoStatus {
        self.status
    }

    pub fn status_detail(&self) -> Option<&str> {
        self.status_detail.as_deref()
    }

    pub fn frame(&self) -> Option<&VideoFrame> {
        self.latest.as_ref()
    }

    pub fn signature(&self) -> Option<(u64, u16, u16)> {
        self.signature
    }

    pub fn start(&mut self, item: &MediaItem, area_width: u16, area_height: u16, position: f64) {
        self.stop();
        if area_width < 2 || area_height < 2 {
            return;
        }

        let (sender, receiver) = mpsc::sync_channel(2);
        self.receiver = receiver;
        self.status = VideoStatus::Loading;
        self.status_detail = None;
        self.signature = Some((item.id, area_width, area_height));

        let cancel = Arc::new(AtomicBool::new(false));
        let child_slot = Arc::new(Mutex::new(None));
        self.cancel = Some(Arc::clone(&cancel));
        self.child = Some(Arc::clone(&child_slot));

        let item = item.clone();
        let ffmpeg = self.ffmpeg.clone();
        let resolver = self.resolver.clone();
        let fps = self.fps;
        let worker_cancel = Arc::clone(&cancel);
        let worker_slot = Arc::clone(&child_slot);
        let spawn_result = thread::Builder::new()
            .name(format!("video-decoder-{}", item.id))
            .spawn(move || {
                decode_worker(
                    item,
                    area_width,
                    area_height,
                    position,
                    fps,
                    ffmpeg,
                    resolver,
                    sender,
                    worker_cancel,
                    worker_slot,
                );
            });
        if let Err(error) = spawn_result {
            self.status = VideoStatus::Unavailable;
            self.status_detail = Some(format!("Could not start video decoder: {error}"));
        }
    }

    pub fn stop(&mut self) {
        // Detach the old worker's channel before killing its process so a late frame can never
        // reappear in a subsequent queue item.
        let (_sender, receiver) = mpsc::sync_channel(1);
        self.receiver = receiver;
        if let Some(cancel) = self.cancel.take() {
            cancel.store(true, Ordering::Relaxed);
        }
        if let Some(slot) = self.child.take() {
            if let Ok(mut guard) = slot.lock() {
                if let Some(mut child) = guard.take() {
                    let _ = child.kill();
                    let _ = child.wait();
                }
            }
        }
        self.latest = None;
        self.pending = None;
        self.signature = None;
        self.status = VideoStatus::Idle;
        self.status_detail = None;
    }

    /// Drain decoder messages and return non-fatal notices for the UI toast area.
    pub fn update(&mut self, position: f64) -> Vec<String> {
        let mut notices = Vec::new();
        self.promote_pending(position);

        loop {
            match self.receiver.try_recv() {
                Ok(VideoMessage::Frame(frame)) => {
                    self.status = VideoStatus::Ready;
                    if frame.pts <= position + 0.16 {
                        self.latest = Some(frame);
                    } else {
                        self.pending = Some(frame);
                        break;
                    }
                }
                Ok(VideoMessage::Notice(message)) => notices.push(message),
                Ok(VideoMessage::Failed(message)) => {
                    self.status = VideoStatus::Unavailable;
                    self.status_detail = Some(message.clone());
                    notices.push(message);
                }
                Ok(VideoMessage::Finished) => {
                    if self.latest.is_none() && self.pending.is_none() {
                        self.status = VideoStatus::Unavailable;
                        self.status_detail = Some(
                            "No terminal video stream; audio playback is unaffected".to_string(),
                        );
                    }
                }
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    if self.status == VideoStatus::Loading {
                        self.status = VideoStatus::Unavailable;
                    }
                    break;
                }
            }
        }
        notices
    }

    fn promote_pending(&mut self, position: f64) {
        let ready = self
            .pending
            .as_ref()
            .map(|frame| frame.pts <= position + 0.16)
            .unwrap_or(false);
        if ready {
            self.latest = self.pending.take();
            self.status = VideoStatus::Ready;
        }
    }
}

impl Drop for VideoDecoder {
    fn drop(&mut self) {
        self.stop();
    }
}

#[allow(clippy::too_many_arguments)]
fn decode_worker(
    item: MediaItem,
    area_width: u16,
    area_height: u16,
    position: f64,
    fps: u8,
    ffmpeg: String,
    resolver: String,
    sender: SyncSender<VideoMessage>,
    cancel: Arc<AtomicBool>,
    child_slot: Arc<Mutex<Option<Child>>>,
) {
    let mut source = item.source.clone();
    if should_resolve(&item) {
        match resolve_stream(&resolver, &source, &cancel) {
            Ok(Some(resolved)) => source = resolved,
            Ok(None) => {}
            Err(error) => {
                let _ = sender.send(VideoMessage::Notice(error));
            }
        }
    }
    if cancel.load(Ordering::Relaxed) {
        return;
    }

    let (width, height) = decoder_dimensions(area_width, area_height);
    let filter = video_filter(width, height, fps);
    let start = if position.is_finite() {
        position.max(0.0)
    } else {
        0.0
    };

    let mut command = Command::new(&ffmpeg);
    command.args([
        "-hide_banner",
        "-loglevel",
        "error",
        "-nostdin",
        "-ss",
        &format!("{start:.3}"),
        "-i",
        &source,
        "-map",
        "0:v:0",
        "-an",
        "-sn",
        "-dn",
        "-vf",
        &filter,
        "-threads",
        "2",
        "-f",
        "rawvideo",
        "-pix_fmt",
        "rgb24",
        "pipe:1",
    ]);
    command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());

    let mut child = match command.spawn() {
        Ok(child) => child,
        Err(error) => {
            let _ = sender.send(VideoMessage::Failed(format!(
                "Video preview could not start '{ffmpeg}': {error}. Audio still works; install FFmpeg for video."
            )));
            return;
        }
    };
    let Some(mut stdout) = child.stdout.take() else {
        let _ = child.kill();
        let _ = sender.send(VideoMessage::Failed(
            "Could not capture the FFmpeg video stream".to_string(),
        ));
        return;
    };
    if let Ok(mut slot) = child_slot.lock() {
        if cancel.load(Ordering::Relaxed) {
            let _ = child.kill();
            let _ = child.wait();
            return;
        }
        *slot = Some(child);
    } else {
        let _ = child.kill();
        return;
    }

    let frame_size = width * height * 3;
    let mut sequence = 0_u64;
    loop {
        if cancel.load(Ordering::Relaxed) {
            break;
        }
        let mut pixels = vec![0_u8; frame_size];
        match stdout.read_exact(&mut pixels) {
            Ok(()) => {}
            Err(error) if error.kind() == ErrorKind::Interrupted => continue,
            Err(_) => break,
        }
        let frame = VideoFrame {
            width,
            height,
            pixels,
            pts: start + sequence as f64 / f64::from(fps),
        };
        sequence += 1;
        if sender.send(VideoMessage::Frame(frame)).is_err() {
            break;
        }
    }

    if let Ok(mut slot) = child_slot.lock() {
        if let Some(mut child) = slot.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
    if !cancel.load(Ordering::Relaxed) {
        let _ = sender.send(VideoMessage::Finished);
    }
}

fn should_resolve(item: &MediaItem) -> bool {
    item.kind == MediaKind::Stream
        && !item.source.to_ascii_lowercase().contains(".m3u8")
        && !item.source.starts_with("magnet:")
}

fn resolve_stream(
    resolver: &str,
    source: &str,
    cancel: &Arc<AtomicBool>,
) -> Result<Option<String>, String> {
    let mut child = Command::new(resolver)
        .args([
            "--no-playlist",
            "--no-warnings",
            "-f",
            "best[height<=480]/best",
            "-g",
            source,
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|error| {
            format!("Could not run '{resolver}' for this web preview ({error}); trying the direct URL")
        })?;

    let started = Instant::now();
    loop {
        if cancel.load(Ordering::Relaxed) {
            let _ = child.kill();
            let _ = child.wait();
            return Ok(None);
        }
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) => {}
            Err(error) => {
                let _ = child.kill();
                return Err(format!("Stream resolver failed: {error}; trying the direct URL"));
            }
        }
        if started.elapsed() > Duration::from_secs(30) {
            let _ = child.kill();
            let _ = child.wait();
            return Err("Stream resolution timed out; trying the direct URL".to_string());
        }
        thread::sleep(Duration::from_millis(50));
    }

    let output = child
        .wait_with_output()
        .map_err(|error| format!("Could not read the resolved stream: {error}"))?;
    if !output.status.success() {
        return Err("This site did not expose a preview stream; trying the direct URL".to_string());
    }
    let url = String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .map(str::to_string);
    Ok(url)
}

fn decoder_dimensions(area_width: u16, area_height: u16) -> (usize, usize) {
    let desired_width = usize::from(area_width.max(2));
    let desired_height = usize::from(area_height.max(1)) * 2;
    let scale = f64::min(
        1.0,
        f64::min(192.0 / desired_width as f64, 192.0 / desired_height as f64),
    );
    let width = ((desired_width as f64 * scale).round() as usize).max(2);
    let mut height = ((desired_height as f64 * scale).round() as usize).max(2);
    if height % 2 != 0 {
        height += 1;
    }
    (width, height)
}

fn video_filter(width: usize, height: usize, fps: u8) -> String {
    format!(
        "fps={fps},scale={width}:{height}:force_original_aspect_ratio=decrease:flags=fast_bilinear,pad={width}:{height}:(ow-iw)/2:(oh-ih)/2:color=black"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn filter_has_fixed_dimensions_for_raw_frames() {
        let filter = video_filter(80, 40, 15);
        assert!(filter.contains("fps=15"));
        assert!(filter.contains("scale=80:40"));
        assert!(filter.contains("pad=80:40"));
    }

    #[test]
    fn dimensions_are_bounded_and_have_even_pixel_height() {
        for (width, height) in [(80, 30), (500, 300), (1, 1)] {
            let (decoded_width, decoded_height) = decoder_dimensions(width, height);
            assert!(decoded_width <= 194);
            assert!(decoded_height <= 194);
            assert_eq!(decoded_height % 2, 0);
        }
    }
}
