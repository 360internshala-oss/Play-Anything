use ratatui::{
    buffer::Buffer,
    layout::{Alignment, Constraint, Direction, Layout, Margin, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{
        Block, BorderType, Borders, Clear, List, ListItem, Paragraph, Widget, Wrap,
    },
    Frame,
};

use crate::{
    app::{App, InputMode, LayoutRegions, StageMode, ToastKind},
    media::{MediaItem, MediaKind},
    theme,
    util::{ellipsize, format_bytes, format_time},
    video::{VideoFrame, VideoStatus},
};

pub fn render(frame: &mut Frame, app: &mut App) {
    let area = frame.area();
    app.regions = LayoutRegions::default();
    frame.render_widget(
        Block::default().style(Style::default().bg(theme::BACKGROUND)),
        area,
    );

    if area.width < 60 || area.height < 18 {
        render_small_terminal(frame, area);
        return;
    }

    let content = area.inner(Margin {
        horizontal: 1,
        vertical: 0,
    });
    let sections = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(8),
            Constraint::Length(6),
            Constraint::Length(1),
        ])
        .split(content);

    render_header(frame, app, sections[0]);
    if app.fullscreen {
        render_stage(frame, app, sections[1]);
    } else {
        render_body(frame, app, sections[1]);
    }
    render_transport(frame, app, sections[2]);
    render_footer(frame, sections[3]);
    render_toast(frame, app, area);

    if app.show_help {
        render_help(frame, area);
    } else if app.input_mode == InputMode::Open {
        render_open_dialog(frame, app, area);
    }
}

fn render_header(frame: &mut Frame, app: &App, area: Rect) {
    let block = Block::default()
        .borders(Borders::BOTTOM)
        .border_style(Style::default().fg(theme::BORDER))
        .style(Style::default().bg(theme::BACKGROUND));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let columns = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Min(34), Constraint::Length(25), Constraint::Length(29)])
        .split(inner);

    let logo = vec![
        Line::from(vec![
            Span::styled("  ▶ ", Style::default().fg(theme::CYAN)),
            Span::styled(
                "PLAY",
                Style::default()
                    .fg(theme::TEXT)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                " ANYTHING",
                Style::default()
                    .fg(theme::PURPLE)
                    .add_modifier(Modifier::BOLD),
            ),
        ]),
        Line::from(Span::styled(
            "  TERMINAL MEDIA EXPERIENCE",
            Style::default().fg(theme::DIM),
        )),
    ];
    frame.render_widget(Paragraph::new(logo), columns[0]);

    let (status, color) = if !app.connected {
        ("●  DISCONNECTED", theme::RED)
    } else if app.loading {
        ("◌  LOADING", theme::YELLOW)
    } else if app.current_id.is_some() && !app.playback.paused {
        ("●  NOW PLAYING", theme::GREEN)
    } else if app.current_id.is_some() {
        ("❚❚  PAUSED", theme::PURPLE)
    } else {
        ("◇  READY", theme::CYAN)
    };
    let spinner = if app.loading {
        match app.tick / 3 % 4 {
            0 => "⠋",
            1 => "⠙",
            2 => "⠹",
            _ => "⠸",
        }
    } else {
        ""
    };
    let status_area = centered_rect(columns[1], 20, 1);
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(status, Style::default().fg(color).add_modifier(Modifier::BOLD)),
            Span::styled(format!(" {spinner}"), Style::default().fg(theme::YELLOW)),
        ]))
        .alignment(Alignment::Center)
        .style(Style::default().bg(theme::ELEVATED)),
        status_area,
    );

    let backend = if app.connected { "MPV · IPC ONLINE" } else { "MPV · OFFLINE" };
    let right = vec![
        Line::from(Span::styled(
            backend,
            Style::default().fg(if app.connected { theme::MUTED } else { theme::RED }),
        )),
        Line::from(vec![
            Span::styled("VIDEO ", Style::default().fg(theme::DIM)),
            Span::styled(
                if app.video.status() == VideoStatus::Ready {
                    "TERMINAL RGB"
                } else {
                    "AMBIENT / AUTO"
                },
                Style::default().fg(theme::CYAN),
            ),
        ]),
    ];
    frame.render_widget(Paragraph::new(right).alignment(Alignment::Right), columns[2]);
}

