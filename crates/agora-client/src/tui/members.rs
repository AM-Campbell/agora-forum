use ratatui::{
    layout::{Constraint, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Row, Table, Wrap},
    Frame,
};

use crate::tui::app::App;

pub fn render(f: &mut Frame, app: &App, area: Rect) {
    let online = app.members.iter().filter(|u| u.is_online).count();
    let footer_text = format!(
        " {} members, {} online  [d]m selected  [?]help  [Esc] back",
        app.members.len(),
        online
    );
    let footer_h = super::footer_height(&footer_text, area.width);

    let chunks = Layout::default()
        .constraints([Constraint::Min(1), Constraint::Length(footer_h)])
        .split(area);

    let header_block = Block::default()
        .borders(Borders::TOP | Borders::LEFT | Borders::RIGHT)
        .title(app.header_title("Members"));

    let inner = header_block.inner(chunks[0]);
    f.render_widget(header_block, chunks[0]);

    let header = Row::new(vec!["Username", "Joined", "Invited by", "Posts", "Status", "Bio"])
        .style(Style::default().add_modifier(Modifier::BOLD));

    let rows: Vec<Row> = app
        .members
        .iter()
        .enumerate()
        .map(|(i, user)| {
            let invited = user.invited_by.as_deref().unwrap_or("-");
            let status = if user.is_online { "online" } else { "offline" };
            let bio = if user.bio.is_empty() {
                "-".to_string()
            } else if user.bio.chars().count() > 30 {
                let truncated: String = user.bio.chars().take(27).collect();
                format!("{}...", truncated)
            } else {
                user.bio.clone()
            };

            let style = if i == app.selected_index {
                Style::default().add_modifier(Modifier::REVERSED)
            } else {
                Style::default()
            };

            Row::new(vec![
                user.username.clone(),
                user.joined_at.clone(),
                invited.to_string(),
                user.post_count.to_string(),
                status.to_string(),
                bio,
            ])
            .style(style)
        })
        .collect();

    let widths = [
        Constraint::Length(16),
        Constraint::Length(20),
        Constraint::Length(16),
        Constraint::Length(6),
        Constraint::Length(8),
        Constraint::Min(10),
    ];

    let table = Table::new(rows, widths).header(header);
    f.render_widget(table, inner);

    let footer = Paragraph::new(Line::from(vec![Span::raw(footer_text)]))
        .block(Block::default().borders(Borders::ALL))
        .wrap(Wrap { trim: false });
    f.render_widget(footer, chunks[1]);
}
