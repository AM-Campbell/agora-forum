use ratatui::{
    layout::{Constraint, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Row, Table, Wrap},
    Frame,
};

use crate::tui::app::App;

pub fn render(f: &mut Frame, app: &App, area: Rect) {
    let footer_str = " [Enter] switch  [Esc] cancel";
    let footer_h = super::footer_height(footer_str, area.width);

    let chunks = Layout::default()
        .constraints([Constraint::Min(1), Constraint::Length(footer_h)])
        .split(area);

    let header_block = Block::default()
        .borders(Borders::TOP | Borders::LEFT | Borders::RIGHT)
        .title(app.header_title("Switch Server"));

    let inner = header_block.inner(chunks[0]);
    f.render_widget(header_block, chunks[0]);

    let header = Row::new(vec!["", "Name", "Address", "User"])
        .style(Style::default().add_modifier(Modifier::BOLD));

    let rows: Vec<Row> = app
        .servers
        .iter()
        .enumerate()
        .map(|(i, srv)| {
            let marker = if srv.server == app.server_addr {
                "*"
            } else {
                " "
            };
            let name = srv
                .server_name
                .as_deref()
                .unwrap_or("UNNAMED-SERVER");

            let style = if i == app.selected_index {
                Style::default().add_modifier(Modifier::REVERSED)
            } else {
                Style::default()
            };

            Row::new(vec![
                marker.to_string(),
                name.to_string(),
                srv.server.clone(),
                srv.username.clone(),
            ])
            .style(style)
        })
        .collect();

    let widths = [
        Constraint::Length(2),
        Constraint::Length(20),
        Constraint::Min(30),
        Constraint::Length(16),
    ];

    let table = Table::new(rows, widths).header(header);
    f.render_widget(table, inner);

    let footer = Paragraph::new(Line::from(vec![Span::raw(footer_str)]))
        .block(Block::default().borders(Borders::ALL))
        .wrap(Wrap { trim: false });
    f.render_widget(footer, chunks[1]);
}