fn render_body(frame: &mut Frame, app: &mut App, area: Rect) {
    let sidebar_width = (area.width / 3).clamp(27, 40);
    let columns = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Length(sidebar_width), Constraint::Min(30)])
        .split(area);
    render_queue(frame, app, columns[0]);

    let info_height = if columns[1].height >= 14 { 6 } else { 5 };
    let right = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(5), Constraint::Length(info_height)])
        .split(columns[1]);
    render_stage(frame, app, right[0]);
    render_track_info(frame, app, right[1]);
}

fn render_queue(frame: &mut Frame, app: &mut App, area: Rect) {
    let title = if app.search_query.is_empty() {
        format!(" QUEUE · {} ", app.items.len())
    } else {
        format!(" QUEUE · {} MATCHES ", app.visible_indices().len())
    };
    let block = theme::panel(title, theme::CYAN);
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let sections = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(2), Constraint::Min(1)])
        .split(inner);
    let search_text = if app.input_mode == InputMode::Search {
        format!(" / {}▌", app.input)
    } else if !app.search_query.is_empty() {
        format!(" / {}  ·  Esc clears", app.search_query)
    } else {
        " ⌕  / filter   ·   o open media".to_string()
    };
    frame.render_widget(
        Paragraph::new(search_text).style(Style::default().fg(if app.input_mode == InputMode::Search {
            theme::CYAN
        } else {
            theme::DIM
        })),
        sections[0],
    );

    let list_area = sections[1];
    app.regions.playlist = list_area;
    let visible = app.visible_indices();
    if visible.is_empty() {
        let empty = if app.items.is_empty() {
            vec![
                Line::from(""),
                Line::from(Span::styled(
                    "YOUR QUEUE IS EMPTY",
                    Style::default().fg(theme::MUTED).add_modifier(Modifier::BOLD),
                )),
                Line::from(""),
                Line::from(Span::styled("Press o to open a file,", Style::default().fg(theme::DIM))),
                Line::from(Span::styled("folder, playlist, or URL", Style::default().fg(theme::DIM))),
            ]
        } else {
            vec![
                Line::from(""),
                Line::from(Span::styled("NO MATCHES", Style::default().fg(theme::MUTED))),
                Line::from(Span::styled("Esc clears the filter", Style::default().fg(theme::DIM))),
            ]
        };
        frame.render_widget(Paragraph::new(empty).alignment(Alignment::Center), list_area);
        return;
    }

    let selected_position = visible
        .iter()
        .position(|index| *index == app.selected)
        .unwrap_or(0);
    let capacity = usize::from(list_area.height).max(1);
    if selected_position < app.list_offset {
        app.list_offset = selected_position;
    } else if selected_position >= app.list_offset + capacity {
        app.list_offset = selected_position + 1 - capacity;
    }
    let max_offset = visible.len().saturating_sub(capacity);
    app.list_offset = app.list_offset.min(max_offset);

    let rows = visible
        .iter()
        .skip(app.list_offset)
        .take(capacity)
        .map(|index| {
            let item = &app.items[*index];
            let selected = *index == app.selected;
            let playing = app.current_id == Some(item.id);
            let marker = if playing {
                "▶"
            } else if selected {
                "▸"
            } else {
                item.kind.icon()
            };
            let marker_color = if playing {
                theme::GREEN
            } else if selected {
                theme::CYAN
            } else {
                theme::DIM
            };
            let duration = item.duration.map(format_time).unwrap_or_else(|| {
                if item.is_remote() {
                    "URL".to_string()
                } else {
                    "--:--".to_string()
                }
            });
            let title_width = usize::from(list_area.width).saturating_sub(13).max(4);
            let title = ellipsize(&item.title, title_width);
            let row_style = if selected {
                Style::default().bg(theme::ELEVATED)
            } else {
                Style::default().bg(theme::SURFACE)
            };
            ListItem::new(Line::from(vec![
                Span::styled(if selected { "▌" } else { " " }, row_style.fg(theme::CYAN)),
                Span::styled(format!("{marker} "), Style::default().fg(marker_color)),
                Span::styled(
                    title,
                    Style::default().fg(if selected { theme::TEXT } else { theme::MUTED }),
                ),
                Span::styled(" ", row_style),
                Span::styled(duration, Style::default().fg(theme::DIM)),
            ]))
            .style(row_style)
        })
        .collect::<Vec<_>>();
    frame.render_widget(List::new(rows), list_area);
}

