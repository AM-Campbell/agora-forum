use ratatui::{
    layout::{Constraint, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Row, Table, Wrap},
    Frame,
};

use crate::tui::app::App;

pub fn render(f: &mut Frame, app: &App, area: Rect) {
    let footer_str = " [Enter] open thread  [?]help  [Esc] back";
    let footer_h = super::footer_height(footer_str, area.width);

    let chunks = Layout::default()
        .constraints([Constraint::Min(1), Constraint::Length(footer_h)])
        .split(area);

    let header_block = Block::default()
        .borders(Borders::TOP | Borders::LEFT | Borders::RIGHT)
        .title(app.header_title("Bookmarks"));

    let inner = header_block.inner(chunks[0]);
    f.render_widget(header_block, chunks[0]);

    if app.bookmarks.is_empty() {
        let msg = Paragraph::new("  No bookmarks yet. Press 'b' in a thread to bookmark it.");
        f.render_widget(msg, inner);
    } else {
        let header = Row::new(vec!["Thread", "Board", "Title"])
            .style(Style::default().add_modifier(Modifier::BOLD));

        let rows: Vec<Row> = app
            .bookmarks
            .iter()
            .enumerate()
            .map(|(i, bm)| {
                let style = if i == app.selected_index {
                    Style::default().add_modifier(Modifier::REVERSED)
                } else {
                    Style::default()
                };

                let title = if bm.thread_title.len() > 35 {
                    format!("{}..", &bm.thread_title[..33])
                } else {
                    bm.thread_title.clone()
                };

                Row::new(vec![
                    format!("#{}", bm.thread_id),
                    bm.board_slug.clone(),
                    title,
                ])
                .style(style)
            })
            .collect();

        let widths = [
            Constraint::Length(8),
            Constraint::Length(14),
            Constraint::Min(20),
        ];

        let table = Table::new(rows, widths).header(header);
        f.render_widget(table, inner);
    }

    let footer = Paragraph::new(Line::from(vec![Span::raw(footer_str)]))
        .block(super::footer_block())
        .wrap(Wrap { trim: false });
    f.render_widget(footer, chunks[1]);
}
