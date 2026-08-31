//! ratatui rendering. Reads [`App`]; never mutates it.

use ratatui::layout::{Alignment, Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line as TuiLine, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Wrap};
use ratatui::Frame;
use unicode_width::UnicodeWidthStr;

use super::app::{App, Line, Screen};
use crate::time_util;

const ROOMS_HINT: &str = " ↑↓ 이동 · Enter 열기 · 글자 입력 = 검색 · Esc 종료";

pub fn render(f: &mut Frame, app: &App) {
    let area = f.area();
    match app.screen {
        Screen::Rooms => render_rooms(f, area, app),
        Screen::Chat => render_chat(f, area, app),
    }
    if app.picker.is_some() {
        render_picker_hint(f, area);
    }
}

// --- room-list screen ----------------------------------------------------

fn render_rooms(f: &mut Frame, area: Rect, app: &App) {
    let chunks = Layout::vertical([
        Constraint::Length(1), // header
        Constraint::Min(1),    // room list
        Constraint::Length(1), // hint
        Constraint::Length(3), // filter box
    ])
    .split(area);

    render_header(f, chunks[0], app, "채팅방");
    render_room_list(f, chunks[1], app);
    f.render_widget(
        Paragraph::new(ROOMS_HINT).style(Style::default().fg(Color::Yellow)),
        chunks[2],
    );
    render_filter(f, chunks[3], app);
}

fn render_room_list(f: &mut Frame, area: Rect, app: &App) {
    let dim = Style::default().fg(Color::DarkGray);

    if app.rooms_loading && app.rooms.is_empty() {
        f.render_widget(Paragraph::new(" 채팅방을 불러오는 중…").style(dim), area);
        return;
    }

    let rooms = app.filtered_rooms();
    if rooms.is_empty() {
        let msg = if app.rooms.is_empty() {
            " 채팅방이 없습니다. 카카오톡 창이 열려 있는지 확인하세요."
        } else {
            " 검색 결과가 없습니다."
        };
        f.render_widget(Paragraph::new(msg).style(dim), area);
        return;
    }

    let viewport = area.height.max(1) as usize;
    let sel = app.rooms_selected.min(rooms.len() - 1);
    let start = sel.saturating_sub(viewport.saturating_sub(1));

    let lines: Vec<TuiLine> = rooms
        .iter()
        .enumerate()
        .skip(start)
        .take(viewport)
        .map(|(i, r)| {
            let selected = i == sel;
            let marker = if selected { "▶ " } else { "  " };
            let unread = if r.unread_count > 0 {
                format!("  ({}건)", r.unread_count)
            } else {
                String::new()
            };
            let preview = r
                .last_message
                .as_ref()
                .filter(|m| !m.text.is_empty())
                .map(|m| format!("   {}", truncate(&m.text, 28)))
                .unwrap_or_default();
            let style = if selected {
                Style::default()
                    .fg(Color::Black)
                    .bg(Color::Cyan)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };
            TuiLine::from(Span::styled(
                format!("{marker}{}{unread}{preview}", r.title),
                style,
            ))
        })
        .collect();

    f.render_widget(Paragraph::new(lines), area);
}

fn render_filter(f: &mut Frame, area: Rect, app: &App) {
    let block = Block::default().borders(Borders::ALL).title(" 검색 ");
    f.render_widget(
        Paragraph::new(format!("> {}", app.rooms_filter)).block(block),
        area,
    );
    let x = area.x + 3 + app.rooms_filter.width() as u16;
    let y = area.y + 1;
    if x < area.x + area.width {
        f.set_cursor_position((x, y));
    }
}

// --- chat screen -------------------------------------------------------

fn render_chat(f: &mut Frame, area: Rect, app: &App) {
    let chunks = Layout::vertical([
        Constraint::Length(1), // header
        Constraint::Min(1),    // transcript
        Constraint::Length(1), // status
        Constraint::Length(3), // input
    ])
    .split(area);

    let title = app.room_title.as_deref().unwrap_or("방 없음");
    render_header(f, chunks[0], app, title);
    render_transcript(f, chunks[1], app);
    render_status(f, chunks[2], app);
    render_input(f, chunks[3], app);
}