fn render_stage(frame: &mut Frame, app: &mut App, area: Rect) {
    let accent = if app.current_id.is_some() && !app.playback.paused {
        theme::CYAN
    } else {
        theme::PURPLE
    };
    let title = format!(" NOW PLAYING · {} ", app.stage_mode.label());
    let block = theme::panel(title, accent);
    let inner = block.inner(area);
    frame.render_widget(block, area);
    app.regions.stage = inner;

    if app.current_item().is_none() {
        render_empty_stage(frame, inner);
        return;
    }

    if app.stage_mode != StageMode::Visualizer {
        if let Some(video) = app.video.frame() {
            frame.render_widget(TerminalVideo { frame: video }, inner);
            if app.playback.paused {
                render_stage_badge(frame, inner, "❚❚  PAUSED", theme::PURPLE);
            }
            return;
        }
    }

    frame.render_widget(
        AmbientVisualizer {
            phase: app.position(),
            paused: app.playback.paused,
            tint: if app.current_item().map(|item| item.kind) == Some(MediaKind::Audio) {
                theme::PURPLE
            } else {
                theme::CYAN
            },
        },
        inner,
    );

    if app.stage_mode != StageMode::Visualizer && app.current_item().map(|item| item.may_have_video()).unwrap_or(false) {
        let (message, detail) = match app.video.status() {
            VideoStatus::Loading => (
                "PREPARING TERMINAL VIDEO",
                "Decoding in the background · audio remains synchronized",
            ),
            VideoStatus::Unavailable => (
                "AUDIO-ONLY PREVIEW",
                app.video
                    .status_detail()
                    .unwrap_or("No decodable video stream · audio playback is unaffected"),
            ),
            _ if app.loading => ("OPENING MEDIA", "Resolving streams and reading metadata"),
            _ => ("AMBIENT VISUALIZER", "Press v to switch the stage view"),
        };
        let box_area = centered_rect(inner, inner.width.saturating_sub(4).min(68), 3);
        frame.render_widget(
            Paragraph::new(vec![
                Line::from(Span::styled(
                    message,
                    Style::default().fg(theme::TEXT).add_modifier(Modifier::BOLD),
                )),
                Line::from(Span::styled(detail, Style::default().fg(theme::MUTED))),
            ])
            .alignment(Alignment::Center)
            .wrap(Wrap { trim: true })
            .style(Style::default().bg(theme::SURFACE)),
            box_area,
        );
    } else {
        render_stage_badge(frame, inner, "AMBIENT VISUALIZER", theme::PURPLE);
    }

    if app.playback.paused {
        render_stage_badge(frame, inner, "❚❚  PAUSED", theme::PURPLE);
    }
}

fn render_stage_badge(frame: &mut Frame, area: Rect, text: &str, color: Color) {
    if area.width < 8 || area.height < 2 {
        return;
    }
    let width = (text.chars().count() as u16 + 2).min(area.width);
    let badge = Rect {
        x: area.x + area.width.saturating_sub(width + 1),
        y: area.y,
        width,
        height: 1,
    };
    frame.render_widget(
        Paragraph::new(text).alignment(Alignment::Center).style(Style::default().fg(color).bg(theme::SURFACE)),
        badge,
    );
}

