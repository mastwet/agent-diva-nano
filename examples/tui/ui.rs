//! UI rendering.

use crate::app::{AppMode, TimelineKind, TuiApp};
use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph, Wrap};

pub fn draw_ui(frame: &mut ratatui::Frame, app: &TuiApp) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(1),
            Constraint::Length(5),
        ])
        .split(frame.area());

    // Status bar
    let status = if app.pending { "processing" } else { "idle" };
    let mode_label = if app.mode == AppMode::Normal {
        ""
    } else {
        " [CONFIG]"
    };
    let status_text = format!(
        "provider: {} | model: {} | session: {} | status: {}{}",
        app.provider_name, app.model, app.session_key, status, mode_label
    );
    frame.render_widget(
        Paragraph::new(status_text).block(
            Block::default()
                .borders(Borders::ALL)
                .title("agent-diva-nano tui"),
        ),
        chunks[0],
    );

    // Timeline
    let mut lines = Vec::new();
    for item in &app.timeline {
        let (label, color) = match item.kind {
            TimelineKind::User => ("user", Color::Cyan),
            TimelineKind::Assistant => ("assistant", Color::Green),
            TimelineKind::Tool => ("tool", Color::Yellow),
            TimelineKind::System => ("system", Color::Blue),
            TimelineKind::Error => ("error", Color::Red),
            TimelineKind::Thinking => ("thinking", Color::DarkGray),
        };
        for text_line in item.text.lines() {
            lines.push(Line::from(vec![
                Span::styled(format!("[{}] ", label), Style::default().fg(color)),
                Span::raw(text_line.to_string()),
            ]));
        }
    }
    let timeline_widget = Paragraph::new(lines)
        .block(Block::default().borders(Borders::ALL).title("timeline"))
        .wrap(Wrap { trim: false })
        .scroll((app.scroll, 0));
    frame.render_widget(timeline_widget, chunks[1]);

    // Input area
    let input_title = app.input_title();
    frame.render_widget(
        Paragraph::new(app.input.clone())
            .block(Block::default().borders(Borders::ALL).title(input_title))
            .wrap(Wrap { trim: false }),
        chunks[2],
    );

    // Cursor positioning
    let area = chunks[2];
    let inner_width = area.width.saturating_sub(2) as usize;
    let char_count = app.input.chars().count();
    let cx = area.x + 1 + (char_count % inner_width.max(1)) as u16;
    let cy = area.y + 1 + (char_count / inner_width.max(1)) as u16;
    frame.set_cursor_position((cx, cy));
}