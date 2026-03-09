use ratatui::{
    layout::{Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph, Wrap},
    Frame,
};

use crate::cli::image::is_displayable_image;
use crate::tui::app::{App, CachedImage};
use crate::tui::boards::format_relative_time;

/// Maximum height (in terminal rows) for an inline image.
const MAX_IMAGE_HEIGHT: u16 = 20;

/// A segment of thread content: either text lines or an inline image.
enum Segment<'a> {
    Text(Vec<Line<'a>>),
    Image { attachment_id: i64, height: u16 },
}

impl Segment<'_> {
    fn height(&self) -> u16 {
        match self {
            Segment::Text(lines) => lines.len() as u16,
            Segment::Image { height, .. } => *height,
        }
    }
}

/// Calculate image display height maintaining aspect ratio, capped at MAX_IMAGE_HEIGHT.
fn image_display_height(img: &image::DynamicImage, available_width: u16) -> u16 {
    let (iw, ih) = (img.width() as f64, img.height() as f64);
    if iw == 0.0 || ih == 0.0 {
        return 1;
    }
    // Terminal cells are roughly 2:1 (height:width in pixels), so we divide by 2
    let aspect = ih / iw / 2.0;
    let h = (available_width as f64 * aspect).round() as u16;
    h.max(1).min(MAX_IMAGE_HEIGHT)
}

pub fn render(f: &mut Frame, app: &mut App, area: Rect) {
    // Compute footer text first to determine its height
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
    let footer_h = super::footer_height(&footer_text, area.width);

    let chunks = Layout::default()
        .constraints([Constraint::Min(1), Constraint::Length(footer_h)])
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

    let thread_id = app
        .current_thread
        .as_ref()
        .map(|t| t.id)
        .unwrap_or(0);
    let location = format!("{} › {} (#{}){}",  board_slug, title_display, thread_id, flags);
    let header_block = Block::default()
        .borders(Borders::TOP | Borders::LEFT | Borders::RIGHT)
        .title(app.header_title(&location));

    let inner = header_block.inner(chunks[0]);
    f.render_widget(header_block, chunks[0]);

    let has_picker = app.image_picker.is_some();

    // Build segments
    let mut segments: Vec<Segment> = Vec::new();
    let mut prev_post_id: Option<i64> = None;

    for (post_idx, post) in app.posts.iter().enumerate() {
        let mut lines: Vec<Line> = Vec::new();

        // NEW divider
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
        let cursor_indicator = if app.post_cursor == Some(post_idx) { "▶ " } else { "  " };

        // Post header
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

        // Reactions
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

        // Push text lines accumulated so far as a segment
        if !lines.is_empty() {
            segments.push(Segment::Text(lines));
        }

        // Attachments — interleave text labels with image segments
        if !post.is_deleted && !post.attachments.is_empty() {
            let mut att_text: Vec<Line> = vec![Line::from("")];
            for att in &post.attachments {
                let is_image = is_displayable_image(&att.content_type);
                let cached = app.image_cache.contains_key(&att.id);

                if is_image && cached && has_picker {
                    // Flush any pending text lines
                    if !att_text.is_empty() {
                        segments.push(Segment::Text(std::mem::take(&mut att_text)));
                    }
                    // Add image segment — extract DynamicImage for height calc
                    let h = match &app.image_cache[&att.id] {
                        CachedImage::Raw(img) => image_display_height(img, inner.width.saturating_sub(4)),
                        CachedImage::Protocol(_) => MAX_IMAGE_HEIGHT, // fallback
                    };
                    segments.push(Segment::Image {
                        attachment_id: att.id,
                        height: h,
                    });
                    // Filename caption below image
                    att_text.push(Line::from(Span::styled(
                        format!("  {} ({:.1} KB)  agora download {}", att.filename, att.size_bytes as f64 / 1024.0, att.id),
                        Style::default().add_modifier(Modifier::DIM),
                    )));
                } else if is_image && has_picker && !cached {
                    // Image protocol supported but still loading
                    att_text.push(Line::from(Span::styled(
                        format!(
                            "  \u{1F5BC} {} (loading... {:.1} KB)",
                            att.filename,
                            att.size_bytes as f64 / 1024.0
                        ),
                        Style::default().fg(Color::Cyan),
                    )));
                } else {
                    // Non-image attachment, or image on unsupported terminal
                    att_text.push(Line::from(Span::styled(
                        format!(
                            "  📎 {} ({}, {:.1} KB)  agora download {}",
                            att.filename,
                            att.content_type,
                            att.size_bytes as f64 / 1024.0,
                            att.id
                        ),
                        Style::default().fg(Color::Cyan),
                    )));
                }
            }
            if !att_text.is_empty() {
                segments.push(Segment::Text(att_text));
            }
        }

        // Blank line between posts
        segments.push(Segment::Text(vec![Line::from("")]));
    }

    // Calculate total height and render with scroll
    let total_height: u16 = segments.iter().map(|s| s.height()).sum();
    let visible_height = inner.height;
    let max_scroll = (total_height as usize).saturating_sub(visible_height as usize);
    let scroll = app.scroll_offset.min(max_scroll);

    // Render visible segments
    let mut y_offset: i32 = -(scroll as i32);

    for segment in &segments {
        let seg_h = segment.height() as i32;

        // Skip segments entirely above viewport
        if y_offset + seg_h <= 0 {
            y_offset += seg_h;
            continue;
        }
        // Stop if we're past the viewport
        if y_offset >= visible_height as i32 {
            break;
        }

        // Calculate the visible portion of this segment
        let clip_top = if y_offset < 0 { (-y_offset) as u16 } else { 0 };
        let render_y = if y_offset > 0 { y_offset as u16 } else { 0 };
        let available = visible_height.saturating_sub(render_y);
        let render_h = (seg_h as u16).saturating_sub(clip_top).min(available);

        if render_h == 0 {
            y_offset += seg_h;
            continue;
        }

        let seg_rect = Rect::new(inner.x, inner.y + render_y, inner.width, render_h);

        match segment {
            Segment::Text(lines) => {
                let paragraph = Paragraph::new(lines.clone())
                    .scroll((clip_top, 0))
                    .wrap(Wrap { trim: false });
                f.render_widget(paragraph, seg_rect);
            }
            Segment::Image { attachment_id, .. } => {
                if app.image_picker.is_some() {
                    // Convert Raw → Protocol on first render
                    if let Some(CachedImage::Raw(_)) = app.image_cache.get(attachment_id) {
                        if let Some(CachedImage::Raw(img)) = app.image_cache.remove(attachment_id) {
                            let picker = app.image_picker.as_ref().unwrap();
                            let proto = picker.new_resize_protocol(img);
                            app.image_cache.insert(*attachment_id, CachedImage::Protocol(proto));
                        }
                    }
                    // Render the StatefulProtocol
                    if let Some(CachedImage::Protocol(proto)) = app.image_cache.get_mut(attachment_id) {
                        let img_rect = Rect::new(
                            seg_rect.x + 2,
                            seg_rect.y,
                            seg_rect.width.saturating_sub(4),
                            seg_rect.height,
                        );
                        let image_widget = ratatui_image::StatefulImage::default();
                        f.render_stateful_widget(image_widget, img_rect, proto);
                    }
                }
            }
        }

        y_offset += seg_h;
    }

    // Footer
    let footer = Paragraph::new(Line::from(vec![Span::raw(footer_text)]))
        .block(Block::default().borders(Borders::ALL))
        .wrap(Wrap { trim: false });
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