fn render_empty_stage(frame: &mut Frame, area: Rect) {
    let content = centered_rect(area, area.width.min(58), 7.min(area.height));
    let lines = vec![
        Line::from(Span::styled(
            "╭──────────────────────╮",
            Style::default().fg(theme::BORDER),
        )),
        Line::from(vec![
            Span::styled("│         ", Style::default().fg(theme::BORDER)),
            Span::styled("▶", Style::default().fg(theme::CYAN).add_modifier(Modifier::BOLD)),
            Span::styled("          │", Style::default().fg(theme::BORDER)),
        ]),
        Line::from(Span::styled(
            "╰──────────────────────╯",
            Style::default().fg(theme::BORDER),
        )),
        Line::from(""),
        Line::from(Span::styled(
            "MAKE SOME NOISE.",
            Style::default().fg(theme::TEXT).add_modifier(Modifier::BOLD),
        )),
        Line::from(Span::styled(
            "Open anything. Press o to begin.",
            Style::default().fg(theme::MUTED),
        )),
        Line::from(Span::styled(
            "Local media · streams · web URLs · playlists",
            Style::default().fg(theme::DIM),
        )),
    ];
    frame.render_widget(Paragraph::new(lines).alignment(Alignment::Center), content);
}

fn render_track_info(frame: &mut Frame, app: &App, area: Rect) {
    let block = theme::panel(" TRACK DETAILS ", theme::PURPLE);
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let Some(item) = app.current_item() else {
        frame.render_widget(
            Paragraph::new("No media loaded · your track metadata will appear here")
                .style(Style::default().fg(theme::DIM)),
            inner,
        );
        return;
    };

    let columns = if inner.width >= 72 {
        Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Min(34), Constraint::Length(29)])
            .split(inner)
    } else {
        Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Min(20)])
            .split(inner)
    };
    let title = app.playback.media_title.as_deref().unwrap_or(&item.title);
    let creator = match (app.playback.artist.as_deref(), app.playback.album.as_deref()) {
        (Some(artist), Some(album)) => format!("{artist}  ·  {album}"),
        (Some(artist), None) => artist.to_string(),
        (None, Some(album)) => album.to_string(),
        (None, None) if item.is_remote() => "Network stream".to_string(),
        (None, None) => "Local media library".to_string(),
    };
    let source = if let Some(size) = item.file_size {
        format!("{}  ·  {}", ellipsize(&item.source, 72), format_bytes(size))
    } else {
        ellipsize(&item.source, 88)
    };
    let queue_position = app
        .items
        .iter()
        .position(|candidate| candidate.id == item.id)
        .map(|index| index + 1)
        .unwrap_or(0);
    let left = vec![
        Line::from(Span::styled(
            ellipsize(title, 74),
            Style::default().fg(theme::TEXT).add_modifier(Modifier::BOLD),
        )),
        Line::from(Span::styled(creator, Style::default().fg(theme::PURPLE))),
        Line::from(Span::styled(source, Style::default().fg(theme::DIM))),
        Line::from(vec![
            Span::styled(format!(" {} ", item.kind.label()), Style::default().fg(theme::CYAN)),
            Span::styled("  ·  ", Style::default().fg(theme::BORDER)),
            Span::styled(
                format!("QUEUE {queue_position}/{}", app.items.len()),
                Style::default().fg(theme::MUTED),
            ),
        ]),
    ];
    frame.render_widget(Paragraph::new(left), columns[0]);

    if columns.len() > 1 {
        let audio = match (app.playback.sample_rate, app.playback.channels) {
            (Some(rate), Some(channels)) => format!("{} kHz · {channels} ch", rate as f64 / 1000.0),
            (Some(rate), None) => format!("{} kHz", rate as f64 / 1000.0),
            _ => "Detecting…".to_string(),
        };
        let video = match (app.playback.video_width, app.playback.video_height) {
            (Some(width), Some(height)) => format!("{width} × {height}"),
            _ if item.kind == MediaKind::Audio => "Audio only".to_string(),
            _ => "Terminal scaled".to_string(),
        };
        let codec = app
            .playback
            .audio_codec
            .as_deref()
            .or(app.playback.video_codec.as_deref())
            .unwrap_or("Auto")
            .to_uppercase();
        let right = vec![
            Line::from(vec![
                Span::styled("CODEC  ", Style::default().fg(theme::DIM)),
                Span::styled(codec, Style::default().fg(theme::MUTED)),
            ]),
            Line::from(vec![
                Span::styled("AUDIO  ", Style::default().fg(theme::DIM)),
                Span::styled(audio, Style::default().fg(theme::MUTED)),
            ]),
            Line::from(vec![
                Span::styled("VIDEO  ", Style::default().fg(theme::DIM)),
                Span::styled(video, Style::default().fg(theme::MUTED)),
            ]),
            Line::from(vec![
                Span::styled("ENGINE ", Style::default().fg(theme::DIM)),
                Span::styled("mpv + FFmpeg", Style::default().fg(theme::CYAN)),
            ]),
        ];
        frame.render_widget(Paragraph::new(right).alignment(Alignment::Right), columns[1]);
    }
}

