pub mod app;
pub mod boards;
pub mod input;
pub mod bookmarks;
pub mod invites;
pub mod markdown;
pub mod members;
pub mod mentions;
pub mod messages;
pub mod search;
pub mod server_picker;
pub mod status;
pub mod thread;
pub mod threads;

/// Calculate the height needed for a footer with borders and padding, accounting for text wrapping.
pub fn footer_height(text: &str, area_width: u16) -> u16 {
    // 2 for left/right borders + 2 for left/right padding
    let inner_width = area_width.saturating_sub(4) as usize;
    if inner_width == 0 {
        return 3;
    }
    let lines = (text.len() + inner_width - 1) / inner_width; // ceil division
    (lines as u16).max(1) + 2 // +2 for top/bottom borders
}

/// Standard footer block with borders and horizontal padding.
pub fn footer_block() -> ratatui::widgets::Block<'static> {
    ratatui::widgets::Block::default()
        .borders(ratatui::widgets::Borders::ALL)
        .padding(ratatui::widgets::Padding::horizontal(1))
}
