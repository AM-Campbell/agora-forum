use ratatui::{
    layout::{Constraint, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Row, Table},
    Frame,
};

use crate::cache;
use crate::tui::app::App;
use crate::tui::boards::format_relative_time;

pub fn render(f: &mut Frame, app: &App, area: Rect) {
    let chunks = Layout::default()
        .constraints([Constraint::Min(1), Constraint::Length(3)])
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
            let unread = cache::is_thread_unread(&app.cache, t.id);
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
            let title = if t.title.len() > 28 {
                format!("{}..", &t.title[..26])
            } else {
                t.title.clone()
            };
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
        Constraint::Length(28),
        Constraint::Length(14),
        Constraint::Length(6),
        Constraint::Min(10),
    ];

    let table = Table::new(rows, widths).header(header);
    f.render_widget(table, inner);

    // Page info + footer
    let page_info = format!(
        "  page {}/{}",
        app.current_page, app.total_pages
    );
    let page_nav = if app.total_pages > 1 { "  []] next  [[] prev" } else { "" };
    let w = area.width as usize;
    let footer_text = if w >= 80 {
        format!(
            " [Enter] open  [n]ew thread  [r]efresh  [?]help  [Esc]{}{}",
            page_nav, page_info
        )
    } else {
        format!(
            " [Enter] open  [n]ew  [?]help  [Esc]{}",
            page_nav
        )
    };
    let footer = Paragraph::new(Line::from(vec![Span::raw(footer_text)]))
        .block(Block::default().borders(Borders::ALL));
    f.render_widget(footer, chunks[1]);
}
