use ratatui::{
    layout::{Constraint, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Row, Table, Wrap},
    Frame,
};

use crate::tui::app::App;

pub fn render(f: &mut Frame, app: &App, area: Rect) {
    let footer_str = " [g]enerate new invite  [y]ank code to clipboard  [?]help  [Esc]";
    let footer_h = super::footer_height(footer_str, area.width);

    let chunks = Layout::default()
        .constraints([Constraint::Min(1), Constraint::Length(footer_h)])
        .split(area);

    let header_block = Block::default()
        .borders(Borders::TOP | Borders::LEFT | Borders::RIGHT)
        .title(app.header_title("Invites"));

    let inner = header_block.inner(chunks[0]);
    f.render_widget(header_block, chunks[0]);

    let header = Row::new(vec!["Code", "Status", "Created"])
        .style(Style::default().add_modifier(Modifier::BOLD));

    let rows: Vec<Row> = app
        .invites
        .iter()
        .enumerate()
        .map(|(i, inv)| {
            let status = match &inv.used_by {
                Some(user) => format!("used by: {}", user),
                None => "unused".to_string(),
            };

            let style = if i == app.selected_index {
                Style::default().add_modifier(Modifier::REVERSED)
            } else {
                Style::default()
            };

            Row::new(vec![inv.code.clone(), status, inv.created_at.clone()]).style(style)
        })
        .collect();

    let widths = [
        Constraint::Length(18),
        Constraint::Length(22),
        Constraint::Min(12),
    ];

    let table = Table::new(rows, widths).header(header);
    f.render_widget(table, inner);

    // Remaining invites count
    let unused = app.invites.iter().filter(|i| i.used_by.is_none()).count();
    let remaining = 5usize.saturating_sub(unused);
    let info = Paragraph::new(Line::from(format!("  ({} invites remaining)", remaining)));
    // Render info at bottom of inner area if there's room
    if inner.height > (app.invites.len() as u16 + 2) {
        let info_area = Rect {
            x: inner.x,
            y: inner.y + app.invites.len() as u16 + 2,
            width: inner.width,
            height: 1,
        };
        f.render_widget(info, info_area);
    }

    let footer = Paragraph::new(Line::from(vec![Span::raw(footer_str)]))
        .block(super::footer_block())
        .wrap(Wrap { trim: false });
    f.render_widget(footer, chunks[1]);
}

/// Copy text to clipboard using OSC 52 escape sequence.
pub fn copy_to_clipboard(text: &str) {
    use base64::Engine;
    let encoded = base64::engine::general_purpose::STANDARD.encode(text);
    // OSC 52 sequence
    print!("\x1b]52;c;{}\x07", encoded);
}