fn render_transport(frame: &mut Frame, app: &mut App, area: Rect) {
    let block = Block::default()
        .borders(Borders::TOP)
        .border_style(Style::default().fg(theme::BORDER))
        .style(Style::default().bg(theme::BACKGROUND));
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let sections = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(2), Constraint::Min(1)])
        .split(inner);
    render_progress(frame, app, sections[0]);

    let columns = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(30), Constraint::Percentage(40), Constraint::Percentage(30)])
        .split(sections[1]);

    let volume_area = centered_rect(columns[0], 23, 1);
    let minus = Rect { width: 3, ..volume_area };
    let plus = Rect {
        x: volume_area.x + volume_area.width.saturating_sub(3),
        width: 3,
        ..volume_area
    };
    app.regions.volume_down = minus;
    app.regions.volume_up = plus;
    frame.render_widget(
        Paragraph::new(" − ").alignment(Alignment::Center).style(Style::default().fg(theme::MUTED)),
        minus,
    );
    frame.render_widget(
        Paragraph::new(" + ").alignment(Alignment::Center).style(Style::default().fg(theme::MUTED)),
        plus,
    );
    let volume_label = if app.playback.muted {
        format!("MUTED  {:3.0}%", app.playback.volume)
    } else {
        format!("VOL  {:3.0}%", app.playback.volume)
    };
    let volume_text = Rect {
        x: minus.x + 3,
        width: volume_area.width.saturating_sub(6),
        ..volume_area
    };
    frame.render_widget(
        Paragraph::new(volume_label).alignment(Alignment::Center).style(Style::default().fg(
            if app.playback.muted { theme::RED } else { theme::CYAN },
        )),
        volume_text,
    );

    let controls = centered_rect(columns[1], 21, 1);
    let previous = Rect { width: 6, ..controls };
    let play = Rect { x: controls.x + 7, width: 7, ..controls };
    let next = Rect { x: controls.x + 15, width: 6, ..controls };
    app.regions.previous = previous;
    app.regions.play_pause = play;
    app.regions.next = next;
    frame.render_widget(
        Paragraph::new("  ⏮  ").alignment(Alignment::Center).style(Style::default().fg(theme::MUTED)),
        previous,
    );
    let play_text = if app.playback.paused { "  ▶  " } else { "  ❚❚  " };
    frame.render_widget(
        Paragraph::new(play_text)
            .alignment(Alignment::Center)
            .style(Style::default().fg(theme::BACKGROUND).bg(theme::CYAN).add_modifier(Modifier::BOLD)),
        play,
    );
    frame.render_widget(
        Paragraph::new("  ⏭  ").alignment(Alignment::Center).style(Style::default().fg(theme::MUTED)),
        next,
    );

    let mode_text = format!(
        "SHUFFLE {} · REPEAT {} · {:.1}×",
        if app.shuffled { "ON" } else { "OFF" },
        app.repeat.label(),
        app.playback.speed
    );
    frame.render_widget(
        Paragraph::new(mode_text).alignment(Alignment::Right).style(Style::default().fg(theme::MUTED)),
        centered_rect(columns[2], columns[2].width.min(36), 1),
    );
}