fn render_header(f: &mut Frame, area: Rect, app: &App, title: &str) {
    let (dot, dot_color) = if app.offline {
        ("◐", Color::Yellow)
    } else if app.connected {
        ("●", Color::Green)
    } else {
        ("○", Color::Red)
    };
    let mut spans = vec![
        Span::styled(format!(" {dot} "), Style::default().fg(dot_color)),
        Span::styled(
            format!("kakao-cli — {title}"),
            Style::default().add_modifier(Modifier::BOLD),
        ),
    ];
    if app.offline {
        spans.push(Span::styled(
            "  · 캐시(읽기 전용)",
            Style::default().fg(Color::Yellow),
        ));
    }
    let line = TuiLine::from(spans);
    f.render_widget(
        Paragraph::new(line).style(Style::default().bg(Color::Rgb(30, 30, 40))),
        area,
    );
}

fn render_transcript(f: &mut Frame, area: Rect, app: &App) {
    let width = area.width.max(1);
    let lines: Vec<TuiLine> = app.lines.iter().map(render_line).collect();

    // Estimate wrapped height so "follow the latest" lands near the bottom.
    let est: usize = app
        .lines
        .iter()
        .map(|l| wrapped_rows(&plain(l), width))
        .sum();
    let viewport = area.height as usize;
    let max_off = est.saturating_sub(viewport);
    let offset = max_off.saturating_sub(app.scrollback as usize) as u16;

    f.render_widget(
        Paragraph::new(lines)
            .wrap(Wrap { trim: false })
            .scroll((offset, 0)),
        area,
    );
}

fn render_status(f: &mut Frame, area: Rect, app: &App) {
    f.render_widget(
        Paragraph::new(app.status.clone())
            .style(Style::default().fg(Color::Yellow))
            .alignment(Alignment::Left),
        area,
    );
}

fn render_input(f: &mut Frame, area: Rect, app: &App) {
    let block = Block::default().borders(Borders::ALL).title(" 입력 (Esc = 방 목록) ");
    f.render_widget(
        Paragraph::new(format!("> {}", app.input)).block(block),
        area,
    );
    // Cursor after the "> " prefix + input width.
    let x = area.x + 3 + app.input.width() as u16;
    let y = area.y + 1;
    if x < area.x + area.width {
        f.set_cursor_position((x, y));
    }
}

fn render_picker_hint(f: &mut Frame, area: Rect) {
    let w = area.width.min(48);
    let rect = Rect {
        x: area.x + (area.width - w) / 2,
        y: area.y + area.height / 2,
        width: w,
        height: 3,
    };
    f.render_widget(Clear, rect);
    f.render_widget(
        Paragraph::new("번호 키로 방 선택 · Esc 취소")
            .alignment(Alignment::Center)
            .block(Block::default().borders(Borders::ALL).title(" 방 선택 ")),
        rect,
    );
}

fn render_line(line: &Line) -> TuiLine<'static> {
    match line {
        Line::System(text) => TuiLine::from(Span::styled(
            text.clone(),
            Style::default().fg(Color::DarkGray),
        )),
        Line::Msg {
            at,
            who,
            body,
            outgoing,
        } => {
            let hm = time_util::parse_iso(at)
                .map(time_util::local_hm)
                .unwrap_or_else(|| "--:--".into());
            let name_style = if *outgoing {
                Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)
            };
            TuiLine::from(vec![
                Span::styled(format!("[{hm}] "), Style::default().fg(Color::DarkGray)),
                Span::styled(format!("{who}  "), name_style),
                Span::raw(body.clone()),
            ])
        }
    }
}

fn plain(line: &Line) -> String {
    match line {
        Line::System(t) => t.clone(),
        Line::Msg { who, body, .. } => format!("[00:00] {who}  {body}"),
    }
}

fn wrapped_rows(s: &str, width: u16) -> usize {
    let w = s.width().max(1);
    ((w as u16).div_ceil(width.max(1))).max(1) as usize
}

/// Truncate to at most `max` chars, appending `…` when clipped.
fn truncate(s: &str, max: usize) -> String {
    let s = s.replace('\n', " ");
    if s.chars().count() <= max {
        s
    } else {
        let head: String = s.chars().take(max.saturating_sub(1)).collect();
        format!("{head}…")
    }
}
