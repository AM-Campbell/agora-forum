use ratatui::{
    style::{Color, Modifier, Style},
    text::{Line, Span},
};

/// Render a post body with basic markdown support.
/// Returns styled lines with the given indent prefix.
pub fn render_body(body: &str, indent: &str, my_username: Option<&str>) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    let mut in_code_block = false;
    let indent = indent.to_string();

    for line in body.lines() {
        if line.trim_start().starts_with("```") {
            in_code_block = !in_code_block;
            lines.push(Line::from(Span::styled(
                format!("{}{}", indent, line),
                Style::default().fg(Color::Cyan),
            )));
            continue;
        }

        if in_code_block {
            lines.push(Line::from(Span::styled(
                format!("{}{}", indent, line),
                Style::default().fg(Color::Cyan),
            )));
            continue;
        }

        // Blockquotes
        if line.starts_with('>') {
            let quote_text = line.strip_prefix("> ").unwrap_or(line.strip_prefix('>').unwrap_or(line));
            lines.push(Line::from(Span::styled(
                format!("{}│ {}", indent, quote_text),
                Style::default().add_modifier(Modifier::DIM),
            )));
            continue;
        }

        // Normal line — parse inline formatting
        let spans = parse_inline(line, my_username);
        let mut prefixed = vec![Span::raw(indent.clone())];
        prefixed.extend(spans);
        lines.push(Line::from(prefixed));
    }

    lines
}

/// Parse inline markdown: **bold**, *italic*, `code`, [text](url), @username
fn parse_inline(text: &str, my_username: Option<&str>) -> Vec<Span<'static>> {
    let mut spans = Vec::new();
    let chars: Vec<char> = text.chars().collect();
    let len = chars.len();
    let mut i = 0;
    let mut buf = String::new();

    while i < len {
        // **bold**
        if i + 1 < len && chars[i] == '*' && chars[i + 1] == '*' {
            if !buf.is_empty() {
                spans.push(Span::raw(buf.clone()));
                buf.clear();
            }
            if let Some(end) = find_closing(&chars, i + 2, &['*', '*']) {
                let content: String = chars[i + 2..end].iter().collect();
                spans.push(Span::styled(content, Style::default().add_modifier(Modifier::BOLD)));
                i = end + 2;
                continue;
            }
        }

        // *italic* (but not **) — only at word boundaries
        if chars[i] == '*' && (i + 1 >= len || chars[i + 1] != '*') {
            let at_word_start = i == 0 || chars[i - 1].is_whitespace();
            if at_word_start {
                if let Some(end) = find_single_closing(&chars, i + 1, '*') {
                    // Closing * must be followed by whitespace, punctuation, or EOL
                    let at_word_end = end + 1 >= len || {
                        let next = chars[end + 1];
                        next.is_whitespace() || next.is_ascii_punctuation()
                    };
                    if at_word_end {
                        if !buf.is_empty() {
                            spans.push(Span::raw(buf.clone()));
                            buf.clear();
                        }
                        let content: String = chars[i + 1..end].iter().collect();
                        spans.push(Span::styled(content, Style::default().add_modifier(Modifier::ITALIC)));
                        i = end + 1;
                        continue;
                    }
                }
            }
        }

        // `code`
        if chars[i] == '`' {
            if !buf.is_empty() {
                spans.push(Span::raw(buf.clone()));
                buf.clear();
            }
            if let Some(end) = find_single_closing(&chars, i + 1, '`') {
                let content: String = chars[i + 1..end].iter().collect();
                spans.push(Span::styled(content, Style::default().fg(Color::Cyan)));
                i = end + 1;
                continue;
            }
        }

        // [text](url)
        if chars[i] == '[' {
            if !buf.is_empty() {
                spans.push(Span::raw(buf.clone()));
                buf.clear();
            }
            if let Some((link_text, end)) = parse_link(&chars, i) {
                spans.push(Span::styled(
                    link_text,
                    Style::default().fg(Color::Blue).add_modifier(Modifier::UNDERLINED),
                ));
                i = end;
                continue;
            }
        }

        // @username
        if chars[i] == '@' && (i == 0 || !chars[i - 1].is_alphanumeric()) {
            if !buf.is_empty() {
                spans.push(Span::raw(buf.clone()));
                buf.clear();
            }
            let start = i + 1;
            let mut end = start;
            while end < len && (chars[end].is_alphanumeric() || chars[end] == '_') {
                end += 1;
            }
            if end > start {
                let username: String = chars[start..end].iter().collect();
                let is_me = my_username.map(|me| me == username).unwrap_or(false);
                let style = if is_me {
                    Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(Color::Yellow)
                };
                spans.push(Span::styled(format!("@{}", username), style));
                i = end;
                continue;
            }
        }

        buf.push(chars[i]);
        i += 1;
    }

    if !buf.is_empty() {
        spans.push(Span::raw(buf));
    }

    spans
}

/// Find closing double-char marker (e.g., **) starting from pos.
fn find_closing(chars: &[char], start: usize, marker: &[char; 2]) -> Option<usize> {
    let len = chars.len();
    let mut i = start;
    while i + 1 < len {
        if chars[i] == marker[0] && chars[i + 1] == marker[1] {
            return Some(i);
        }
        i += 1;
    }
    None
}

/// Find closing single-char marker starting from pos.
fn find_single_closing(chars: &[char], start: usize, marker: char) -> Option<usize> {
    (start..chars.len()).find(|&i| chars[i] == marker)
}

/// Parse [text](url) link, returns (display_text, end_position).
fn parse_link(chars: &[char], start: usize) -> Option<(String, usize)> {
    // Find closing ]
    let mut i = start + 1;
    let len = chars.len();
    while i < len && chars[i] != ']' {
        i += 1;
    }
    if i >= len {
        return None;
    }
    let text: String = chars[start + 1..i].iter().collect();
    i += 1; // skip ]
    if i >= len || chars[i] != '(' {
        return None;
    }
    i += 1; // skip (
    let url_start = i;
    while i < len && chars[i] != ')' {
        i += 1;
    }
    if i >= len {
        return None;
    }
    let _url: String = chars[url_start..i].iter().collect();
    Some((text, i + 1))
}
