use std::{
    collections::HashSet,
    env, fs,
    path::{Path, PathBuf},
};

use walkdir::WalkDir;

const MAX_DISCOVERED_ITEMS: usize = 10_000;

const AUDIO_EXTENSIONS: &[&str] = &[
    "aac", "aif", "aiff", "alac", "amr", "ape", "au", "cda", "dff", "dsf", "flac", "gsm", "it",
    "m4a", "m4b", "mid", "midi", "mka", "mod", "mp2", "mp3", "mpc", "oga", "ogg", "opus", "ra",
    "ram", "s3m", "shn", "snd", "spx", "tta", "voc", "wav", "wave", "wma", "wv", "xm",
];

const VIDEO_EXTENSIONS: &[&str] = &[
    "3g2", "3gp", "asf", "avi", "divx", "dv", "f4v", "flv", "h264", "h265", "hevc", "m2ts", "m2v",
    "m4v", "mkv", "mov", "mp4", "mpeg", "mpg", "mts", "mxf", "nut", "ogm", "ogv", "rm", "rmvb",
    "swf", "ts", "vob", "webm", "wmv", "y4m",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MediaKind {
    Audio,
    Video,
    Stream,
    Unknown,
}

impl MediaKind {
    pub fn label(self) -> &'static str {
        match self {
            Self::Audio => "AUDIO",
            Self::Video => "VIDEO",
            Self::Stream => "STREAM",
            Self::Unknown => "MEDIA",
        }
    }

    pub fn icon(self) -> &'static str {
        match self {
            Self::Audio => "♪",
            Self::Video => "▶",
            Self::Stream => "◉",
            Self::Unknown => "◆",
        }
    }
}

#[derive(Debug, Clone)]
pub struct MediaItem {
    pub id: u64,
    pub source: String,
    pub title: String,
    pub kind: MediaKind,
    pub duration: Option<f64>,
    pub file_size: Option<u64>,
}

impl MediaItem {
    pub fn is_remote(&self) -> bool {
        is_url(&self.source)
    }

    pub fn may_have_video(&self) -> bool {
        matches!(
            self.kind,
            MediaKind::Video | MediaKind::Stream | MediaKind::Unknown
        )
    }
}

#[derive(Debug, Default)]
pub struct Discovery {
    pub items: Vec<MediaItem>,
    pub warnings: Vec<String>,
    pub next_id: u64,
}

pub fn is_url(value: &str) -> bool {
    let value = value.trim();
    value.contains("://")
        || value.starts_with("magnet:")
        || value.starts_with("ytdl:")
        || value.starts_with("bd://")
        || value.starts_with("dvd://")
}

pub fn expand_path(value: &str) -> PathBuf {
    let value = value.trim();
    if value == "~" || value.starts_with("~/") {
        if let Some(home) = env::var_os("HOME").or_else(|| env::var_os("USERPROFILE")) {
            let mut path = PathBuf::from(home);
            if value.len() > 2 {
                path.push(&value[2..]);
            }
            return path;
        }
    }
    PathBuf::from(value)
}

fn extension(value: &str) -> Option<String> {
    let clean = value.split(['?', '#']).next().unwrap_or(value);
    Path::new(clean)
        .extension()
        .and_then(|extension| extension.to_str())
        .map(str::to_ascii_lowercase)
}

pub fn is_supported_file(path: &Path) -> bool {
    let Some(extension) = path.extension().and_then(|value| value.to_str()) else {
        return false;
    };
    let extension = extension.to_ascii_lowercase();
    AUDIO_EXTENSIONS.contains(&extension.as_str()) || VIDEO_EXTENSIONS.contains(&extension.as_str())
}

fn kind_for_source(source: &str) -> MediaKind {
    let Some(extension) = extension(source) else {
        return if is_url(source) {
            MediaKind::Stream
        } else {
            MediaKind::Unknown
        };
    };

    if AUDIO_EXTENSIONS.contains(&extension.as_str()) {
        MediaKind::Audio
    } else if VIDEO_EXTENSIONS.contains(&extension.as_str()) {
        MediaKind::Video
    } else if is_url(source) {
        MediaKind::Stream
    } else {
        MediaKind::Unknown
    }
}

fn is_playlist(path: &Path) -> bool {
    path.extension()
        .and_then(|value| value.to_str())
        .map(|value| matches!(value.to_ascii_lowercase().as_str(), "m3u" | "m3u8" | "pls"))
        .unwrap_or(false)
}

fn decode_percent_encoding(value: &str) -> String {
    let bytes = value.as_bytes();
    let mut output = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' && index + 2 < bytes.len() {
            let high = (bytes[index + 1] as char).to_digit(16);
            let low = (bytes[index + 2] as char).to_digit(16);
            if let (Some(high), Some(low)) = (high, low) {
                output.push((high * 16 + low) as u8);
                index += 3;
                continue;
            }
        }
        output.push(bytes[index]);
        index += 1;
    }
    String::from_utf8_lossy(&output).into_owned()
}

