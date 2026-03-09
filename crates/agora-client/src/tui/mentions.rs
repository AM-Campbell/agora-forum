use ratatui::{
    layout::{Constraint, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Wrap},
    Frame,
};

use crate::tui::app::App;

const LINES_PER_ITEM: usize = 4; // header, meta, snippet, blank

pub fn render(f: &mut Frame, app: &App, area: Rect) {
    let footer_text = format!(
        " {} mentions  [Enter] open  [?]help  [Esc] back",
        app.mentions.len()
    );
    let footer_h = super::footer_height(&footer_text, area.width);

    let chunks = Layout::default()
        .constraints([Constraint::Min(1), Constraint::Length(footer_h)])
        .split(area);

    let header_block = Block::default()
        .borders(Borders::TOP | Borders::LEFT | Borders::RIGHT)
        .title(app.header_title("@Mentions"));

    let inner = header_block.inner(chunks[0]);
    f.render_widget(header_block, chunks[0]);

    if app.mentions.is_empty() {
        let msg = Paragraph::new("  No mentions found.")
            .wrap(Wrap { trim: false });
        f.render_widget(msg, inner);
    } else {
        let mut lines: Vec<Line> = Vec::new();
        for (i, m) in app.mentions.iter().enumerate() {
            let style = if i == app.selected_index {
                Style::default().add_modifier(Modifier::REVERSED)
            } else {
                Style::default()
            };

            lines.push(Line::from(vec![
                Span::styled(
                    format!("  [Thread #{}] ", m.thread_id),
                    style.add_modifier(Modifier::BOLD),
                ),
                Span::styled(m.thread_title.clone(), style),
            ]));
            lines.push(Line::from(Span::styled(
                format!("    by {} — {}", m.author, m.created_at),
                Style::default().add_modifier(Modifier::DIM),
            )));
            lines.push(Line::from(Span::styled(
                format!("    {}", m.snippet),
                Style::default(),
            )));
            lines.push(Line::from(""));
        }

        // Scroll to keep selected item visible
        let visible_height = inner.height as usize;
        let selected_top = app.selected_index * LINES_PER_ITEM;
        let scroll = if selected_top + LINES_PER_ITEM > visible_height {
            selected_top.saturating_sub(visible_height.saturating_sub(LINES_PER_ITEM))
        } else {
            0
        };

        let paragraph = Paragraph::new(lines)
            .scroll((scroll as u16, 0))
            .wrap(Wrap { trim: false });
        f.render_widget(paragraph, inner);
    }

    let footer = Paragraph::new(Line::from(vec![Span::raw(footer_text)]))
        .block(super::footer_block())
        .wrap(Wrap { trim: false });
    f.render_widget(footer, chunks[1]);
}