fn render_progress(frame: &mut Frame, app: &mut App, area: Rect) {
    let position = app.position();
    let duration = app.duration();
    let left_width = if duration.map(|value| value >= 3600.0).unwrap_or(false) { 9 } else { 7 };
    let right_width = left_width;
    let bar = Rect {
        x: area.x + left_width + 1,
        y: area.y,
        width: area.width.saturating_sub(left_width + right_width + 2),
        height: 1,
    };
    app.regions.progress = bar;
    frame.render_widget(
        Paragraph::new(format_time(position)).style(Style::default().fg(theme::TEXT)),
        Rect { width: left_width, ..area },
    );
    let right_label = match duration {
        Some(duration) => format_time(duration),
        None if app.current_item().map(MediaItem::is_remote).unwrap_or(false) => "LIVE".to_string(),
        None => "--:--".to_string(),
    };
    frame.render_widget(
        Paragraph::new(right_label).alignment(Alignment::Right).style(Style::default().fg(theme::MUTED)),
        Rect {
            x: area.x + area.width.saturating_sub(right_width),
            width: right_width,
            ..area
        },
    );

    if bar.width == 0 {
        return;
    }
    let progress = app.progress();
    let knob = ((bar.width.saturating_sub(1)) as f64 * progress).round() as u16;
    let buffer = frame.buffer_mut();
    for offset in 0..bar.width {
        let played = offset <= knob && duration.is_some();
        let character = if offset == knob && duration.is_some() {
            "●"
        } else if played {
            "━"
        } else {
            "─"
        };
        buffer[(bar.x + offset, bar.y)].set_symbol(character).set_fg(if played {
            theme::CYAN
        } else {
            theme::BORDER
        });
    }
}

fn render_footer(frame: &mut Frame, area: Rect) {
    let line = Line::from(vec![
        key("space"),
        label(" pause   "),
        key("←/→"),
        label(" seek   "),
        key("enter"),
        label(" play   "),
        key("o"),
        label(" open   "),
        key("/"),
        label(" search   "),
        key("?"),
        label(" help   "),
        key("q"),
        label(" quit"),
    ]);
    frame.render_widget(Paragraph::new(line).alignment(Alignment::Center), area);
}

fn key(text: &'static str) -> Span<'static> {
    Span::styled(format!(" {text} "), Style::default().fg(theme::CYAN))
}

fn label(text: &'static str) -> Span<'static> {
    Span::styled(text, Style::default().fg(theme::DIM))
}

fn render_toast(frame: &mut Frame, app: &App, area: Rect) {
    let Some(toast) = app.toast.as_ref().filter(|toast| toast.until > std::time::Instant::now()) else {
        return;
    };
    let color = match toast.kind {
        ToastKind::Info => theme::CYAN,
        ToastKind::Success => theme::GREEN,
        ToastKind::Warning => theme::YELLOW,
        ToastKind::Error => theme::RED,
    };
    let width = area.width.saturating_sub(8).clamp(30, 76);
    let toast_area = Rect {
        x: area.x + (area.width.saturating_sub(width)) / 2,
        y: area.height.saturating_sub(9),
        width,
        height: 3,
    };
    frame.render_widget(Clear, toast_area);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(color))
        .style(Style::default().bg(theme::ELEVATED));
    let inner = block.inner(toast_area);
    frame.render_widget(block, toast_area);
    frame.render_widget(
        Paragraph::new(toast.message.as_str())
            .alignment(Alignment::Center)
            .wrap(Wrap { trim: true })
            .style(Style::default().fg(theme::TEXT)),
        inner,
    );
}

fn render_open_dialog(frame: &mut Frame, app: &App, area: Rect) {
    let dialog = centered_rect(area, area.width.saturating_sub(6).min(76), 7.min(area.height));
    frame.render_widget(Clear, dialog);
    let block = theme::panel(" OPEN MEDIA · FILE / FOLDER / URL ", theme::CYAN)
        .border_style(Style::default().fg(theme::CYAN).bg(theme::ELEVATED))
        .style(Style::default().bg(theme::ELEVATED));
    let inner = block.inner(dialog);
    frame.render_widget(block, dialog);
    let lines = vec![
        Line::from(Span::styled(
            "Enter a local path, direct stream, web URL, or playlist:",
            Style::default().fg(theme::MUTED),
        )),
        Line::from(""),
        input_line(app),
        Line::from(""),
        Line::from(vec![
            Span::styled(" Enter ", Style::default().fg(theme::CYAN)),
            Span::styled(" open & play    ", Style::default().fg(theme::DIM)),
            Span::styled(" Esc ", Style::default().fg(theme::PURPLE)),
            Span::styled(" cancel    ~ is supported", Style::default().fg(theme::DIM)),
        ]),
    ];
    frame.render_widget(Paragraph::new(lines), inner);
}

fn input_line(app: &App) -> Line<'static> {
    let characters: Vec<char> = app.input.chars().collect();
    let cursor = app.input_cursor.min(characters.len());
    let before: String = characters[..cursor].iter().collect();
    let current = characters.get(cursor).copied().unwrap_or(' ');
    let after: String = if cursor < characters.len() {
        characters[cursor + 1..].iter().collect()
    } else {
        String::new()
    };
    Line::from(vec![
        Span::styled(" › ", Style::default().fg(theme::CYAN)),
        Span::styled(before, Style::default().fg(theme::TEXT)),
        Span::styled(current.to_string(), Style::default().fg(theme::BACKGROUND).bg(theme::CYAN)),
        Span::styled(after, Style::default().fg(theme::TEXT)),
    ])
}