fn title_for_source(source: &str) -> String {
    if is_url(source) {
        let clean = source.split(['?', '#']).next().unwrap_or(source);
        let final_segment = clean
            .trim_end_matches('/')
            .rsplit('/')
            .next()
            .unwrap_or(clean);
        if !final_segment.is_empty() && !final_segment.contains("://") {
            let title = decode_percent_encoding(final_segment);
            if matches!(kind_for_source(source), MediaKind::Audio | MediaKind::Video) {
                return Path::new(&title)
                    .file_stem()
                    .and_then(|value| value.to_str())
                    .unwrap_or(&title)
                    .replace('_', " ");
            }
            return title;
        }
        return source
            .split("://")
            .nth(1)
            .and_then(|value| value.split('/').next())
            .unwrap_or(source)
            .to_string();
    }

    Path::new(source)
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or(source)
        .replace('_', " ")
}

fn item_from_path(id: u64, path: &Path) -> Result<MediaItem, String> {
    let canonical =
        fs::canonicalize(path).map_err(|error| format!("{}: {error}", path.display()))?;
    let metadata = fs::metadata(&canonical).ok();
    let source = canonical.to_string_lossy().into_owned();
    Ok(MediaItem {
        id,
        title: title_for_source(&source),
        kind: kind_for_source(&source),
        source,
        duration: None,
        file_size: metadata.map(|value| value.len()),
    })
}

fn item_from_input(id: u64, input: &str, base: Option<&Path>) -> Result<MediaItem, String> {
    let input = input.trim();
    if input.is_empty() {
        return Err("The media location is empty".to_string());
    }
    if is_url(input) {
        return Ok(MediaItem {
            id,
            source: input.to_string(),
            title: title_for_source(input),
            kind: kind_for_source(input),
            duration: None,
            file_size: None,
        });
    }

    let expanded = expand_path(input);
    let path = if expanded.is_relative() {
        base.map_or(expanded.clone(), |base| base.join(&expanded))
    } else {
        expanded
    };
    item_from_path(id, &path)
}

/// Discover media from files, directories, playlists, and URLs.
///
/// Explicit files are accepted even when their extension is unknown; directory scans only
/// include known media extensions so opening a project folder does not fill the queue with
/// unrelated files.
pub fn discover(inputs: &[String], recursive: bool, first_id: u64) -> Discovery {
    let mut result = Discovery {
        next_id: first_id,
        ..Discovery::default()
    };
    let mut seen = HashSet::new();

    for input in inputs {
        if result.items.len() >= MAX_DISCOVERED_ITEMS {
            result.warnings.push(format!(
                "Discovery stopped at {MAX_DISCOVERED_ITEMS} items; open a more specific folder"
            ));
            break;
        }

        let trimmed = input.trim();
        if trimmed.is_empty() {
            continue;
        }
        if is_url(trimmed) {
            match item_from_input(result.next_id, trimmed, None) {
                Ok(item) => {
                    result.next_id += 1;
                    if seen.insert(item.source.clone()) {
                        result.items.push(item);
                    }
                }
                Err(error) => result.warnings.push(error),
            }
            continue;
        }

        let path = expand_path(trimmed);
        if !path.exists() {
            result
                .warnings
                .push(format!("{}: no such file or directory", path.display()));
            continue;
        }

        if path.is_dir() {
            let directory_start = result.items.len();
            let mut walker = WalkDir::new(&path).follow_links(false);
            if !recursive {
                walker = walker.max_depth(1);
            }
            let entries = walker.into_iter().filter_entry(|entry| {
                entry.depth() == 0 || !entry.file_name().to_string_lossy().starts_with('.')
            });

            for entry in entries {
                if result.items.len() >= MAX_DISCOVERED_ITEMS {
                    break;
                }
                match entry {
                    Ok(entry) if entry.file_type().is_file() && is_supported_file(entry.path()) => {
                        match item_from_path(result.next_id, entry.path()) {
                            Ok(item) => {
                                result.next_id += 1;
                                if seen.insert(item.source.clone()) {
                                    result.items.push(item);
                                }
                            }
                            Err(error) => result.warnings.push(error),
                        }
                    }
                    Ok(_) => {}
                    Err(error) => result.warnings.push(error.to_string()),
                }
            }
            result.items[directory_start..].sort_by(|left, right| {
                left.title
                    .to_lowercase()
                    .cmp(&right.title.to_lowercase())
                    .then(left.id.cmp(&right.id))
            });
            continue;
        }

        if is_playlist(&path) {
            match import_playlist(&path, result.next_id, &mut seen) {
                Ok((items, next_id)) => {
                    result.next_id = next_id;
                    result.items.extend(items);
                }
                Err(error) => result.warnings.push(error),
            }
            continue;
        }

        match item_from_path(result.next_id, &path) {
            Ok(item) => {
                result.next_id += 1;
                if seen.insert(item.source.clone()) {
                    result.items.push(item);
                }
            }
            Err(error) => result.warnings.push(error),
        }
    }

    // IDs double as the stable original queue order. Assign them after per-directory sorting so
    // disabling shuffle restores exactly the order in which items were presented.
    for (offset, item) in result.items.iter_mut().enumerate() {
        item.id = first_id + offset as u64;
    }
    result.next_id = first_id + result.items.len() as u64;
    result
}

