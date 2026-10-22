use std::{
    collections::HashSet,
    time::{Duration, Instant},
};

use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseEvent, MouseEventKind};
use ratatui::layout::Rect;
use serde_json::Value;

use crate::{
    media::{discover, Discovery, MediaItem},
    player::{MpvPlayer, PlayerEvent},
    util::{clamp, shuffled_indices},
    video::VideoDecoder,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RepeatMode {
    Off,
    All,
    One,
}

impl RepeatMode {
    pub fn label(self) -> &'static str {
        match self {
            Self::Off => "OFF",
            Self::All => "ALL",
            Self::One => "ONE",
        }
    }

    fn next(self) -> Self {
        match self {
            Self::Off => Self::All,
            Self::All => Self::One,
            Self::One => Self::Off,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StageMode {
    Auto,
    Video,
    Visualizer,
}

impl StageMode {
    pub fn label(self) -> &'static str {
        match self {
            Self::Auto => "AUTO",
            Self::Video => "VIDEO",
            Self::Visualizer => "VISUALIZER",
        }
    }

    fn next(self) -> Self {
        match self {
            Self::Auto => Self::Video,
            Self::Video => Self::Visualizer,
            Self::Visualizer => Self::Auto,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputMode {
    Normal,
    Open,
    Search,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToastKind {
    Info,
    Success,
    Warning,
    Error,
}

#[derive(Debug, Clone)]
pub struct Toast {
    pub message: String,
    pub kind: ToastKind,
    pub until: Instant,
}

#[derive(Debug, Clone)]
pub struct PlaybackState {
    pub position: f64,
    pub duration: Option<f64>,
    pub paused: bool,
    pub volume: f64,
    pub muted: bool,
    pub speed: f64,
    pub idle: bool,
    pub media_title: Option<String>,
    pub artist: Option<String>,
    pub album: Option<String>,
    pub audio_codec: Option<String>,
    pub video_codec: Option<String>,
    pub video_width: Option<u64>,
    pub video_height: Option<u64>,
    pub sample_rate: Option<u64>,
    pub channels: Option<u64>,
    pub track_count: Option<u64>,
    position_updated: Instant,
}

impl Default for PlaybackState {
    fn default() -> Self {
        Self {
            position: 0.0,
            duration: None,
            paused: true,
            volume: 100.0,
            muted: false,
            speed: 1.0,
            idle: true,
            media_title: None,
            artist: None,
            album: None,
            audio_codec: None,
            video_codec: None,
            video_width: None,
            video_height: None,
            sample_rate: None,
            channels: None,
            track_count: None,
            position_updated: Instant::now(),
        }
    }
}

impl PlaybackState {
    fn reset_media(&mut self) {
        let volume = self.volume;
        let muted = self.muted;
        let speed = self.speed;
        *self = Self {
            volume,
            muted,
            speed,
            paused: false,
            idle: false,
            ..Self::default()
        };
    }

    fn set_position(&mut self, position: f64) {
        self.position = if position.is_finite() {
            position.max(0.0)
        } else {
            0.0
        };
        self.position_updated = Instant::now();
    }
}

#[derive(Debug, Default, Clone, Copy)]
pub struct LayoutRegions {
    pub playlist: Rect,
    pub stage: Rect,
    pub progress: Rect,
    pub previous: Rect,
    pub play_pause: Rect,
    pub next: Rect,
    pub volume_down: Rect,
    pub volume_up: Rect,
}

pub struct AppConfig {
    pub recursive: bool,
    pub video_enabled: bool,
    pub ffmpeg: String,
    pub resolver: String,
    pub video_fps: u8,
}

pub struct App {
    player: MpvPlayer,
    pub items: Vec<MediaItem>,
    pub selected: usize,
    pub current_id: Option<u64>,
    pub playback: PlaybackState,
    pub repeat: RepeatMode,
    pub shuffled: bool,
    pub stage_mode: StageMode,
    pub fullscreen: bool,
    pub show_help: bool,
    pub input_mode: InputMode,
    pub input: String,
    pub input_cursor: usize,
    pub search_query: String,
    pub connected: bool,
    pub loading: bool,
    pub video: VideoDecoder,
    pub regions: LayoutRegions,
    pub list_offset: usize,
    pub toast: Option<Toast>,
    pub tick: u64,
    recursive: bool,
    video_enabled: bool,
    next_id: u64,
    original_input: String,
    last_click: Option<(Instant, u64)>,
}

impl App {
    pub fn new(player: MpvPlayer, discovery: Discovery, config: AppConfig) -> Self {
        let initial_warning = discovery.warnings.first().cloned();
        let next_id = discovery.next_id;
        let mut app = Self {
            player,
            items: discovery.items,
            selected: 0,
            current_id: None,
            playback: PlaybackState::default(),
            repeat: RepeatMode::Off,
            shuffled: false,
            stage_mode: StageMode::Auto,
            fullscreen: false,
            show_help: false,
            input_mode: InputMode::Normal,
            input: String::new(),
            input_cursor: 0,
            search_query: String::new(),
            connected: true,
            loading: false,
            video: VideoDecoder::new(config.ffmpeg, config.resolver, config.video_fps),
            regions: LayoutRegions::default(),
            list_offset: 0,
            toast: None,
            tick: 0,
            recursive: config.recursive,
            video_enabled: config.video_enabled,
            next_id,
            original_input: String::new(),
            last_click: None,
        };
        if !app.items.is_empty() {
            app.play_index(0);
        }
        if let Some(warning) = initial_warning {
            app.toast(warning, ToastKind::Warning);
        }
        app
    }

    pub fn current_item(&self) -> Option<&MediaItem> {
        let id = self.current_id?;
        self.items.iter().find(|item| item.id == id)
    }

    fn current_index(&self) -> Option<usize> {
        let id = self.current_id?;
        self.items.iter().position(|item| item.id == id)
    }

    pub fn selected_item(&self) -> Option<&MediaItem> {
        self.items.get(self.selected)
    }

    pub fn visible_indices(&self) -> Vec<usize> {
        let query = self.search_query.trim().to_lowercase();
        self.items
            .iter()
            .enumerate()
            .filter(|(_, item)| {
                query.is_empty()
                    || item.title.to_lowercase().contains(&query)
                    || item.source.to_lowercase().contains(&query)
                    || item.kind.label().to_lowercase().contains(&query)
            })
            .map(|(index, _)| index)
            .collect()
    }

    pub fn position(&self) -> f64 {
        let mut position = self.playback.position;
        if !self.playback.paused && !self.playback.idle && !self.loading && self.connected {
            position += self.playback.position_updated.elapsed().as_secs_f64();
        }
        if let Some(duration) = self.duration() {
            position = position.min(duration);
        }
        position.max(0.0)
    }

    pub fn duration(&self) -> Option<f64> {
        self.playback
            .duration
            .or_else(|| self.current_item().and_then(|item| item.duration))
            .filter(|duration| duration.is_finite() && *duration > 0.0)
    }

    pub fn progress(&self) -> f64 {
        self.duration()
            .map(|duration| (self.position() / duration).clamp(0.0, 1.0))
            .unwrap_or(0.0)
    }

    pub fn toast(&mut self, message: impl Into<String>, kind: ToastKind) {
        self.toast = Some(Toast {
            message: message.into(),
            kind,
            until: Instant::now() + Duration::from_secs(4),
        });
    }

    pub fn tick(&mut self) {
        self.tick = self.tick.wrapping_add(1);
        while let Ok(event) = self.player.events.try_recv() {
            self.handle_player_event(event);
        }

        let position = self.position();
        for notice in self.video.update(position) {
            self.toast(notice, ToastKind::Warning);
        }
        if self
            .toast
            .as_ref()
            .map(|toast| toast.until <= Instant::now())
            .unwrap_or(false)
        {
            self.toast = None;
        }
        self.sync_video_viewport();
    }

    fn handle_player_event(&mut self, event: PlayerEvent) {
        match event {
            PlayerEvent::Property { name, value } => self.update_property(&name, &value),
            PlayerEvent::StartFile => {
                self.loading = true;
                self.playback.set_position(0.0);
            }
            PlayerEvent::FileLoaded => {
                self.loading = false;
                self.playback.idle = false;
            }
            PlayerEvent::EndFile { reason } => {
                self.loading = false;
                if reason == "eof" {
                    self.advance_after_eof();
                } else if reason == "error" {
                    self.playback.paused = true;
                    self.playback.idle = true;
                    self.video.stop();
                    self.toast(
                        "mpv could not play this source. Try another item or check the URL.",
                        ToastKind::Error,
                    );
                }
            }
            PlayerEvent::Disconnected(message) => {
                self.connected = false;
                self.loading = false;
                self.playback.paused = true;
                self.video.stop();
                self.toast(format!("Player disconnected: {message}"), ToastKind::Error);
            }
        }
    }

    fn update_property(&mut self, name: &str, value: &Value) {
        match name {
            "time-pos" => {
                if let Some(position) = number(value) {
                    self.playback.set_position(position);
                }
            }
            "duration" => {
                self.playback.duration = number(value).filter(|value| *value > 0.0);
                if let Some(id) = self.current_id {
                    if let Some(item) = self.items.iter_mut().find(|item| item.id == id) {
                        item.duration = self.playback.duration;
                    }
                }
            }
            "pause" => {
                if let Some(paused) = boolean(value) {
                    self.playback.paused = paused;
                    self.playback.position_updated = Instant::now();
                }
            }
            "volume" => {
                if let Some(volume) = number(value) {
                    self.playback.volume = clamp(volume, 0.0, 100.0);
                }
            }
            "mute" => {
                if let Some(muted) = boolean(value) {
                    self.playback.muted = muted;
                }
            }
            "speed" => {
                if let Some(speed) = number(value) {
                    self.playback.speed = clamp(speed, 0.25, 4.0);
                }
            }
            "idle-active" => self.playback.idle = boolean(value).unwrap_or(false),
            "media-title" => {
                self.playback.media_title = value
                    .as_str()
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .map(str::to_string);
            }
            "metadata" => self.update_metadata(value),
            "audio-codec-name" => self.playback.audio_codec = string(value),
            "video-codec-name" => self.playback.video_codec = string(value),
            "video-params/w" => self.playback.video_width = value.as_u64(),
            "video-params/h" => self.playback.video_height = value.as_u64(),
            "audio-params/samplerate" => self.playback.sample_rate = value.as_u64(),
            "audio-params/channels" => self.playback.channels = value.as_u64(),
            "track-list/count" => self.playback.track_count = value.as_u64(),
            _ => {}
        }
    }

    fn update_metadata(&mut self, value: &Value) {
        let Some(metadata) = value.as_object() else {
            return;
        };
        for (key, value) in metadata {
            let Some(value) = value.as_str().map(str::trim).filter(|value| !value.is_empty()) else {
                continue;
            };
            match key.to_ascii_lowercase().as_str() {
                "title" => self.playback.media_title = Some(value.to_string()),
                "artist" | "album_artist" => self.playback.artist = Some(value.to_string()),
                "album" => self.playback.album = Some(value.to_string()),
                _ => {}
            }
        }
    }

    fn play_index(&mut self, index: usize) {
        let Some(item) = self.items.get(index).cloned() else {
            return;
        };
        self.selected = index;
        self.current_id = Some(item.id);
        self.playback.reset_media();
        self.loading = true;
        self.video.stop();
        if let Err(error) = self.player.load(&item.source) {
            self.loading = false;
            self.playback.paused = true;
            self.toast(format!("Could not play {}: {error}", item.title), ToastKind::Error);
        }
    }

    fn play_selected(&mut self) {
        let visible = self.visible_indices();
        let index = if visible.contains(&self.selected) {
            Some(self.selected)
        } else {
            visible.first().copied()
        };
        if let Some(index) = index {
            self.play_index(index);
        }
    }

    fn play_id(&mut self, id: u64) {
        if let Some(index) = self.items.iter().position(|item| item.id == id) {
            self.play_index(index);
        }
    }

    fn toggle_pause(&mut self) {
        if self.current_id.is_none() {
            self.play_selected();
            return;
        }
        let paused = !self.playback.paused;
        if let Err(error) = self.player.set_paused(paused) {
            self.toast(error.to_string(), ToastKind::Error);
        } else {
            self.playback.paused = paused;
            self.playback.position_updated = Instant::now();
        }
    }

    fn stop(&mut self) {
        if let Err(error) = self.player.stop() {
            self.toast(error.to_string(), ToastKind::Error);
        }
        self.video.stop();
        self.current_id = None;
        self.loading = false;
        self.playback = PlaybackState {
            volume: self.playback.volume,
            muted: self.playback.muted,
            speed: self.playback.speed,
            ..PlaybackState::default()
        };
    }

    fn next(&mut self) {
        if self.items.is_empty() {
            return;
        }
        let next = self.current_index().map_or(self.selected, |index| index + 1);
        self.play_index(next % self.items.len());
    }

    fn previous(&mut self) {
        if self.items.is_empty() {
            return;
        }
        if self.position() > 4.0 {
            if let Some(index) = self.current_index() {
                self.play_index(index);
                return;
            }
        }
        let current = self.current_index().unwrap_or(self.selected);
        let previous = if current == 0 {
            self.items.len() - 1
        } else {
            current - 1
        };
        self.play_index(previous);
    }

    fn advance_after_eof(&mut self) {
        if self.repeat == RepeatMode::One {
            if let Some(id) = self.current_id {
                self.play_id(id);
            }
            return;
        }
        if let Some(index) = self.current_index() {
            if index + 1 < self.items.len() {
                self.play_index(index + 1);
            } else if self.repeat == RepeatMode::All && !self.items.is_empty() {
                self.play_index(0);
            } else {
                self.playback.paused = true;
                self.playback.idle = true;
                if let Some(duration) = self.duration() {
                    self.playback.set_position(duration);
                }
                self.video.stop();
            }
        }
    }

    fn seek_relative(&mut self, amount: f64) {
        if self.current_id.is_none() {
            return;
        }
        let mut target = (self.position() + amount).max(0.0);
        if let Some(duration) = self.duration() {
            target = target.min(duration);
        }
        self.seek_absolute(target);
    }

    fn seek_absolute(&mut self, position: f64) {
        if self.current_id.is_none() {
            return;
        }
        let position = position.max(0.0);
        match self.player.seek_absolute(position) {
            Ok(()) => {
                self.playback.set_position(position);
                self.restart_video();
            }
            Err(error) => self.toast(format!("Seek failed: {error}"), ToastKind::Error),
        }
    }

    fn restart_video(&mut self) {
        if !self.video_enabled || self.stage_mode == StageMode::Visualizer {
            return;
        }
        let Some(item) = self.current_item().cloned().filter(|item| item.may_have_video()) else {
            return;
        };
        let area = self.regions.stage;
        self.video
            .start(&item, area.width, area.height, self.position());
    }

    fn sync_video_viewport(&mut self) {
        let should_decode = self.video_enabled
            && self.connected
            && !self.playback.idle
            && self.stage_mode != StageMode::Visualizer
            && self.current_item().map(MediaItem::may_have_video).unwrap_or(false);
        if !should_decode {
            if self.video.signature().is_some() {
                self.video.stop();
            }
            return;
        }

        let area = self.regions.stage;
        if area.width < 2 || area.height < 2 {
            if self.video.signature().is_some() {
                self.video.stop();
            }
            return;
        }
        let Some(item) = self.current_item().cloned() else {
            return;
        };
        let signature = (item.id, area.width, area.height);
        if self.video.signature() != Some(signature) {
            self.video
                .start(&item, area.width, area.height, self.position());
        }
    }

    fn change_volume(&mut self, amount: f64) {
        let volume = clamp((self.playback.volume + amount).round(), 0.0, 100.0);
        match self.player.set_volume(volume) {
            Ok(()) => self.playback.volume = volume,
            Err(error) => self.toast(error.to_string(), ToastKind::Error),
        }
    }

    fn toggle_mute(&mut self) {
        let muted = !self.playback.muted;
        match self.player.set_muted(muted) {
            Ok(()) => self.playback.muted = muted,
            Err(error) => self.toast(error.to_string(), ToastKind::Error),
        }
    }

    fn change_speed(&mut self, amount: f64) {
        let speed = clamp(
            ((self.playback.speed + amount) * 10.0).round() / 10.0,
            0.25,
            4.0,
        );
        match self.player.set_speed(speed) {
            Ok(()) => {
                self.playback.speed = speed;
                self.toast(format!("Playback speed {speed:.1}×"), ToastKind::Info);
            }
            Err(error) => self.toast(error.to_string(), ToastKind::Error),
        }
    }

    fn toggle_repeat(&mut self) {
        self.repeat = self.repeat.next();
        self.toast(
            format!("Repeat {}", self.repeat.label().to_lowercase()),
            ToastKind::Info,
        );
    }

    fn toggle_shuffle(&mut self) {
        let selected_id = self.selected_item().map(|item| item.id);
        self.shuffled = !self.shuffled;
        if self.shuffled {
            let order = shuffled_indices(self.items.len());
            let mut slots: Vec<Option<MediaItem>> = self.items.drain(..).map(Some).collect();
            self.items = order
                .into_iter()
                .filter_map(|index| slots.get_mut(index).and_then(Option::take))
                .collect();
            self.toast("Queue shuffled", ToastKind::Success);
        } else {
            self.items.sort_by_key(|item| item.id);
            self.toast("Original queue order restored", ToastKind::Info);
        }
        if let Some(selected_id) = selected_id {
            self.selected = self
                .items
                .iter()
                .position(|item| item.id == selected_id)
                .unwrap_or(0);
        }
        self.list_offset = 0;
    }

    fn move_selection(&mut self, amount: isize) {
        let visible = self.visible_indices();
        if visible.is_empty() {
            return;
        }
        let position = visible
            .iter()
            .position(|index| *index == self.selected)
            .unwrap_or(0);
        let next = if amount.is_negative() {
            position.saturating_sub(amount.unsigned_abs())
        } else {
            (position + amount as usize).min(visible.len() - 1)
        };
        self.selected = visible[next];
    }

    fn select_first(&mut self) {
        if let Some(index) = self.visible_indices().first() {
            self.selected = *index;
        }
    }

    fn select_last(&mut self) {
        if let Some(index) = self.visible_indices().last() {
            self.selected = *index;
        }
    }

    fn begin_input(&mut self, mode: InputMode) {
        self.original_input = if mode == InputMode::Search {
            self.search_query.clone()
        } else {
            String::new()
        };
        self.input = if mode == InputMode::Search {
            self.search_query.clone()
        } else {
            String::new()
        };
        self.input_cursor = self.input.chars().count();
        self.input_mode = mode;
    }

    fn cancel_input(&mut self) {
        if self.input_mode == InputMode::Search {
            self.search_query = self.original_input.clone();
        }
        self.input.clear();
        self.input_cursor = 0;
        self.input_mode = InputMode::Normal;
    }

    fn submit_input(&mut self) {
        match self.input_mode {
            InputMode::Open => {
                let input = self.input.trim().to_string();
                self.input_mode = InputMode::Normal;
                self.input.clear();
                self.input_cursor = 0;
                if !input.is_empty() {
                    self.open(&input);
                }
            }
            InputMode::Search => {
                self.search_query = self.input.trim().to_string();
                self.input_mode = InputMode::Normal;
                self.input.clear();
                self.input_cursor = 0;
                self.select_first();
            }
            InputMode::Normal => {}
        }
    }

    fn open(&mut self, input: &str) {
        let discovery = discover(&[input.to_string()], self.recursive, self.next_id);
        self.next_id = discovery.next_id;
        if discovery.items.is_empty() {
            let message = discovery
                .warnings
                .first()
                .cloned()
                .unwrap_or_else(|| "No playable media found".to_string());
            self.toast(message, ToastKind::Warning);
            return;
        }

        let existing: HashSet<String> = self.items.iter().map(|item| item.source.clone()).collect();
        let first_source = discovery.items[0].source.clone();
        let existing_target = self
            .items
            .iter()
            .find(|item| item.source == first_source)
            .map(|item| item.id);
        let mut added = 0;
        for item in discovery.items {
            if !existing.contains(&item.source) {
                self.items.push(item);
                added += 1;
            }
        }

        let target = existing_target.or_else(|| {
            let first_source = first_source;
            self.items
                .iter()
                .find(|item| item.source == first_source)
                .map(|item| item.id)
        });
        if let Some(id) = target {
            self.play_id(id);
        }
        let kind = if discovery.warnings.is_empty() {
            ToastKind::Success
        } else {
            ToastKind::Warning
        };
        let message = if added == 0 {
            "Opened existing queue item".to_string()
        } else if let Some(warning) = discovery.warnings.first() {
            format!("Added {added} item(s); {warning}")
        } else {
            format!("Added {added} item(s) to the queue")
        };
        self.toast(message, kind);
    }

    fn remove_selected(&mut self) {
        if self.items.is_empty() {
            return;
        }
        let removed = self.items.remove(self.selected);
        if self.current_id == Some(removed.id) {
            self.stop();
        }
        if self.selected >= self.items.len() {
            self.selected = self.items.len().saturating_sub(1);
        }
        self.toast(format!("Removed {}", removed.title), ToastKind::Info);
    }

    fn clear_queue(&mut self) {
        if self.items.is_empty() {
            return;
        }
        self.stop();
        self.items.clear();
        self.selected = 0;
        self.list_offset = 0;
        self.search_query.clear();
        self.toast("Queue cleared", ToastKind::Info);
    }

    fn edit_input(&mut self, edit: InputEdit) {
        let mut characters: Vec<char> = self.input.chars().collect();
        self.input_cursor = self.input_cursor.min(characters.len());
        match edit {
            InputEdit::Character(character) => {
                characters.insert(self.input_cursor, character);
                self.input_cursor += 1;
            }
            InputEdit::Paste(text) => {
                let inserted: Vec<char> = text.chars().collect();
                let inserted_len = inserted.len();
                characters.splice(self.input_cursor..self.input_cursor, inserted);
                self.input_cursor += inserted_len;
            }
            InputEdit::Backspace => {
                if self.input_cursor > 0 {
                    self.input_cursor -= 1;
                    characters.remove(self.input_cursor);
                }
            }
            InputEdit::Delete => {
                if self.input_cursor < characters.len() {
                    characters.remove(self.input_cursor);
                }
            }
            InputEdit::Left => self.input_cursor = self.input_cursor.saturating_sub(1),
            InputEdit::Right => {
                self.input_cursor = (self.input_cursor + 1).min(characters.len())
            }
            InputEdit::Home => self.input_cursor = 0,
            InputEdit::End => self.input_cursor = characters.len(),
            InputEdit::Clear => {
                characters.clear();
                self.input_cursor = 0;
            }
        }
        self.input = characters.into_iter().collect();
        if self.input_mode == InputMode::Search {
            self.search_query = self.input.clone();
            self.select_first();
        }
    }

    pub fn handle_key(&mut self, key: KeyEvent) -> bool {
        if !matches!(key.kind, KeyEventKind::Press | KeyEventKind::Repeat) {
            return false;
        }
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
            return true;
        }
        if self.show_help {
            if matches!(key.code, KeyCode::Esc | KeyCode::Char('?') | KeyCode::Char('q')) {
                self.show_help = false;
            }
            return false;
        }
        if self.input_mode != InputMode::Normal {
            match key.code {
                KeyCode::Esc => self.cancel_input(),
                KeyCode::Enter => self.submit_input(),
                KeyCode::Backspace => self.edit_input(InputEdit::Backspace),
                KeyCode::Delete => self.edit_input(InputEdit::Delete),
                KeyCode::Left => self.edit_input(InputEdit::Left),
                KeyCode::Right => self.edit_input(InputEdit::Right),
                KeyCode::Home => self.edit_input(InputEdit::Home),
                KeyCode::End => self.edit_input(InputEdit::End),
                KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                    self.edit_input(InputEdit::Clear)
                }
                KeyCode::Char(character)
                    if !key.modifiers.intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) =>
                {
                    self.edit_input(InputEdit::Character(character));
                }
                _ => {}
            }
            return false;
        }

        match key.code {
            KeyCode::Char('q') => return true,
            KeyCode::Char('?') | KeyCode::F(1) => self.show_help = true,
            KeyCode::Char(' ') => self.toggle_pause(),
            KeyCode::Enter => self.play_selected(),
            KeyCode::Up | KeyCode::Char('k') => self.move_selection(-1),
            KeyCode::Down | KeyCode::Char('j') => self.move_selection(1),
            KeyCode::PageUp => self.move_selection(-10),
            KeyCode::PageDown => self.move_selection(10),
            KeyCode::Home => self.select_first(),
            KeyCode::End => self.select_last(),
            KeyCode::Left => self.seek_relative(if key.modifiers.contains(KeyModifiers::SHIFT) {
                -30.0
            } else {
                -5.0
            }),
            KeyCode::Right => self.seek_relative(if key.modifiers.contains(KeyModifiers::SHIFT) {
                30.0
            } else {
                5.0
            }),
            KeyCode::Char('n') => self.next(),
            KeyCode::Char('p') => self.previous(),
            KeyCode::Char('x') => self.stop(),
            KeyCode::Char('+') | KeyCode::Char('=') => self.change_volume(5.0),
            KeyCode::Char('-') | KeyCode::Char('_') => self.change_volume(-5.0),
            KeyCode::Char('m') => self.toggle_mute(),
            KeyCode::Char('[') => self.change_speed(-0.1),
            KeyCode::Char(']') => self.change_speed(0.1),
            KeyCode::Char('\\') => {
                let difference = 1.0 - self.playback.speed;
                self.change_speed(difference);
            }
            KeyCode::Char('r') => self.toggle_repeat(),
            KeyCode::Char('s') => self.toggle_shuffle(),
            KeyCode::Char('/') => self.begin_input(InputMode::Search),
            KeyCode::Char('o') | KeyCode::Char('u') => self.begin_input(InputMode::Open),
            KeyCode::Char('d') => self.remove_selected(),
            KeyCode::Char('C') => self.clear_queue(),
            KeyCode::Char('f') => self.fullscreen = !self.fullscreen,
            KeyCode::Char('v') => {
                self.stage_mode = self.stage_mode.next();
                if self.stage_mode == StageMode::Visualizer {
                    self.video.stop();
                }
            }
            KeyCode::Char('a') => match self.player.cycle_audio_track() {
                Ok(()) => self.toast("Switched audio track", ToastKind::Info),
                Err(error) => self.toast(error.to_string(), ToastKind::Error),
            },
            KeyCode::Esc => {
                if !self.search_query.is_empty() {
                    self.search_query.clear();
                    self.list_offset = 0;
                } else if self.fullscreen {
                    self.fullscreen = false;
                }
            }
            _ => {}
        }
        false
    }

    pub fn handle_paste(&mut self, text: String) {
        if self.input_mode != InputMode::Normal {
            self.edit_input(InputEdit::Paste(text));
        }
    }

    pub fn handle_mouse(&mut self, mouse: MouseEvent) {
        if self.show_help || self.input_mode != InputMode::Normal {
            return;
        }
        match mouse.kind {
            MouseEventKind::ScrollUp if contains(self.regions.playlist, mouse.column, mouse.row) => {
                self.move_selection(-3)
            }
            MouseEventKind::ScrollDown
                if contains(self.regions.playlist, mouse.column, mouse.row) =>
            {
                self.move_selection(3)
            }
            MouseEventKind::Down(crossterm::event::MouseButton::Left) => {
                let x = mouse.column;
                let y = mouse.row;
                if contains(self.regions.playlist, x, y) {
                    let row = self.list_offset + usize::from(y - self.regions.playlist.y);
                    if let Some(index) = self.visible_indices().get(row).copied() {
                        self.selected = index;
                        let id = self.items[index].id;
                        let double_click = self
                            .last_click
                            .map(|(time, last_id)| {
                                last_id == id && time.elapsed() < Duration::from_millis(400)
                            })
                            .unwrap_or(false);
                        if double_click {
                            self.play_index(index);
                            self.last_click = None;
                        } else {
                            self.last_click = Some((Instant::now(), id));
                        }
                    }
                } else if contains(self.regions.progress, x, y) {
                    if let Some(duration) = self.duration() {
                        let width = self.regions.progress.width.max(1);
                        let ratio = f64::from(x.saturating_sub(self.regions.progress.x))
                            / f64::from(width.saturating_sub(1).max(1));
                        self.seek_absolute(duration * ratio.clamp(0.0, 1.0));
                    }
                } else if contains(self.regions.play_pause, x, y) {
                    self.toggle_pause();
                } else if contains(self.regions.previous, x, y) {
                    self.previous();
                } else if contains(self.regions.next, x, y) {
                    self.next();
                } else if contains(self.regions.volume_down, x, y) {
                    self.change_volume(-5.0);
                } else if contains(self.regions.volume_up, x, y) {
                    self.change_volume(5.0);
                }
            }
            _ => {}
        }
    }
}

fn contains(area: Rect, x: u16, y: u16) -> bool {
    area.width > 0
        && area.height > 0
        && x >= area.x
        && x < area.x.saturating_add(area.width)
        && y >= area.y
        && y < area.y.saturating_add(area.height)
}

fn number(value: &Value) -> Option<f64> {
    value
        .as_f64()
        .or_else(|| value.as_str().and_then(|value| value.parse().ok()))
        .filter(|value: &f64| value.is_finite())
}

fn boolean(value: &Value) -> Option<bool> {
    value.as_bool().or_else(|| match value.as_str()? {
        "yes" => Some(true),
        "no" => Some(false),
        _ => None,
    })
}

fn string(value: &Value) -> Option<String> {
    value
        .as_str()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

enum InputEdit {
    Character(char),
    Paste(String),
    Backspace,
    Delete,
    Left,
    Right,
    Home,
    End,
    Clear,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn repeat_modes_cycle_predictably() {
        assert_eq!(RepeatMode::Off.next(), RepeatMode::All);
        assert_eq!(RepeatMode::All.next(), RepeatMode::One);
        assert_eq!(RepeatMode::One.next(), RepeatMode::Off);
    }

    #[test]
    fn backend_values_are_safely_converted() {
        assert_eq!(number(&Value::from(2.5)), Some(2.5));
        assert_eq!(number(&Value::from("3.5")), Some(3.5));
        assert_eq!(boolean(&Value::from("yes")), Some(true));
        assert_eq!(boolean(&Value::from("no")), Some(false));
        assert_eq!(number(&Value::Null), None);
    }
}
