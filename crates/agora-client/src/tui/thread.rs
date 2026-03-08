use ratatui::{
    layout::{Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph, Wrap},
    Frame,
};

use crate::tui::app::App;
use crate::tui::boards::format_relative_time;

pub fn render(f: &mut Frame, app: &App, area: Rect) {
    let chunks = Layout::default()
        .constraints([Constraint::Min(1), Constraint::Length(3)])
        .split(area);

    let thread_title = app
        .current_thread
        .as_ref()
        .map(|t| t.title.as_str())
        .unwrap_or("?");
    let board_slug = app
        .current_thread
        .as_ref()
        .map(|t| t.board_slug.as_str())
        .unwrap_or("?");

    let title_display = if thread_title.len() > 40 {
        format!("{}..", &thread_title[..38])
    } else {
        thread_title.to_string()
    };

    // Show pinned/locked flags
    let mut flags = String::new();
    if let Some(t) = &app.current_thread {
        if t.pinned {
            flags.push_str(" [PINNED]");
        }
        if t.locked {
            flags.push_str(" [LOCKED]");
        }
    }

    let location = format!("{} › {}{}", board_slug, title_display, flags);
    let header_block = Block::default()
        .borders(Borders::TOP | Borders::LEFT | Borders::RIGHT)
        .title(app.header_title(&location));

    let inner = header_block.inner(chunks[0]);
    f.render_widget(header_block, chunks[0]);

    // Build post content as lines
    let mut lines: Vec<Line> = Vec::new();
    let mut prev_post_id: Option<i64> = None;

    for (post_idx, post) in app.posts.iter().enumerate() {
        // NEW divider: insert before first post that's newer than last-read
        if let Some(last_read_id) = app.last_read_post_id {
            let prev_is_old = prev_post_id.map(|pid| pid <= last_read_id).unwrap_or(true);
            if post.id > last_read_id && prev_is_old && prev_post_id.is_some() {
                let divider_width = inner.width.saturating_sub(4) as usize;
                let label = " NEW ";
                let side = divider_width.saturating_sub(label.len()) / 2;
                let divider_text = format!(
                    "  {}{}{}",
                    "─".repeat(side),
                    label,
                    "─".repeat(divider_width.saturating_sub(side + label.len()))
                );
                lines.push(Line::from(Span::styled(
                    divider_text,
                    Style::default().fg(Color::Green).add_modifier(Modifier::BOLD),
                )));
                lines.push(Line::from(""));
            }
        }
        prev_post_id = Some(post.id);

        let time = format_relative_time(&post.created_at);

        // Post-selection indicator
        let cursor_indicator = if app.post_cursor == Some(post_idx) { "▶ " } else { "  " };

        // Post header with edit/deleted indicators
        let mut header_spans = vec![
            Span::styled(
                format!("{}[#{}] {}", cursor_indicator, post.post_number, post.author),
                Style::default().add_modifier(Modifier::BOLD),
            ),
            Span::raw(format!("  {}", time)),
        ];

        if post.edited_at.is_some() {
            header_spans.push(Span::styled(
                " (edited)",
                Style::default().fg(Color::Yellow).add_modifier(Modifier::DIM),
            ));
        }

        if post.is_deleted {
            header_spans.push(Span::styled(
                " [DELETED]",
                Style::default().fg(Color::Red),
            ));
        }

        lines.push(Line::from(header_spans));

        // Reply-to indicator
        if let Some(parent_num) = post.parent_post_number {
            let parent_author = post.parent_author.as_deref().unwrap_or("?");
            lines.push(Line::from(Span::styled(
                format!("  ↳ re: #{} ({})", parent_num, parent_author),
                Style::default().fg(Color::Cyan).add_modifier(Modifier::DIM),
            )));
        }

        // Separator
        lines.push(Line::from(format!(
            "  {}",
            "─".repeat(inner.width.saturating_sub(4) as usize)
        )));

        // Post body
        if post.is_deleted {
            lines.push(Line::from(Span::styled(
                "  [This post has been deleted by a moderator]",
                Style::default().add_modifier(Modifier::DIM | Modifier::ITALIC),
            )));
        } else {
            let body_lines = crate::tui::markdown::render_body(&post.body, "  ", Some(&app.username));
            lines.extend(body_lines);
        }

        // Reactions (skip for deleted posts)
        if !post.is_deleted && !post.reactions.is_empty() {
            let mut reaction_spans: Vec<Span> = vec![Span::raw("  ")];
            for rc in &post.reactions {
                let emoji = match rc.reaction.as_str() {
                    "thumbsup" => "\u{1F44D}",
                    "check" => "\u{2705}",
                    "heart" => "\u{2764}\u{FE0F}",
                    "think" => "\u{1F914}",
                    "laugh" => "\u{1F602}",
                    other => other,
                };
                let style = if rc.reacted_by_me {
                    Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
                } else {
                    Style::default().add_modifier(Modifier::DIM)
                };
                reaction_spans.push(Span::styled(format!("{} {} ", emoji, rc.count), style));
            }
            lines.push(Line::from(reaction_spans));
        }

        // Attachments
        if !post.attachments.is_empty() {
            lines.push(Line::from(""));
            for att in &post.attachments {
                lines.push(Line::from(Span::styled(
                    format!(
                        "  📎 {} ({}, {:.1} KB)",
                        att.filename,
                        att.content_type,
                        att.size_bytes as f64 / 1024.0
                    ),
                    Style::default().fg(Color::Cyan),
                )));
            }
        }

        lines.push(Line::from(""));
    }

    // Calculate scroll
    let visible_height = inner.height as usize;
    let total_lines = lines.len();
    let max_scroll = total_lines.saturating_sub(visible_height);
    let scroll = app.scroll_offset.min(max_scroll);

    let paragraph = Paragraph::new(lines)
        .scroll((scroll as u16, 0))
        .wrap(Wrap { trim: false });

    f.render_widget(paragraph, inner);

    // Footer
    let post_info = if !app.posts.is_empty() {
        format!(
            "  posts 1-{} of {}  page {}/{}",
            app.posts.len(),
            app.posts.len(),
            app.current_page,
            app.total_pages
        )
    } else {
        String::new()
    };

    let page_nav = if app.total_pages > 1 {
        format!("  []] next  [[] prev{}", post_info)
    } else {
        post_info
    };

    let w = area.width as usize;
    let footer_text = if app.post_cursor.is_some() {
        if w >= 70 {
            format!(
                " POST MODE: [j/k] select  [R]eply-to  [+] react  [Esc] exit{}",
                page_nav
            )
        } else {
            format!(" POST: [j/k]  [R]eply  [+]react  [Esc]{}", page_nav)
        }
    } else if w >= 90 {
        format!(
            " [n]ew reply  [e]dit  [b]ookmark  [Tab] post mode  [j/k] scroll  [r]efresh  [?]help  [Esc]{}",
            page_nav
        )
    } else if w >= 70 {
        format!(
            " [n]ew  [e]dit  [b]ookmark  [Tab] posts  [?]help  [Esc]{}",
            page_nav
        )
    } else {
        format!(" [n]ew  [Tab] posts  [?]help  [Esc]{}", page_nav)
    };

    let footer = Paragraph::new(Line::from(vec![Span::raw(footer_text)]))
        .block(Block::default().borders(Borders::ALL));
    f.render_widget(footer, chunks[1]);

    // Reaction picker popup
    if app.show_reaction_picker {
        let popup_width = 52u16;
        let popup_height = 5u16;
        let popup_x = area.x + (area.width.saturating_sub(popup_width)) / 2;
        let popup_y = area.y + (area.height.saturating_sub(popup_height)) / 2;
        let popup_area = Rect::new(popup_x, popup_y, popup_width, popup_height);
        f.render_widget(Clear, popup_area);
        let picker = Paragraph::new(vec![
            Line::from(""),
            Line::from("  1:\u{1F44D} thumbs up  2:\u{2705} check  3:\u{2764}\u{FE0F} heart  4:\u{1F914} think  5:\u{1F602} laugh"),
            Line::from(""),
        ])
        .block(Block::default().borders(Borders::ALL).title(" React "));
        f.render_widget(picker, popup_area);
    }
}
