//! ratatui rendering. Reads [`App`]; never mutates it.

use ratatui::layout::{Alignment, Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line as TuiLine, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Wrap};
use ratatui::Frame;
use unicode_width::UnicodeWidthStr;

use super::app::{App, Line};
use crate::time_util;

pub fn render(f: &mut Frame, app: &App) {
    let area = f.area();
    let chunks = Layout::vertical([
        Constraint::Length(1), // header
        Constraint::Min(1),    // transcript
        Constraint::Length(1), // status
        Constraint::Length(3), // input
    ])
    .split(area);

    render_header(f, chunks[0], app);
    render_transcript(f, chunks[1], app);
    render_status(f, chunks[2], app);
    render_input(f, chunks[3], app);

    if app.picker.is_some() {
        render_picker_hint(f, area);
    }
}

fn render_header(f: &mut Frame, area: Rect, app: &App) {
    let room = app.room_title.as_deref().unwrap_or("방 없음");
    let dot = if app.connected { "●" } else { "○" };
    let line = TuiLine::from(vec![
        Span::styled(
            format!(" {dot} "),
            Style::default().fg(if app.connected { Color::Green } else { Color::Red }),
        ),
        Span::styled(
            format!("kakao-cli — {room}"),
            Style::default().add_modifier(Modifier::BOLD),
        ),
    ]);
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
    let block = Block::default().borders(Borders::ALL).title(" 입력 ");
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
