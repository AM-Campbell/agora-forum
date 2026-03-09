use ratatui::{
    layout::{Constraint, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Row, Table, Wrap},
    Frame,
};

use crate::cache;
use crate::tui::app::App;

pub fn render(f: &mut Frame, app: &App, area: Rect) {
    // Footer — responsive based on terminal width
    let w = area.width as usize;
    let footer_text = if w >= 105 {
        " [Enter] open  [r]efresh  [S]erver  [b]ookmarks  [@]mentions  [i]nvites  [w]ho  [m]sg  [/]search  [?]help  [q]uit".to_string()
    } else if w >= 80 {
        " [Enter] open  [r]efresh  [S]erver  [b]ookmarks  [@]mentions  [/]search  [?]help  [q]uit".to_string()
    } else {
        " [Enter] open  [r]efresh  [?]help  [q]uit".to_string()
    };
    let footer_h = super::footer_height(&footer_text, area.width);

    let chunks = Layout::default()
        .constraints([Constraint::Min(1), Constraint::Length(footer_h)])
        .split(area);

    // Header
    let header_block = Block::default()
        .borders(Borders::TOP | Borders::LEFT | Borders::RIGHT)
        .title(app.header_title("Boards"));

    let inner = header_block.inner(chunks[0]);
    f.render_widget(header_block, chunks[0]);

    // Board table
    let header = Row::new(vec!["#", "Board", "Threads", "Unread", "Latest"])
        .style(Style::default().add_modifier(Modifier::BOLD));

    let rows: Vec<Row> = app
        .boards
        .iter()
        .enumerate()
        .map(|(i, b)| {
            let unread = cache::get_unread_count(&app.cache, b.id);
            let unread_str = if unread > 0 {
                format!("({})", unread)
            } else {
                String::new()
            };
            let latest = b.last_post_at.as_deref().unwrap_or("-");
            let latest = format_relative_time(latest);

            let style = if i == app.selected_index {
                Style::default().add_modifier(Modifier::REVERSED)
            } else {
                Style::default()
            };

            Row::new(vec![
                format!("{}", i + 1),
                b.slug.clone(),
                format!("{}", b.thread_count),
                unread_str,
                latest,
            ])
            .style(style)
        })
        .collect();

    let widths = [
        Constraint::Length(4),
        Constraint::Length(24),
        Constraint::Length(8),
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

pub fn format_relative_time(timestamp: &str) -> String {
    let parsed = chrono::DateTime::parse_from_rfc3339(timestamp)
        .or_else(|_| {
            chrono::NaiveDateTime::parse_from_str(timestamp, "%Y-%m-%d %H:%M:%S")
                .map(|dt| dt.and_utc().fixed_offset())
        });

    match parsed {
        Ok(dt) => {
            let now = chrono::Utc::now();
            let duration = now.signed_duration_since(dt);
            let minutes = duration.num_minutes();
            let hours = duration.num_hours();
            let days = duration.num_days();

            if minutes < 1 {
                "just now".to_string()
            } else if minutes < 60 {
                format!("{}m ago", minutes)
            } else if hours < 24 {
                format!("{}h ago", hours)
            } else if days == 1 {
                "yesterday".to_string()
            } else if days < 30 {
                format!("{}d ago", days)
            } else {
                dt.format("%Y-%m-%d %H:%M").to_string()
            }
        }
        Err(_) => timestamp.to_string(),
    }
}
