use ratatui::{
    layout::{Constraint, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Row, Table, Wrap},
    Frame,
};

use crate::cache;
use crate::tui::app::App;
use crate::tui::boards::format_relative_time;

pub fn render(f: &mut Frame, app: &App, area: Rect) {
    // Compute footer text first so we can size its constraint
    let page_info = format!(
        "  page {}/{}",
        app.current_page, app.total_pages
    );
    let page_nav = if app.total_pages > 1 { "  []] next  [[] prev" } else { "" };
    let footer_text = format!(
        " [Enter] open  [n]ew thread  [r]efresh  [?]help  [Esc]{}{}",
        page_nav, page_info
    );
    let footer_h = super::footer_height(&footer_text, area.width);

    let chunks = Layout::default()
        .constraints([Constraint::Min(1), Constraint::Length(footer_h)])
        .split(area);

    let board_name = app
        .current_board
        .as_ref()
        .map(|b| b.slug.as_str())
        .unwrap_or("?");

    let header_block = Block::default()
        .borders(Borders::TOP | Borders::LEFT | Borders::RIGHT)
        .title(app.header_title(board_name));

    let inner = header_block.inner(chunks[0]);
    f.render_widget(header_block, chunks[0]);

    let header = Row::new(vec!["", "#", "Title", "Author", "Posts", "Latest"])
        .style(Style::default().add_modifier(Modifier::BOLD));

    let rows: Vec<Row> = app
        .threads
        .iter()
        .enumerate()
        .map(|(i, t)| {
            let unread = cache::is_thread_unread(&app.cache, t.id, t.latest_post_id);
            let mut prefix = String::new();
            if t.pinned {
                prefix.push_str("📌");
            }
            if t.locked {
                prefix.push_str("🔒");
            }
            if unread {
                prefix.push('*');
            }
            let marker = if prefix.is_empty() { " ".to_string() } else { prefix };
            let title = t.title.clone();
            let author = if t.author.len() > 12 {
                format!("{}..", &t.author[..10])
            } else {
                t.author.clone()
            };
            let latest = format_relative_time(&t.last_post_at);

            let style = if i == app.selected_index {
                Style::default().add_modifier(Modifier::REVERSED)
            } else {
                Style::default()
            };

            Row::new(vec![
                marker.to_string(),
                format!("{}", t.id),
                title,
                author,
                format!("{}", t.post_count),
                latest,
            ])
            .style(style)
        })
        .collect();

    let widths = [
        Constraint::Length(2),
        Constraint::Length(5),
        Constraint::Min(20),
        Constraint::Length(14),
        Constraint::Length(6),
        Constraint::Length(10),
    ];

    let table = Table::new(rows, widths).header(header);
    f.render_widget(table, inner);

    let footer = Paragraph::new(Line::from(vec![Span::raw(footer_text)]))
        .block(super::footer_block())
        .wrap(Wrap { trim: false });
    f.render_widget(footer, chunks[1]);
}