fn import_playlist(
    path: &Path,
    first_id: u64,
    seen: &mut HashSet<String>,
) -> Result<(Vec<MediaItem>, u64), String> {
    let content = fs::read_to_string(path)
        .map_err(|error| format!("{}: could not read playlist: {error}", path.display()))?;
    let base = path.parent().unwrap_or_else(|| Path::new("."));
    let extension = path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    let entries = if extension == "pls" {
        parse_pls(&content)
    } else {
        parse_m3u(&content)
    };

    let mut items = Vec::new();
    let mut next_id = first_id;
    for entry in entries {
        match item_from_input(next_id, &entry.source, Some(base)) {
            Ok(mut item) => {
                next_id += 1;
                if seen.insert(item.source.clone()) {
                    item.title = entry.title.unwrap_or(item.title);
                    item.duration = entry.duration;
                    items.push(item);
                }
            }
            Err(_) => {
                // Playlists often contain stale entries. Keep all usable entries.
                continue;
            }
        }
    }
    Ok((items, next_id))
}

#[derive(Debug, PartialEq)]
struct PlaylistEntry {
    source: String,
    title: Option<String>,
    duration: Option<f64>,
}

fn parse_m3u(content: &str) -> Vec<PlaylistEntry> {
    let mut entries = Vec::new();
    let mut pending_title = None;
    let mut pending_duration = None;

    for line in content.lines().map(str::trim) {
        if let Some(info) = line.strip_prefix("#EXTINF:") {
            let mut parts = info.splitn(2, ',');
            pending_duration = parts
                .next()
                .and_then(|value| value.trim().parse::<f64>().ok())
                .filter(|value| value.is_finite() && *value >= 0.0);
            pending_title = parts
                .next()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_string);
            continue;
        }
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        entries.push(PlaylistEntry {
            source: line.to_string(),
            title: pending_title.take(),
            duration: pending_duration.take(),
        });
    }
    entries
}

fn parse_pls(content: &str) -> Vec<PlaylistEntry> {
    let mut entries = Vec::new();
    for line in content.lines().map(str::trim) {
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        if key
            .get(..4)
            .map(|key| key.eq_ignore_ascii_case("file"))
            .unwrap_or(false)
            && key[4..].chars().all(|character| character.is_ascii_digit())
        {
            entries.push(PlaylistEntry {
                source: value.trim().to_string(),
                title: None,
                duration: None,
            });
        }
    }
    entries
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recognizes_media_and_streams() {
        assert_eq!(kind_for_source("song.FLAC"), MediaKind::Audio);
        assert_eq!(kind_for_source("movie.mkv"), MediaKind::Video);
        assert_eq!(
            kind_for_source("https://example.com/watch?v=1"),
            MediaKind::Stream
        );
        assert!(is_supported_file(Path::new("anything.opus")));
        assert!(!is_supported_file(Path::new("notes.txt")));
    }

    #[test]
    fn parses_extended_m3u_metadata() {
        let entries = parse_m3u(
            "#EXTM3U\n#EXTINF:123.5,Example Artist - Example Song\nmusic/song.mp3\nhttps://example.com/live\n",
        );
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].source, "music/song.mp3");
        assert_eq!(
            entries[0].title.as_deref(),
            Some("Example Artist - Example Song")
        );
        assert_eq!(entries[0].duration, Some(123.5));
        assert_eq!(entries[1].source, "https://example.com/live");
    }

    #[test]
    fn parses_pls_case_insensitively() {
        let entries = parse_pls("[playlist]\nFile1=one.mp3\nTitle1=One\nFILE2=two.ogg\n");
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].source, "one.mp3");
        assert_eq!(entries[1].source, "two.ogg");
    }

    #[test]
    fn url_titles_are_readable() {
        assert_eq!(
            title_for_source("https://host.test/My%20Song.mp3?x=1"),
            "My Song"
        );
        assert_eq!(title_for_source("https://host.test/live"), "live");
    }
}
