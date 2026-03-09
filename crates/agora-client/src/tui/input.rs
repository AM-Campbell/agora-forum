use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use super::app::{App, View};

#[derive(Debug, Clone, PartialEq)]
pub enum Action {
    None,
    Quit,

    // Data fetching
    FetchBoards,
    FetchThreads { slug: String, page: i64 },
    FetchThread { thread_id: i64, page: i64 },
    FetchInvites,
    FetchMembers,
    FetchInbox,
    FetchBookmarks,
    FetchMentions,
    SearchQuery { query: String, by: Option<String> },

    // Mutations
    ToggleBookmark { thread_id: i64 },
    ReactToPost { thread_id: i64, post_id: i64, reaction: String },
    DeletePost { thread_id: i64, post_id: i64 },
    GenerateInvite,

    // Pagination
    NextPage { context: PageContext },
    PrevPage { context: PageContext },

    // Editor flows (suspend TUI → editor → API → resume)
    OpenEditor { kind: EditorKind },

    // Clipboard
    CopyToClipboard { text: String },

    // Navigate into items (fetch + push view)
    EnterBoard { index: usize },
    EnterThread { thread_id: i64 },
    EnterSearchResult { thread_id: i64 },
    EnterBookmark { thread_id: i64 },
    EnterMention { thread_id: i64 },
    EnterConversation { username: String },

    // Server switching
    SwitchServer { server_addr: String },
}

#[derive(Debug, Clone, PartialEq)]
pub enum PageContext {
    Threads { slug: String },
    Thread { thread_id: i64 },
}

#[derive(Debug, Clone, PartialEq)]
pub enum EditorKind {
    NewThread { board_slug: String },
    Reply { thread_id: i64, thread_title: String, board_slug: String },
    ReplyTo {
        thread_id: i64,
        thread_title: String,
        board_slug: String,
        parent_post_id: i64,
        parent_author: String,
        parent_body: String,
    },
    EditPost { thread_id: i64, post_id: i64, old_body: String, thread_title: String },
    NewDm,
    DmToUser { recipient: String },
}

/// Parse "by:username rest of query" from TUI search input.
/// Returns (query_text, Option<username>).
pub fn parse_search_query(input: &str) -> (String, Option<String>) {
    let trimmed = input.trim();
    if let Some(rest) = trimmed.strip_prefix("by:") {
        let mut parts = rest.splitn(2, ' ');
        let username = parts.next().unwrap_or("").to_string();
        let query = parts.next().unwrap_or("").trim().to_string();
        if username.is_empty() {
            (trimmed.to_string(), None)
        } else {
            (query, Some(username))
        }
    } else {
        (trimmed.to_string(), None)
    }
}

/// Build the reaction picker item list: recent emoji (with digit labels) followed by all emoji.
/// Returns Vec of (emoji_char, display_label).
pub fn reaction_picker_items(recent: &[String], emojis: &[crate::config::EmojiEntry]) -> Vec<(String, String)> {
    let mut items = Vec::new();
    // Recent reactions with digit key labels (0-9)
    for (i, emoji) in recent.iter().take(10).enumerate() {
        // Find label from config, fall back to just the emoji
        let label = emojis.iter()
            .find(|e| e.emoji == *emoji)
            .map(|e| e.label.as_str())
            .unwrap_or("");
        if label.is_empty() {
            items.push((emoji.clone(), format!("[{}] {}", i, emoji)));
        } else {
            items.push((emoji.clone(), format!("[{}] {} {}", i, emoji, label)));
        }
    }
    // All emoji from config
    for entry in emojis {
        items.push((entry.emoji.clone(), format!("    {} {}", entry.emoji, entry.label)));
    }
    items
}

