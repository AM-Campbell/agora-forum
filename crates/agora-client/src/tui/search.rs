use ratatui::{
    layout::{Constraint, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Row, Table, Wrap},
    Frame,
};

use crate::tui::app::App;

pub fn render(f: &mut Frame, app: &App, area: Rect) {
    let footer_text = if app.search_input_mode {
        " [Enter] search  [Esc] cancel  (tip: by:username to filter by author)"
    } else {
        " [/] new search  [Enter] open thread  [?]help  [Esc] back"
    };
    let footer_h = super::footer_height(footer_text, area.width);

    let chunks = Layout::default()
        .constraints([
            Constraint::Length(3),
            Constraint::Min(1),
            Constraint::Length(footer_h),
        ])
        .split(area);

    // Search input
    let input_text = if app.search_input_mode {
        format!("Search: {}|", app.search_query)
    } else if app.search_query.is_empty() {
        "Search: (press / to type)".to_string()
    } else {
        format!("Search: {}", app.search_query)
    };

    let input = Paragraph::new(input_text)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(app.header_title("Search")),
        );
    f.render_widget(input, chunks[0]);

    // Results
    let results_block = Block::default()
        .borders(Borders::LEFT | Borders::RIGHT);

    let inner = results_block.inner(chunks[1]);
    f.render_widget(results_block, chunks[1]);

    if app.search_results.is_empty() && !app.search_query.is_empty() && !app.search_input_mode {
        let msg = Paragraph::new("  No results found.");
        f.render_widget(msg, inner);
    } else {
        let header = Row::new(vec!["Type", "Thread", "Snippet"])
            .style(Style::default().add_modifier(Modifier::BOLD));

        let rows: Vec<Row> = app
            .search_results
            .iter()
            .enumerate()
            .map(|(i, result)| {
                let title = result
                    .thread_title
                    .as_deref()
                    .unwrap_or("?");
                let kind = match result.kind.as_str() {
                    "thread" => "Thread",
                    "post" => "Post",
                    _ => &result.kind,
                };

                let style = if i == app.selected_index {
                    Style::default().add_modifier(Modifier::REVERSED)
                } else {
                    Style::default()
                };

                Row::new(vec![
                    kind.to_string(),
                    format!("#{} {}", result.thread_id, title),
                    result.snippet.clone(),
                ])
                .style(style)
            })
            .collect();

        let widths = [
            Constraint::Length(8),
            Constraint::Length(30),
            Constraint::Min(20),
        ];

        let table = Table::new(rows, widths).header(header);
        f.render_widget(table, inner);
    }

    let footer = Paragraph::new(Line::from(vec![Span::raw(footer_text)]))
        .block(Block::default().borders(Borders::ALL))
        .wrap(Wrap { trim: false });
    f.render_widget(footer, chunks[2]);
}
