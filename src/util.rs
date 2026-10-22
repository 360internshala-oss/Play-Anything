use std::time::{SystemTime, UNIX_EPOCH};

/// Format a media timestamp without allowing invalid backend values into the UI.
pub fn format_time(seconds: f64) -> String {
    if !seconds.is_finite() || seconds < 0.0 {
        return "--:--".to_string();
    }

    let total = seconds.round() as u64;
    let hours = total / 3600;
    let minutes = (total % 3600) / 60;
    let secs = total % 60;

    if hours > 0 {
        format!("{hours}:{minutes:02}:{secs:02}")
    } else {
        format!("{minutes:02}:{secs:02}")
    }
}

pub fn format_bytes(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    if bytes == 0 {
        return "0 B".to_string();
    }

    let index = ((bytes as f64).log(1024.0).floor() as usize).min(UNITS.len() - 1);
    let value = bytes as f64 / 1024_f64.powi(index as i32);
    if index == 0 {
        format!("{bytes} B")
    } else {
        format!("{value:.1} {}", UNITS[index])
    }
}

pub fn clamp(value: f64, min: f64, max: f64) -> f64 {
    if value.is_finite() {
        value.clamp(min, max)
    } else {
        min
    }
}

/// A tiny, dependency-free PRNG used only to shuffle the queue.
pub fn shuffled_indices(len: usize) -> Vec<usize> {
    let mut indices: Vec<usize> = (0..len).collect();
    if len < 2 {
        return indices;
    }

    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.subsec_nanos() as u64);
    let mut state = nanos ^ (len as u64).rotate_left(17) ^ 0x9e37_79b9_7f4a_7c15;

    for index in (1..len).rev() {
        // xorshift64*: more than sufficient for presentation order, not cryptography.
        state ^= state >> 12;
        state ^= state << 25;
        state ^= state >> 27;
        let random = state.wrapping_mul(0x2545_f491_4f6c_dd1d);
        indices.swap(index, random as usize % (index + 1));
    }

    indices
}

pub fn ellipsize(value: &str, max_chars: usize) -> String {
    if max_chars == 0 {
        return String::new();
    }
    let mut chars = value.chars();
    let prefix: String = chars.by_ref().take(max_chars).collect();
    if chars.next().is_some() {
        let mut result: String = prefix.chars().take(max_chars.saturating_sub(1)).collect();
        result.push('…');
        result
    } else {
        prefix
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn timestamps_cover_minutes_hours_and_invalid_values() {
        assert_eq!(format_time(0.0), "00:00");
        assert_eq!(format_time(65.0), "01:05");
        assert_eq!(format_time(3_661.0), "1:01:01");
        assert_eq!(format_time(f64::NAN), "--:--");
        assert_eq!(format_time(-2.0), "--:--");
    }

    #[test]
    fn sizes_are_human_readable() {
        assert_eq!(format_bytes(512), "512 B");
        assert_eq!(format_bytes(2048), "2.0 KB");
        assert_eq!(format_bytes(5 * 1024 * 1024), "5.0 MB");
    }

    #[test]
    fn shuffle_preserves_every_index() {
        let mut values = shuffled_indices(100);
        values.sort_unstable();
        assert_eq!(values, (0..100).collect::<Vec<_>>());
    }

    #[test]
    fn ellipsizing_never_exceeds_the_requested_character_count() {
        assert_eq!(ellipsize("abcdef", 4), "abc…");
        assert_eq!(ellipsize("abc", 4), "abc");
        assert_eq!(ellipsize("abc", 0), "");
    }
}