/// Pure key handler: mutates App state and returns an Action describing
/// what side effect (if any) the main loop should execute.
pub fn handle_key(app: &mut App, key: KeyEvent) -> Action {
    // Help overlay intercepts
    if app.show_help {
        match key.code {
            KeyCode::Char('?') | KeyCode::Esc => app.show_help = false,
            _ => {}
        }
        return Action::None;
    }

    // Search input mode intercepts all keys
    if app.search_input_mode {
        match key.code {
            KeyCode::Esc => {
                app.search_input_mode = false;
            }
            KeyCode::Enter => {
                app.search_input_mode = false;
                if !app.search_query.is_empty() {
                    let (q, by) = parse_search_query(&app.search_query);
                    return Action::SearchQuery { query: q, by };
                }
            }
            KeyCode::Backspace => {
                app.search_query.pop();
            }
            KeyCode::Char(c) => {
                app.search_query.push(c);
            }
            _ => {}
        }
        return Action::None;
    }

    // Ctrl+C always quits
    if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
        return Action::Quit;
    }

    app.status_message = None;

    // Reaction picker intercepts
    if app.show_reaction_picker {
        let picker_items = reaction_picker_items(&app.recent_reactions, &app.emojis);
        let reaction = match key.code {
            KeyCode::Esc => {
                app.show_reaction_picker = false;
                app.reaction_picker_scroll = 0;
                return Action::None;
            }
            // Digit keys 0-9 select from recent reactions
            KeyCode::Char(c @ '0'..='9') => {
                let idx = c.to_digit(10).unwrap() as usize;
                app.recent_reactions.get(idx).cloned()
            }
            // Enter selects the highlighted item
            KeyCode::Enter => {
                picker_items.get(app.reaction_picker_scroll).map(|(emoji, _)| emoji.clone())
            }
            // j/k or arrows to scroll
            KeyCode::Char('j') | KeyCode::Down => {
                if app.reaction_picker_scroll + 1 < picker_items.len() {
                    app.reaction_picker_scroll += 1;
                }
                return Action::None;
            }
            KeyCode::Char('k') | KeyCode::Up => {
                app.reaction_picker_scroll = app.reaction_picker_scroll.saturating_sub(1);
                return Action::None;
            }
            _ => None,
        };
        if let Some(reaction) = reaction {
            app.show_reaction_picker = false;
            app.reaction_picker_scroll = 0;
            if let (Some(thread), Some(cursor)) = (&app.current_thread, app.post_cursor) {
                if let Some(post) = app.posts.get(cursor) {
                    return Action::ReactToPost {
                        thread_id: thread.id,
                        post_id: post.id,
                        reaction,
                    };
                }
            }
        }
        return Action::None;
    }

    // Post-selection mode in thread view
    if app.post_cursor.is_some() && app.current_view() == &View::Thread {
        match key.code {
            KeyCode::Esc | KeyCode::Tab => {
                app.post_cursor = None;
                return Action::None;
            }
            KeyCode::Char('j') | KeyCode::Down => {
                let max = app.posts.len().saturating_sub(1);
                if let Some(ref mut c) = app.post_cursor {
                    if *c < max {
                        *c += 1;
                    }
                }
                return Action::None;
            }
            KeyCode::Char('k') | KeyCode::Up => {
                if let Some(ref mut c) = app.post_cursor {
                    *c = c.saturating_sub(1);
                }
                return Action::None;
            }
            KeyCode::Char('+') => {
                app.recent_reactions = crate::cache::get_recent_reactions(&app.cache);
                app.reaction_picker_scroll = 0;
                app.show_reaction_picker = true;
                return Action::None;
            }
            KeyCode::Char('d') => {
                // Delete selected post (if it's yours or you're mod/admin)
                if let Some(thread) = &app.current_thread.clone() {
                    if let Some(cursor) = app.post_cursor {
                        if let Some(post) = app.posts.get(cursor) {
                            if post.is_deleted {
                                app.status_message = Some("Post is already deleted.".to_string());
                            } else if post.author != app.username {
                                app.status_message = Some("You can only delete your own posts.".to_string());
                            } else {
                                return Action::DeletePost {
                                    thread_id: thread.id,
                                    post_id: post.id,
                                };
                            }
                        }
                    }
                }
                return Action::None;
            }
            KeyCode::Char('e') => {
                // Edit selected post (if it's yours)
                if let Some(thread) = &app.current_thread.clone() {
                    if let Some(cursor) = app.post_cursor {
                        if let Some(post) = app.posts.get(cursor) {
                            if post.is_deleted {
                                app.status_message = Some("Cannot edit a deleted post.".to_string());
                            } else if post.author != app.username {
                                app.status_message = Some("You can only edit your own posts.".to_string());
                            } else {
                                return Action::OpenEditor {
                                    kind: EditorKind::EditPost {
                                        thread_id: thread.id,
                                        post_id: post.id,
                                        old_body: post.body.clone(),
                                        thread_title: thread.title.clone(),
                                    },
                                };
                            }
                        }
                    }
                }
                return Action::None;
            }
            KeyCode::Char('R') => {
                // Reply-to selected post
                if let Some(thread) = &app.current_thread.clone() {
                    if thread.locked {
                        app.status_message = Some("Thread is locked".to_string());
                        return Action::None;
                    }
                    if let Some(cursor) = app.post_cursor {
                        if let Some(parent_post) = app.posts.get(cursor) {
                            return Action::OpenEditor {
                                kind: EditorKind::ReplyTo {
                                    thread_id: thread.id,
                                    thread_title: thread.title.clone(),
                                    board_slug: thread.board_slug.clone(),
                                    parent_post_id: parent_post.id,
                                    parent_author: parent_post.author.clone(),
                                    parent_body: parent_post.body.clone(),
                                },
                            };
                        }
                    }
                }
                return Action::None;
            }
            KeyCode::Char('n') => {
                // Exit post mode and fall through to normal 'n' handler (new reply)
                app.post_cursor = None;
            }
            _ => return Action::None,
        }
    }

    // Server picker view intercepts
    if app.current_view() == &View::ServerPicker {
        match key.code {
            KeyCode::Char('j') | KeyCode::Down => {
                app.move_down();
                return Action::None;
            }
            KeyCode::Char('k') | KeyCode::Up => {
                app.move_up();
                return Action::None;
            }
            KeyCode::Enter => {
                if let Some(srv) = app.servers.get(app.selected_index) {
                    let addr = srv.server.clone();
                    app.pop_view();
                    return Action::SwitchServer { server_addr: addr };
                }
                return Action::None;
            }
            KeyCode::Esc | KeyCode::Char('q') => {
                app.pop_view();
                return Action::None;
            }
            _ => return Action::None,
        }
    }

    match key.code {
        KeyCode::Char('q') | KeyCode::Esc => {
            if !app.pop_view() {
                return Action::Quit;
            }
            Action::None
        }
        KeyCode::Char('j') | KeyCode::Down => {
            app.move_down();
            Action::None
        }
        KeyCode::Char('k') | KeyCode::Up => {
            app.move_up();
            Action::None
        }
        KeyCode::Char('g') => {
            if app.current_view() == &View::Invites {
                Action::GenerateInvite
            } else {
                app.go_top();
                Action::None
            }
        }
        KeyCode::Char('G') => {
            app.go_bottom();
            Action::None
        }
        KeyCode::PageUp => {
            app.page_up();
            Action::None
        }
        KeyCode::PageDown => {
            app.page_down();
            Action::None
        }
        KeyCode::Char('?') => {
            app.show_help = true;
            Action::None
        }
        KeyCode::Char('i') => Action::FetchInvites,
        KeyCode::Char('w') => Action::FetchMembers,
        KeyCode::Char('/') => {
            if app.current_view() != &View::Search {
                app.search_query.clear();
                app.search_results.clear();
                app.push_view(View::Search);
            }
            app.search_input_mode = true;
            Action::None
        }
        KeyCode::Char('b') => {
            if app.current_view() == &View::Thread {
                if let Some(thread) = &app.current_thread {
                    Action::ToggleBookmark { thread_id: thread.id }
                } else {
                    Action::None
                }
            } else {
                Action::FetchBookmarks
            }
        }
        KeyCode::Char('e') => Action::None,
        KeyCode::Char('m') => Action::FetchInbox,
        KeyCode::Char('r') => {
            match app.current_view().clone() {
                View::Boards => Action::FetchBoards,
                View::Threads => {
                    if let Some(board) = &app.current_board {
                        Action::FetchThreads {
                            slug: board.slug.clone(),
                            page: app.current_page,
                        }
                    } else {
                        Action::None
                    }
                }
                View::Thread => {
                    if let Some(thread) = &app.current_thread {
                        Action::FetchThread {
                            thread_id: thread.id,
                            page: app.current_page,
                        }
                    } else {
                        Action::None
                    }
                }
                View::Invites => Action::FetchInvites,
                View::Members => Action::FetchMembers,
                View::Messages => Action::FetchInbox,
                _ => Action::None,
            }
        }
        KeyCode::Enter => {
            match app.current_view().clone() {
                View::Boards => {
                    if app.boards.get(app.selected_index).is_some() {
                        Action::EnterBoard { index: app.selected_index }
                    } else {
                        Action::None
                    }
                }
                View::Threads => {
                    if let Some(thread) = app.threads.get(app.selected_index) {
                        Action::EnterThread { thread_id: thread.id }
                    } else {
                        Action::None
                    }
                }
                View::Search => {
                    if let Some(result) = app.search_results.get(app.selected_index) {
                        Action::EnterSearchResult { thread_id: result.thread_id }
                    } else {
                        Action::None
                    }
                }
                View::Bookmarks => {
                    if let Some(bm) = app.bookmarks.get(app.selected_index) {
                        Action::EnterBookmark { thread_id: bm.thread_id }
                    } else {
                        Action::None
                    }
                }
                View::Mentions => {
                    if let Some(mention) = app.mentions.get(app.selected_index) {
                        Action::EnterMention { thread_id: mention.thread_id }
                    } else {
                        Action::None
                    }
                }
                View::Messages => {
                    if let Some(conv) = app.dm_conversations.get(app.selected_index) {
                        Action::EnterConversation { username: conv.username.clone() }
                    } else {
                        Action::None
                    }
                }
                _ => Action::None,
            }
        }
        KeyCode::Char('n') => {
            match app.current_view().clone() {
                View::Threads => {
                    if let Some(board) = &app.current_board {
                        Action::OpenEditor {
                            kind: EditorKind::NewThread { board_slug: board.slug.clone() },
                        }
                    } else {
                        Action::None
                    }
                }
                View::Thread => {
                    if let Some(thread) = &app.current_thread {
                        if thread.locked {
                            app.status_message = Some("Thread is locked".to_string());
                            Action::None
                        } else {
                            Action::OpenEditor {
                                kind: EditorKind::Reply {
                                    thread_id: thread.id,
                                    thread_title: thread.title.clone(),
                                    board_slug: thread.board_slug.clone(),
                                },
                            }
                        }
                    } else {
                        Action::None
                    }
                }
                View::Messages => Action::OpenEditor { kind: EditorKind::NewDm },
                View::MessageThread => {
                    if let Some(partner) = &app.dm_partner {
                        Action::OpenEditor {
                            kind: EditorKind::DmToUser { recipient: partner.clone() },
                        }
                    } else {
                        Action::None
                    }
                }
                _ => Action::None,
            }
        }
        KeyCode::Char('d') => {
            if app.current_view() == &View::Members {
                if let Some(user) = app.members.get(app.selected_index) {
                    Action::OpenEditor {
                        kind: EditorKind::DmToUser { recipient: user.username.clone() },
                    }
                } else {
                    Action::None
                }
            } else {
                Action::None
            }
        }
        KeyCode::Char('y') => {
            if app.current_view() == &View::Invites {
                if let Some(inv) = app.invites.get(app.selected_index) {
                    Action::CopyToClipboard { text: inv.code.clone() }
                } else {
                    Action::None
                }
            } else {
                Action::None
            }
        }
        KeyCode::Tab => {
            if app.current_view() == &View::Thread && !app.posts.is_empty() {
                app.post_cursor = Some(0);
            }
            Action::None
        }
        KeyCode::Char(']') => {
            match app.current_view().clone() {
                View::Threads => {
                    let slug = app.current_board.as_ref().map(|b| b.slug.clone());
                    if let Some(slug) = slug {
                        if app.next_page() {
                            Action::NextPage {
                                context: PageContext::Threads { slug },
                            }
                        } else {
                            Action::None
                        }
                    } else {
                        Action::None
                    }
                }
                View::Thread => {
                    let tid = app.current_thread.as_ref().map(|t| t.id);
                    if let Some(thread_id) = tid {
                        if app.next_page() {
                            Action::NextPage {
                                context: PageContext::Thread { thread_id },
                            }
                        } else {
                            Action::None
                        }
                    } else {
                        Action::None
                    }
                }
                _ => Action::None,
            }
        }
        KeyCode::Char('[') => {
            match app.current_view().clone() {
                View::Threads => {
                    let slug = app.current_board.as_ref().map(|b| b.slug.clone());
                    if let Some(slug) = slug {
                        if app.prev_page() {
                            Action::PrevPage {
                                context: PageContext::Threads { slug },
                            }
                        } else {
                            Action::None
                        }
                    } else {
                        Action::None
                    }
                }
                View::Thread => {
                    let tid = app.current_thread.as_ref().map(|t| t.id);
                    if let Some(thread_id) = tid {
                        if app.prev_page() {
                            Action::PrevPage {
                                context: PageContext::Thread { thread_id },
                            }
                        } else {
                            Action::None
                        }
                    } else {
                        Action::None
                    }
                }
                _ => Action::None,
            }
        }
        KeyCode::Char('S') => {
            if app.current_view() == &View::Boards {
                app.servers = crate::config::list_servers();
                app.selected_index = 0;
                app.push_view(View::ServerPicker);
            }
            Action::None
        }
        KeyCode::Char('@') => Action::FetchMentions,
        _ => Action::None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cache::Cache;
    use agora_common::*;
    use std::sync::{Arc, Mutex};

    fn test_app() -> App {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        let cache: Cache = Arc::new(Mutex::new(conn));
        App::new("test".into(), "Test Server".into(), "testuser".into(), cache, None)
    }

    fn test_post(id: i64, author: &str, body: &str) -> Post {
        Post {
            id,
            post_number: id,
            author: author.into(),
            body: body.into(),
            created_at: "now".into(),
            edited_at: None,
            is_deleted: false,
            attachments: Vec::new(),
            parent_post_id: None,
            parent_post_number: None,
            parent_author: None,
            reactions: Vec::new(),
        }
    }

    fn test_board(id: i64, slug: &str, name: &str) -> Board {
        Board {
            id,
            slug: slug.into(),
            name: name.into(),
            description: String::new(),
            thread_count: 0,
            last_post_at: None,
        }
    }

    fn test_thread_summary(id: i64, title: &str) -> ThreadSummary {
        ThreadSummary {
            id,
            title: title.into(),
            author: "user".into(),
            created_at: "now".into(),
            last_post_at: "now".into(),
            post_count: 0,
            pinned: false,
            locked: false,
            latest_post_id: 0,
        }
    }

    fn test_thread_detail(id: i64, title: &str, locked: bool) -> ThreadDetail {
        ThreadDetail {
            id,
            board_id: 1,
            board_slug: "test".into(),
            title: title.into(),
            author: "user".into(),
            created_at: "now".into(),
            pinned: false,
            locked,
        }
    }

    fn test_board_info(slug: &str) -> BoardInfo {
        BoardInfo {
            id: 1,
            slug: slug.into(),
            name: "Test".into(),
            description: String::new(),
        }
    }

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    fn ctrl_key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::CONTROL)
    }

    // ── Navigation (8) ──────────────────────────────────────────

    #[test]
    fn j_moves_down_in_list_view() {
        let mut app = test_app();
        app.boards = vec![test_board(1, "a", "A"), test_board(2, "b", "B")];
        assert_eq!(app.selected_index, 0);
        let action = handle_key(&mut app, key(KeyCode::Char('j')));
        assert_eq!(action, Action::None);
        assert_eq!(app.selected_index, 1);
    }

    #[test]
    fn k_moves_up_in_list_view() {
        let mut app = test_app();
        app.boards = vec![test_board(1, "a", "A"), test_board(2, "b", "B")];
        app.selected_index = 1;
        let action = handle_key(&mut app, key(KeyCode::Char('k')));
        assert_eq!(action, Action::None);
        assert_eq!(app.selected_index, 0);
    }

    #[test]
    fn k_clamps_at_top() {
        let mut app = test_app();
        app.boards = vec![test_board(1, "a", "A"), test_board(2, "b", "B")];
        app.selected_index = 0;
        handle_key(&mut app, key(KeyCode::Char('k')));
        assert_eq!(app.selected_index, 0);
    }

    #[test]
    fn j_clamps_at_bottom() {
        let mut app = test_app();
        app.boards = vec![test_board(1, "a", "A")];
        handle_key(&mut app, key(KeyCode::Char('j')));
        assert_eq!(app.selected_index, 0); // only 1 item, can't go past
    }

    #[test]
    fn g_goes_to_top() {
        let mut app = test_app();
        app.boards = vec![test_board(1, "a", "A"), test_board(2, "b", "B")];
        app.selected_index = 1;
        let action = handle_key(&mut app, key(KeyCode::Char('g')));
        assert_eq!(action, Action::None);
        assert_eq!(app.selected_index, 0);
    }

    #[test]
    fn g_capital_goes_to_bottom() {
        let mut app = test_app();
        app.boards = vec![
            test_board(1, "a", "A"),
            test_board(2, "b", "B"),
            test_board(3, "c", "C"),
        ];
        let action = handle_key(&mut app, key(KeyCode::Char('G')));
        assert_eq!(action, Action::None);
        assert_eq!(app.selected_index, 2);
    }

    #[test]
    fn page_up_in_thread_view() {
        let mut app = test_app();
        app.push_view(View::Thread);
        app.scroll_offset = 30;
        handle_key(&mut app, key(KeyCode::PageUp));
        assert_eq!(app.scroll_offset, 10); // 30 - PAGE_SCROLL_SIZE(20)
    }

    #[test]
    fn page_down_in_thread_view() {
        let mut app = test_app();
        app.push_view(View::Thread);
        app.scroll_offset = 0;
        handle_key(&mut app, key(KeyCode::PageDown));
        assert_eq!(app.scroll_offset, 20);
    }

    // ── View stack (4) ──────────────────────────────────────────

    #[test]
    fn q_pops_view() {
        let mut app = test_app();
        app.push_view(View::Threads);
        let action = handle_key(&mut app, key(KeyCode::Char('q')));
        assert_eq!(action, Action::None);
        assert_eq!(app.current_view(), &View::Boards);
    }

    #[test]
    fn q_at_root_returns_quit() {
        let mut app = test_app();
        let action = handle_key(&mut app, key(KeyCode::Char('q')));
        assert_eq!(action, Action::Quit);
    }

    #[test]
    fn esc_pops_view() {
        let mut app = test_app();
        app.push_view(View::Invites);
        let action = handle_key(&mut app, key(KeyCode::Esc));
        assert_eq!(action, Action::None);
        assert_eq!(app.current_view(), &View::Boards);
    }

    #[test]
    fn ctrl_c_always_quits() {
        let mut app = test_app();
        app.push_view(View::Threads);
        let action = handle_key(&mut app, ctrl_key(KeyCode::Char('c')));
        assert_eq!(action, Action::Quit);
    }

    // ── Help overlay (3) ────────────────────────────────────────

    #[test]
    fn question_mark_toggles_help() {
        let mut app = test_app();
        assert!(!app.show_help);
        handle_key(&mut app, key(KeyCode::Char('?')));
        assert!(app.show_help);
    }

    #[test]
    fn help_overlay_blocks_other_keys() {
        let mut app = test_app();
        app.boards = vec![test_board(1, "a", "A"), test_board(2, "b", "B")];
        app.show_help = true;
        let action = handle_key(&mut app, key(KeyCode::Char('j')));
        assert_eq!(action, Action::None);
        assert!(app.show_help); // still showing
        assert_eq!(app.selected_index, 0); // j didn't move selection
    }

    #[test]
    fn help_closes_on_esc() {
        let mut app = test_app();
        app.show_help = true;
        handle_key(&mut app, key(KeyCode::Esc));
        assert!(!app.show_help);
    }

    // ── Search input (5) ────────────────────────────────────────

    #[test]
    fn search_chars_accumulate() {
        let mut app = test_app();
        app.search_input_mode = true;
        handle_key(&mut app, key(KeyCode::Char('h')));
        handle_key(&mut app, key(KeyCode::Char('i')));
        assert_eq!(app.search_query, "hi");
    }

    #[test]
    fn search_backspace_removes_char() {
        let mut app = test_app();
        app.search_input_mode = true;
        app.search_query = "test".to_string();
        handle_key(&mut app, key(KeyCode::Backspace));
        assert_eq!(app.search_query, "tes");
    }

    #[test]
    fn search_enter_returns_search_action() {
        let mut app = test_app();
        app.search_input_mode = true;
        app.search_query = "hello".to_string();
        let action = handle_key(&mut app, key(KeyCode::Enter));
        assert_eq!(
            action,
            Action::SearchQuery { query: "hello".to_string(), by: None }
        );
        assert!(!app.search_input_mode);
    }

    #[test]
    fn search_esc_exits_input_mode() {
        let mut app = test_app();
        app.search_input_mode = true;
        app.search_query = "partial".to_string();
        let action = handle_key(&mut app, key(KeyCode::Esc));
        assert_eq!(action, Action::None);
        assert!(!app.search_input_mode);
    }

    #[test]
    fn search_empty_enter_returns_none() {
        let mut app = test_app();
        app.search_input_mode = true;
        app.search_query = String::new();
        let action = handle_key(&mut app, key(KeyCode::Enter));
        assert_eq!(action, Action::None);
    }

    // ── Reaction picker (3) ─────────────────────────────────────

    fn test_emojis() -> Vec<crate::config::EmojiEntry> {
        vec![
            crate::config::EmojiEntry { label: "thumbsup".into(), emoji: "\u{1F44D}".into() },
            crate::config::EmojiEntry { label: "heart".into(), emoji: "\u{2764}\u{FE0F}".into() },
            crate::config::EmojiEntry { label: "skull".into(), emoji: "\u{1F480}".into() },
        ]
    }

    #[test]
    fn reaction_picker_selects_via_digit_key() {
        let mut app = test_app();
        app.emojis = test_emojis();
        app.push_view(View::Thread);
        app.current_thread = Some(test_thread_detail(1, "Test", false));
        app.posts = vec![test_post(10, "other", "hello")];
        app.post_cursor = Some(0);
        app.recent_reactions = vec!["\u{2764}\u{FE0F}".into(), "\u{1F480}".into()];
        app.show_reaction_picker = true;

        let action = handle_key(&mut app, key(KeyCode::Char('0')));
        assert_eq!(
            action,
            Action::ReactToPost { thread_id: 1, post_id: 10, reaction: "\u{2764}\u{FE0F}".into() }
        );
        assert!(!app.show_reaction_picker);
    }

    #[test]
    fn reaction_picker_selects_via_enter() {
        let mut app = test_app();
        app.emojis = test_emojis();
        app.push_view(View::Thread);
        app.current_thread = Some(test_thread_detail(1, "Test", false));
        app.posts = vec![test_post(10, "other", "hello")];
        app.post_cursor = Some(0);
        app.show_reaction_picker = true;
        // No recents, so scroll=0 is the first emoji entry (thumbsup)
        app.reaction_picker_scroll = 0;

        let action = handle_key(&mut app, key(KeyCode::Enter));
        assert_eq!(
            action,
            Action::ReactToPost { thread_id: 1, post_id: 10, reaction: "\u{1F44D}".into() }
        );
        assert!(!app.show_reaction_picker);
    }

    #[test]
    fn reaction_picker_esc_cancels() {
        let mut app = test_app();
        app.show_reaction_picker = true;
        let action = handle_key(&mut app, key(KeyCode::Esc));
        assert_eq!(action, Action::None);
        assert!(!app.show_reaction_picker);
    }

    #[test]
    fn reaction_picker_invalid_key_returns_none() {
        let mut app = test_app();
        app.show_reaction_picker = true;
        let action = handle_key(&mut app, key(KeyCode::Char('z')));
        assert_eq!(action, Action::None);
        assert!(app.show_reaction_picker); // still showing (no valid pick)
    }

    // ── Post-selection mode (5) ─────────────────────────────────

    #[test]
    fn post_cursor_j_moves_down() {
        let mut app = test_app();
        app.push_view(View::Thread);
        app.posts = vec![test_post(1, "a", ""), test_post(2, "b", "")];
        app.post_cursor = Some(0);
        handle_key(&mut app, key(KeyCode::Char('j')));
        assert_eq!(app.post_cursor, Some(1));
    }

    #[test]
    fn post_cursor_k_moves_up() {
        let mut app = test_app();
        app.push_view(View::Thread);
        app.posts = vec![test_post(1, "a", ""), test_post(2, "b", "")];
        app.post_cursor = Some(1);
        handle_key(&mut app, key(KeyCode::Char('k')));
        assert_eq!(app.post_cursor, Some(0));
    }

    #[test]
    fn post_cursor_esc_exits() {
        let mut app = test_app();
        app.push_view(View::Thread);
        app.post_cursor = Some(0);
        handle_key(&mut app, key(KeyCode::Esc));
        assert_eq!(app.post_cursor, None);
    }

    #[test]
    fn post_cursor_plus_opens_picker() {
        let mut app = test_app();
        app.push_view(View::Thread);
        app.posts = vec![test_post(1, "a", "")];
        app.post_cursor = Some(0);
        handle_key(&mut app, key(KeyCode::Char('+')));
        assert!(app.show_reaction_picker);
    }

    #[test]
    fn post_cursor_r_on_locked_thread_shows_status() {
        let mut app = test_app();
        app.push_view(View::Thread);
        app.current_thread = Some(test_thread_detail(1, "T", true));
        app.posts = vec![test_post(1, "a", "hello")];
        app.post_cursor = Some(0);
        let action = handle_key(&mut app, key(KeyCode::Char('R')));
        assert_eq!(action, Action::None);
        assert_eq!(app.status_message, Some("Thread is locked".to_string()));
    }

    // ── Enter key (5) ───────────────────────────────────────────

    #[test]
    fn enter_on_boards_returns_enter_board() {
        let mut app = test_app();
        app.boards = vec![test_board(1, "gen", "General")];
        let action = handle_key(&mut app, key(KeyCode::Enter));
        assert_eq!(action, Action::EnterBoard { index: 0 });
    }

    #[test]
    fn enter_on_threads_returns_enter_thread() {
        let mut app = test_app();
        app.push_view(View::Threads);
        app.threads = vec![test_thread_summary(42, "test")];
        let action = handle_key(&mut app, key(KeyCode::Enter));
        assert_eq!(action, Action::EnterThread { thread_id: 42 });
    }

    #[test]
    fn enter_on_search_returns_enter_search_result() {
        let mut app = test_app();
        app.push_view(View::Search);
        app.search_results = vec![SearchResult {
            kind: "post".into(),
            thread_id: 7,
            post_id: 1,
            snippet: "...found...".into(),
            thread_title: Some("found".into()),
            author: Some("u".into()),
        }];
        let action = handle_key(&mut app, key(KeyCode::Enter));
        assert_eq!(action, Action::EnterSearchResult { thread_id: 7 });
    }

    #[test]
    fn enter_on_bookmarks_returns_enter_bookmark() {
        let mut app = test_app();
        app.push_view(View::Bookmarks);
        app.bookmarks = vec![BookmarkInfo {
            thread_id: 99,
            thread_title: "saved".into(),
            board_slug: "gen".into(),
            created_at: "now".into(),
        }];
        let action = handle_key(&mut app, key(KeyCode::Enter));
        assert_eq!(action, Action::EnterBookmark { thread_id: 99 });
    }

    #[test]
    fn enter_on_messages_returns_enter_conversation() {
        let mut app = test_app();
        app.push_view(View::Messages);
        app.dm_conversations = vec![DmConversationSummary {
            username: "alice".into(),
            public_key: String::new(),
            last_message_at: "now".into(),
            message_count: 0,
        }];
        let action = handle_key(&mut app, key(KeyCode::Enter));
        assert_eq!(action, Action::EnterConversation { username: "alice".into() });
    }

    // ── Action keys (8) ─────────────────────────────────────────

    #[test]
    fn i_returns_fetch_invites() {
        let mut app = test_app();
        assert_eq!(handle_key(&mut app, key(KeyCode::Char('i'))), Action::FetchInvites);
    }

    #[test]
    fn w_returns_fetch_members() {
        let mut app = test_app();
        assert_eq!(handle_key(&mut app, key(KeyCode::Char('w'))), Action::FetchMembers);
    }

    #[test]
    fn m_returns_fetch_inbox() {
        let mut app = test_app();
        assert_eq!(handle_key(&mut app, key(KeyCode::Char('m'))), Action::FetchInbox);
    }

    #[test]
    fn at_returns_fetch_mentions() {
        let mut app = test_app();
        assert_eq!(handle_key(&mut app, key(KeyCode::Char('@'))), Action::FetchMentions);
    }

    #[test]
    fn r_on_boards_returns_fetch_boards() {
        let mut app = test_app();
        assert_eq!(handle_key(&mut app, key(KeyCode::Char('r'))), Action::FetchBoards);
    }

    #[test]
    fn b_in_thread_returns_toggle_bookmark() {
        let mut app = test_app();
        app.push_view(View::Thread);
        app.current_thread = Some(test_thread_detail(5, "T", false));
        let action = handle_key(&mut app, key(KeyCode::Char('b')));
        assert_eq!(action, Action::ToggleBookmark { thread_id: 5 });
    }

    #[test]
    fn n_in_threads_returns_new_thread_editor() {
        let mut app = test_app();
        app.push_view(View::Threads);
        app.current_board = Some(test_board_info("gen"));
        let action = handle_key(&mut app, key(KeyCode::Char('n')));
        assert_eq!(
            action,
            Action::OpenEditor { kind: EditorKind::NewThread { board_slug: "gen".into() } }
        );
    }

    #[test]
    fn e_in_post_mode_returns_edit_post_editor() {
        let mut app = test_app();
        app.push_view(View::Thread);
        app.current_thread = Some(test_thread_detail(1, "Title", false));
        app.posts = vec![test_post(10, "testuser", "my post")];
        app.post_cursor = Some(0);
        let action = handle_key(&mut app, key(KeyCode::Char('e')));
        assert_eq!(
            action,
            Action::OpenEditor {
                kind: EditorKind::EditPost {
                    thread_id: 1,
                    post_id: 10,
                    old_body: "my post".into(),
                    thread_title: "Title".into(),
                }
            }
        );
    }

    #[test]
    fn e_outside_post_mode_is_noop() {
        let mut app = test_app();
        app.push_view(View::Thread);
        app.current_thread = Some(test_thread_detail(1, "Title", false));
        app.posts = vec![test_post(10, "testuser", "my post")];
        // No post_cursor — not in post mode
        let action = handle_key(&mut app, key(KeyCode::Char('e')));
        assert_eq!(action, Action::None);
    }

    // ── Pagination (4) ──────────────────────────────────────────

    #[test]
    fn bracket_right_in_threads_returns_next_page() {
        let mut app = test_app();
        app.push_view(View::Threads);
        app.current_board = Some(test_board_info("gen"));
        app.current_page = 1;
        app.total_pages = 3;
        let action = handle_key(&mut app, key(KeyCode::Char(']')));
        assert_eq!(
            action,
            Action::NextPage { context: PageContext::Threads { slug: "gen".into() } }
        );
        assert_eq!(app.current_page, 2);
    }

    #[test]
    fn bracket_left_in_threads_returns_prev_page() {
        let mut app = test_app();
        app.push_view(View::Threads);
        app.current_board = Some(test_board_info("gen"));
        app.current_page = 2;
        app.total_pages = 3;
        let action = handle_key(&mut app, key(KeyCode::Char('[')));
        assert_eq!(
            action,
            Action::PrevPage { context: PageContext::Threads { slug: "gen".into() } }
        );
        assert_eq!(app.current_page, 1);
    }

    #[test]
    fn bracket_right_at_last_page_is_none() {
        let mut app = test_app();
        app.push_view(View::Threads);
        app.current_board = Some(test_board_info("gen"));
        app.current_page = 3;
        app.total_pages = 3;
        let action = handle_key(&mut app, key(KeyCode::Char(']')));
        assert_eq!(action, Action::None);
    }

    #[test]
    fn bracket_left_at_first_page_is_none() {
        let mut app = test_app();
        app.push_view(View::Threads);
        app.current_board = Some(test_board_info("gen"));
        app.current_page = 1;
        app.total_pages = 3;
        let action = handle_key(&mut app, key(KeyCode::Char('[')));
        assert_eq!(action, Action::None);
    }

    // ── Status message (1) ──────────────────────────────────────

    #[test]
    fn status_message_cleared_on_key() {
        let mut app = test_app();
        app.status_message = Some("old message".to_string());
        app.boards = vec![test_board(1, "a", "A")];
        handle_key(&mut app, key(KeyCode::Char('j')));
        assert_eq!(app.status_message, None);
    }

    // ── parse_search_query (4) ──────────────────────────────────

    #[test]
    fn parse_plain_query() {
        let (q, by) = parse_search_query("hello world");
        assert_eq!(q, "hello world");
        assert_eq!(by, None);
    }

    #[test]
    fn parse_by_prefix() {
        let (q, by) = parse_search_query("by:alice some query");
        assert_eq!(q, "some query");
        assert_eq!(by, Some("alice".into()));
    }

    #[test]
    fn parse_by_only_username() {
        let (q, by) = parse_search_query("by:bob");
        assert_eq!(q, "");
        assert_eq!(by, Some("bob".into()));
    }

    #[test]
    fn parse_by_empty_username() {
        let (q, by) = parse_search_query("by:");
        assert_eq!(q, "by:");
        assert_eq!(by, None);
    }

    // ── Negative / edge case tests ──────────────────────────────

    #[test]
    fn enter_on_empty_boards_returns_none() {
        let mut app = test_app();
        // boards is empty
        let action = handle_key(&mut app, key(KeyCode::Enter));
        assert_eq!(action, Action::None);
    }

    #[test]
    fn enter_on_empty_threads_returns_none() {
        let mut app = test_app();
        app.push_view(View::Threads);
        // threads is empty
        let action = handle_key(&mut app, key(KeyCode::Enter));
        assert_eq!(action, Action::None);
    }

    #[test]
    fn e_outside_thread_view_is_noop() {
        let mut app = test_app();
        // On Boards view
        let action = handle_key(&mut app, key(KeyCode::Char('e')));
        assert_eq!(action, Action::None);
    }

    #[test]
    fn e_in_post_mode_on_others_post_shows_status() {
        let mut app = test_app();
        app.push_view(View::Thread);
        app.current_thread = Some(test_thread_detail(1, "T", false));
        app.posts = vec![test_post(1, "someone_else", "their post")];
        app.post_cursor = Some(0);
        let action = handle_key(&mut app, key(KeyCode::Char('e')));
        assert_eq!(action, Action::None);
        assert!(app.status_message.unwrap().contains("only edit your own"));
    }

    #[test]
    fn d_in_post_mode_returns_delete_post() {
        let mut app = test_app();
        app.push_view(View::Thread);
        app.current_thread = Some(test_thread_detail(1, "T", false));
        app.posts = vec![test_post(10, "testuser", "my post")];
        app.post_cursor = Some(0);
        let action = handle_key(&mut app, key(KeyCode::Char('d')));
        assert_eq!(action, Action::DeletePost { thread_id: 1, post_id: 10 });
    }

    #[test]
    fn d_in_post_mode_on_others_post_shows_status() {
        let mut app = test_app();
        app.push_view(View::Thread);
        app.current_thread = Some(test_thread_detail(1, "T", false));
        app.posts = vec![test_post(1, "someone_else", "their post")];
        app.post_cursor = Some(0);
        let action = handle_key(&mut app, key(KeyCode::Char('d')));
        assert_eq!(action, Action::None);
        assert!(app.status_message.unwrap().contains("only delete your own"));
    }

    #[test]
    fn n_on_locked_thread_shows_status() {
        let mut app = test_app();
        app.push_view(View::Thread);
        app.current_thread = Some(test_thread_detail(1, "Locked", true));
        let action = handle_key(&mut app, key(KeyCode::Char('n')));
        assert_eq!(action, Action::None);
        assert_eq!(app.status_message, Some("Thread is locked".to_string()));
    }

    #[test]
    fn n_on_unlocked_thread_returns_reply_editor() {
        let mut app = test_app();
        app.push_view(View::Thread);
        app.current_thread = Some(test_thread_detail(1, "Open", false));
        let action = handle_key(&mut app, key(KeyCode::Char('n')));
        assert_eq!(
            action,
            Action::OpenEditor {
                kind: EditorKind::Reply {
                    thread_id: 1,
                    thread_title: "Open".into(),
                    board_slug: "test".into(),
                }
            }
        );
    }

    #[test]
    fn post_cursor_r_on_unlocked_returns_reply_to_editor() {
        let mut app = test_app();
        app.push_view(View::Thread);
        app.current_thread = Some(test_thread_detail(1, "Open", false));
        app.posts = vec![test_post(5, "someone", "parent body")];
        app.post_cursor = Some(0);
        let action = handle_key(&mut app, key(KeyCode::Char('R')));
        assert_eq!(
            action,
            Action::OpenEditor {
                kind: EditorKind::ReplyTo {
                    thread_id: 1,
                    thread_title: "Open".into(),
                    board_slug: "test".into(),
                    parent_post_id: 5,
                    parent_author: "someone".into(),
                    parent_body: "parent body".into(),
                }
            }
        );
    }

    #[test]
    fn b_outside_thread_returns_fetch_bookmarks() {
        let mut app = test_app();
        // On Boards view (not Thread)
        let action = handle_key(&mut app, key(KeyCode::Char('b')));
        assert_eq!(action, Action::FetchBookmarks);
    }

    #[test]
    fn r_on_threads_view_returns_fetch_threads() {
        let mut app = test_app();
        app.push_view(View::Threads);
        app.current_board = Some(test_board_info("gen"));
        app.current_page = 2;
        let action = handle_key(&mut app, key(KeyCode::Char('r')));
        assert_eq!(
            action,
            Action::FetchThreads { slug: "gen".into(), page: 2 }
        );
    }

    #[test]
    fn r_on_thread_view_returns_fetch_thread() {
        let mut app = test_app();
        app.push_view(View::Thread);
        app.current_thread = Some(test_thread_detail(42, "T", false));
        app.current_page = 1;
        let action = handle_key(&mut app, key(KeyCode::Char('r')));
        assert_eq!(
            action,
            Action::FetchThread { thread_id: 42, page: 1 }
        );
    }

    #[test]
    fn slash_opens_search_view_and_input_mode() {
        let mut app = test_app();
        assert_eq!(app.current_view(), &View::Boards);
        let action = handle_key(&mut app, key(KeyCode::Char('/')));
        assert_eq!(action, Action::None);
        assert_eq!(app.current_view(), &View::Search);
        assert!(app.search_input_mode);
        assert!(app.search_query.is_empty());
    }

    #[test]
    fn d_on_members_returns_dm_editor() {
        let mut app = test_app();
        app.push_view(View::Members);
        app.members = vec![UserInfo {
            username: "bob".into(),
            role: "member".into(),
            post_count: 0,
            joined_at: "now".into(),
            last_seen_at: None,
            invited_by: None,
            is_online: false,
            bio: String::new(),
        }];
        let action = handle_key(&mut app, key(KeyCode::Char('d')));
        assert_eq!(
            action,
            Action::OpenEditor { kind: EditorKind::DmToUser { recipient: "bob".into() } }
        );
    }

    #[test]
    fn d_outside_members_is_noop() {
        let mut app = test_app();
        let action = handle_key(&mut app, key(KeyCode::Char('d')));
        assert_eq!(action, Action::None);
    }

    #[test]
    fn y_on_invites_returns_copy_to_clipboard() {
        let mut app = test_app();
        app.push_view(View::Invites);
        app.invites = vec![InviteInfo {
            code: "ABC123".into(),
            created_at: "now".into(),
            used_by: None,
        }];
        let action = handle_key(&mut app, key(KeyCode::Char('y')));
        assert_eq!(action, Action::CopyToClipboard { text: "ABC123".into() });
    }

    #[test]
    fn y_outside_invites_is_noop() {
        let mut app = test_app();
        let action = handle_key(&mut app, key(KeyCode::Char('y')));
        assert_eq!(action, Action::None);
    }

    #[test]
    fn tab_in_thread_with_posts_enables_post_cursor() {
        let mut app = test_app();
        app.push_view(View::Thread);
        app.posts = vec![test_post(1, "a", "hello")];
        assert_eq!(app.post_cursor, None);
        let action = handle_key(&mut app, key(KeyCode::Tab));
        assert_eq!(action, Action::None);
        assert_eq!(app.post_cursor, Some(0));
    }

    #[test]
    fn tab_in_thread_without_posts_does_not_enable_cursor() {
        let mut app = test_app();
        app.push_view(View::Thread);
        // posts is empty
        let action = handle_key(&mut app, key(KeyCode::Tab));
        assert_eq!(action, Action::None);
        assert_eq!(app.post_cursor, None);
    }

    #[test]
    fn g_on_invites_returns_generate_invite() {
        let mut app = test_app();
        app.push_view(View::Invites);
        let action = handle_key(&mut app, key(KeyCode::Char('g')));
        assert_eq!(action, Action::GenerateInvite);
    }

    #[test]
    fn n_on_messages_returns_new_dm_editor() {
        let mut app = test_app();
        app.push_view(View::Messages);
        let action = handle_key(&mut app, key(KeyCode::Char('n')));
        assert_eq!(action, Action::OpenEditor { kind: EditorKind::NewDm });
    }

    #[test]
    fn n_on_message_thread_returns_dm_to_user() {
        let mut app = test_app();
        app.push_view(View::MessageThread);
        app.dm_partner = Some("alice".into());
        let action = handle_key(&mut app, key(KeyCode::Char('n')));
        assert_eq!(
            action,
            Action::OpenEditor { kind: EditorKind::DmToUser { recipient: "alice".into() } }
        );
    }

    #[test]
    fn search_by_prefix_parsed_correctly() {
        let mut app = test_app();
        app.search_input_mode = true;
        app.search_query = "by:alice hello".to_string();
        let action = handle_key(&mut app, key(KeyCode::Enter));
        assert_eq!(
            action,
            Action::SearchQuery { query: "hello".into(), by: Some("alice".into()) }
        );
    }

    #[test]
    fn post_cursor_j_clamps_at_bottom() {
        let mut app = test_app();
        app.push_view(View::Thread);
        app.posts = vec![test_post(1, "a", "")];
        app.post_cursor = Some(0);
        handle_key(&mut app, key(KeyCode::Char('j')));
        assert_eq!(app.post_cursor, Some(0)); // only 1 post, can't go further
    }

    #[test]
    fn post_cursor_k_clamps_at_top() {
        let mut app = test_app();
        app.push_view(View::Thread);
        app.posts = vec![test_post(1, "a", ""), test_post(2, "b", "")];
        app.post_cursor = Some(0);
        handle_key(&mut app, key(KeyCode::Char('k')));
        assert_eq!(app.post_cursor, Some(0)); // already at 0
    }

    #[test]
    fn enter_on_mentions_returns_enter_mention() {
        let mut app = test_app();
        app.push_view(View::Mentions);
        app.mentions = vec![MentionResult {
            post_id: 1,
            thread_id: 50,
            thread_title: "mentioned".into(),
            author: "u".into(),
            snippet: "hey @testuser".into(),
            created_at: "now".into(),
        }];
        let action = handle_key(&mut app, key(KeyCode::Enter));
        assert_eq!(action, Action::EnterMention { thread_id: 50 });
    }

    #[test]
    fn arrow_keys_work_same_as_jk() {
        let mut app = test_app();
        app.boards = vec![test_board(1, "a", "A"), test_board(2, "b", "B")];
        handle_key(&mut app, key(KeyCode::Down));
        assert_eq!(app.selected_index, 1);
        handle_key(&mut app, key(KeyCode::Up));
        assert_eq!(app.selected_index, 0);
    }
}