fn render_help(frame: &mut Frame, area: Rect) {
    let dialog = centered_rect(
        area,
        area.width.saturating_sub(4).min(76),
        area.height.saturating_sub(2).min(25),
    );
    frame.render_widget(Clear, dialog);
    let block = theme::panel(" PLAY ANYTHING · KEYBOARD MAP ", theme::PURPLE)
        .border_style(Style::default().fg(theme::PURPLE).bg(theme::ELEVATED))
        .style(Style::default().bg(theme::ELEVATED));
    let inner = block.inner(dialog);
    frame.render_widget(block, dialog);

    let lines = vec![
        help_section("TRANSPORT"),
        help_row("Space", "Play / pause"),
        help_row("Enter", "Play selected queue item"),
        help_row("← / →", "Seek 5 seconds · Shift seeks 30"),
        help_row("n / p / x", "Next / previous / stop"),
        help_row("+ / − / m", "Volume up / down / mute"),
        help_row("[ / ] / \\", "Slower / faster / reset speed"),
        help_row("a", "Cycle embedded audio tracks"),
        help_section("QUEUE"),
        help_row("j / k or ↑ / ↓", "Move selection"),
        help_row("PgUp / PgDn", "Jump ten items"),
        help_row("Home / End", "First / last item"),
        help_row("o or u", "Open file, folder, playlist, or URL"),
        help_row("/ then Enter", "Filter queue · Esc clears filter"),
        help_row("d / C", "Remove selected / clear entire queue"),
        help_row("r / s", "Repeat off→all→one / shuffle on→off"),
        help_section("VIEW & MOUSE"),
        help_row("f / v", "Focus stage / cycle video-visualizer"),
        help_row("Click / double-click", "Select / play · click bar to seek"),
        help_row("Mouse wheel", "Scroll the queue"),
        help_section("GENERAL"),
        help_row("? / F1 / q", "Toggle this help / quit"),
        Line::from(Span::styled(
            "Ctrl+C always quits · Esc closes this panel",
            Style::default().fg(theme::DIM),
        )),
    ];
    frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), inner);
}

fn help_section(title: &'static str) -> Line<'static> {
    Line::from(vec![
        Span::raw(""),
        Span::styled(title, Style::default().fg(theme::CYAN).add_modifier(Modifier::BOLD)),
    ])
}

fn help_row(key: &'static str, description: &'static str) -> Line<'static> {
    Line::from(vec![
        Span::styled(format!(" {key:<20}", key = key), Style::default().fg(theme::PURPLE)),
        Span::styled(description, Style::default().fg(theme::MUTED)),
    ])
}

