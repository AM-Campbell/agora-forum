use ratatui::{
    layout::{Constraint, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Row, Table, Wrap},
    Frame,
};

use crate::tui::app::App;

/// Render the inbox (conversation list) view.
pub fn render_inbox(f: &mut Frame, app: &App, area: Rect) {
    let chunks = Layout::default()
        .constraints([Constraint::Min(1), Constraint::Length(3)])
        .split(area);

    let header_block = Block::default()
        .borders(Borders::TOP | Borders::LEFT | Borders::RIGHT)
        .title(app.header_title("Messages"));

    let inner = header_block.inner(chunks[0]);
    f.render_widget(header_block, chunks[0]);

    if app.dm_conversations.is_empty() {
        let msg = Paragraph::new("  No conversations yet. Press 'n' to start one.");
        f.render_widget(msg, inner);
    } else {
        let header = Row::new(vec!["User", "Messages", "Last message"])
            .style(Style::default().add_modifier(Modifier::BOLD));

        let rows: Vec<Row> = app
            .dm_conversations
            .iter()
            .enumerate()
            .map(|(i, conv)| {
                let style = if i == app.selected_index {
                    Style::default().add_modifier(Modifier::REVERSED)
                } else {
                    Style::default()
                };

                Row::new(vec![
                    conv.username.clone(),
                    conv.message_count.to_string(),
                    conv.last_message_at.clone(),
                ])
                .style(style)
            })
            .collect();

        let widths = [
            Constraint::Length(16),
            Constraint::Length(10),
            Constraint::Min(20),
        ];

        let table = Table::new(rows, widths).header(header);
        f.render_widget(table, inner);
    }

    let footer = Paragraph::new(Line::from(vec![Span::raw(
        " [Enter] open  [n]ew message  [r]efresh  [?]help  [Esc] back",
    )]))
    .block(Block::default().borders(Borders::ALL));
    f.render_widget(footer, chunks[1]);
}

/// Render a specific DM conversation (decrypted messages).
pub fn render_thread(f: &mut Frame, app: &App, area: Rect) {
    let chunks = Layout::default()
        .constraints([Constraint::Min(1), Constraint::Length(3)])
        .split(area);

    let partner = app
        .dm_partner
        .as_deref()
        .unwrap_or("?");

    let location = format!("Messages › {}", partner);
    let header_block = Block::default()
        .borders(Borders::TOP | Borders::LEFT | Borders::RIGHT)
        .title(app.header_title(&location));

    let inner = header_block.inner(chunks[0]);
    f.render_widget(header_block, chunks[0]);

    // Build message lines
    let mut lines: Vec<Line> = Vec::new();

    for msg in &app.dm_decrypted {
        lines.push(Line::from(vec![
            Span::styled(
                format!("[{}] ", msg.0),
                Style::default().add_modifier(Modifier::DIM),
            ),
            Span::styled(
                format!("{}:", msg.1),
                Style::default().add_modifier(Modifier::BOLD),
            ),
        ]));
        lines.push(Line::from(format!("  {}", msg.2)));
        lines.push(Line::from(""));
    }

    let content = Paragraph::new(lines)
        .wrap(Wrap { trim: false })
        .scroll((app.scroll_offset as u16, 0));
    f.render_widget(content, inner);

    let footer = Paragraph::new(Line::from(vec![Span::raw(
        " [j/k] scroll  [n]ew reply  [?]help  [Esc] back to inbox",
    )]))
    .block(Block::default().borders(Borders::ALL));
    f.render_widget(footer, chunks[1]);
}
