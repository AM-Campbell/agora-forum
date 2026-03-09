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

/// Calculate the height needed for a footer with borders, accounting for text wrapping.
pub fn footer_height(text: &str, area_width: u16) -> u16 {
    let inner_width = area_width.saturating_sub(2) as usize; // 2 for left/right borders
    if inner_width == 0 {
        return 3;
    }
    let lines = (text.len() + inner_width - 1) / inner_width; // ceil division
    (lines as u16).max(1) + 2 // +2 for top/bottom borders
}