fn render_small_terminal(frame: &mut Frame, area: Rect) {
    let content = centered_rect(area, area.width.min(48), area.height.min(7));
    let lines = vec![
        Line::from(Span::styled(
            "▶ PLAY ANYTHING",
            Style::default().fg(theme::CYAN).add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        Line::from(Span::styled("A little more room, please.", Style::default().fg(theme::TEXT))),
        Line::from(Span::styled("Minimum terminal size: 60 × 18", Style::default().fg(theme::MUTED))),
        Line::from(Span::styled("Resize to keep your place in the queue.", Style::default().fg(theme::DIM))),
        Line::from(""),
        Line::from(Span::styled("q to quit", Style::default().fg(theme::PURPLE))),
    ];
    frame.render_widget(Paragraph::new(lines).alignment(Alignment::Center), content);
}

fn centered_rect(area: Rect, width: u16, height: u16) -> Rect {
    let width = width.min(area.width);
    let height = height.min(area.height);
    Rect {
        x: area.x + (area.width.saturating_sub(width)) / 2,
        y: area.y + (area.height.saturating_sub(height)) / 2,
        width,
        height,
    }
}

struct TerminalVideo<'a> {
    frame: &'a VideoFrame,
}

impl Widget for TerminalVideo<'_> {
    fn render(self, area: Rect, buffer: &mut Buffer) {
        if area.width == 0 || area.height == 0 || self.frame.width == 0 || self.frame.height == 0 {
            return;
        }
        for y in 0..area.height {
            for x in 0..area.width {
                let source_x = usize::from(x) * self.frame.width / usize::from(area.width);
                let source_top = usize::from(y) * 2 * self.frame.height
                    / (usize::from(area.height) * 2);
                let source_bottom = (usize::from(y) * 2 + 1) * self.frame.height
                    / (usize::from(area.height) * 2);
                let top = pixel(self.frame, source_x, source_top);
                let bottom = pixel(self.frame, source_x, source_bottom);
                buffer[(area.x + x, area.y + y)]
                    .set_char('▀')
                    .set_fg(top)
                    .set_bg(bottom);
            }
        }
    }
}

fn pixel(frame: &VideoFrame, x: usize, y: usize) -> Color {
    let x = x.min(frame.width.saturating_sub(1));
    let y = y.min(frame.height.saturating_sub(1));
    let index = (y * frame.width + x) * 3;
    if index + 2 >= frame.pixels.len() {
        return Color::Black;
    }
    Color::Rgb(frame.pixels[index], frame.pixels[index + 1], frame.pixels[index + 2])
}

struct AmbientVisualizer {
    phase: f64,
    paused: bool,
    tint: Color,
}

impl Widget for AmbientVisualizer {
    fn render(self, area: Rect, buffer: &mut Buffer) {
        if area.width < 4 || area.height < 3 {
            return;
        }
        let count = (usize::from(area.width) / 2).clamp(8, 64);
        let spacing = usize::from(area.width) / count;
        let baseline = area.y + area.height * 3 / 4;
        let max_height = usize::from(area.height) / 2;
        let phase = if self.paused { 0.35 } else { self.phase };

        for x in 0..area.width {
            buffer[(area.x + x, baseline)].set_char('─').set_fg(theme::BORDER);
        }
        for index in 0..count {
            let i = index as f64;
            let wave = (i * 0.41 + phase * 2.7).sin() * 0.45
                + (i * 0.17 - phase * 1.9).cos() * 0.30
                + (i * 0.73 + phase * 1.2).sin() * 0.25;
            let envelope = 0.35 + 0.65 * ((i / count as f64) * std::f64::consts::PI).sin();
            let energy = (0.14 + wave.abs() * 0.86) * envelope * if self.paused { 0.32 } else { 1.0 };
            let height = (energy * max_height as f64).round() as usize;
            let x = area.x + (index * spacing + spacing / 2) as u16;
            if x >= area.x + area.width {
                continue;
            }
            for level in 0..height {
                let y = baseline.saturating_sub(level as u16 + 1);
                if y < area.y {
                    continue;
                }
                let ratio = level as f64 / max_height.max(1) as f64;
                let color = if ratio > 0.72 { theme::PINK } else { self.tint };
                buffer[(x, y)].set_char('█').set_fg(color);
            }
            for reflection in 0..(height / 3) {
                let y = baseline + reflection as u16 + 1;
                if y >= area.y + area.height {
                    break;
                }
                buffer[(x, y)].set_char('░').set_fg(theme::BORDER);
            }
        }
    }
}
