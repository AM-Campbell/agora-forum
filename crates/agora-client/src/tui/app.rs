use crossterm::{
    event::{self, Event},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::CrosstermBackend,
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph, Wrap},
    Frame, Terminal,
};
use std::io;

use crate::api::ApiClient;
use crate::cache::{self, Cache};
use crate::config;
use crate::editor;
use crate::tui::input::{self, Action, EditorKind, PageContext};
use crate::tui::status::ConnectionState;
use agora_common::*;

pub enum TuiResult {
    Quit,
    SwitchServer { server_addr: String },
}

const MIN_TERMINAL_WIDTH: u16 = 60;
const MIN_TERMINAL_HEIGHT: u16 = 15;
const PAGE_SCROLL_SIZE: usize = 20;

macro_rules! suspend_tui {
    () => {
        disable_raw_mode().ok();
        execute!(io::stdout(), LeaveAlternateScreen).ok();
    };
}

macro_rules! resume_tui {
    ($terminal:expr) => {
        enable_raw_mode().ok();
        execute!(io::stdout(), EnterAlternateScreen).ok();
        $terminal.clear().ok();
    };
}

#[derive(Debug, Clone, PartialEq)]
pub enum View {
    Boards,
    Threads,
    Thread,
    Invites,
    Members,
    Search,
    Messages,
    MessageThread,
    Bookmarks,
    Mentions,
    ServerPicker,
}

pub struct App {
    pub view_stack: Vec<View>,
    #[allow(dead_code)]
    pub server_addr: String,
    pub server_name: String,
    pub username: String,
    pub connection_state: ConnectionState,
    pub cache: Cache,

    // Board list
    pub boards: Vec<Board>,
    pub selected_index: usize,

    // Thread list
    pub current_board: Option<BoardInfo>,
    pub threads: Vec<ThreadSummary>,
    pub current_page: i64,
    pub total_pages: i64,

    // Thread view
    pub current_thread: Option<ThreadDetail>,
    pub posts: Vec<Post>,
    pub scroll_offset: usize,

    // Invites
    pub invites: Vec<InviteInfo>,

    // Members
    pub members: Vec<UserInfo>,

    // Search
    pub search_input_mode: bool,
    pub search_query: String,
    pub search_results: Vec<SearchResult>,

    // DMs
    pub dm_conversations: Vec<DmConversationSummary>,
    pub dm_partner: Option<String>,
    pub dm_decrypted: Vec<(String, String, String)>, // (timestamp, sender, plaintext)

    // Bookmarks
    pub bookmarks: Vec<BookmarkInfo>,

    // Help overlay
    pub show_help: bool,

    // Status message
    pub status_message: Option<String>,

    // Unread divider — snapshot of last-read post ID before entering thread
    pub last_read_post_id: Option<i64>,

    // Post-selection mode (for reply-to, reactions)
    pub post_cursor: Option<usize>,

    // Reactions picker
    pub show_reaction_picker: bool,

    // Mentions
    pub mentions: Vec<MentionResult>,

    // Reply context lines
    pub reply_context: usize,

    // Server picker
    pub servers: Vec<config::ServerConfig>,
}

impl App {
    pub fn new(server_addr: String, server_name: String, username: String, cache: Cache) -> Self {
        Self {
            view_stack: vec![View::Boards],
            server_addr,
            server_name,
            username,
            connection_state: ConnectionState::Connecting,
            cache,
            boards: Vec::new(),
            selected_index: 0,
            current_board: None,
            threads: Vec::new(),
            current_page: 1,
            total_pages: 1,
            current_thread: None,
            posts: Vec::new(),
            scroll_offset: 0,
            invites: Vec::new(),
            members: Vec::new(),
            search_input_mode: false,
            search_query: String::new(),
            search_results: Vec::new(),
            dm_conversations: Vec::new(),
            dm_partner: None,
            dm_decrypted: Vec::new(),
            bookmarks: Vec::new(),
            show_help: false,
            status_message: None,
            last_read_post_id: None,
            post_cursor: None,
            show_reaction_picker: false,
            mentions: Vec::new(),
            reply_context: 3,
            servers: Vec::new(),
        }
    }

    pub fn current_view(&self) -> &View {
        self.view_stack.last().unwrap_or(&View::Boards)
    }

    pub fn push_view(&mut self, view: View) {
        self.selected_index = 0;
        self.scroll_offset = 0;
        self.view_stack.push(view);
    }

    pub fn pop_view(&mut self) -> bool {
        if self.view_stack.len() > 1 {
            self.view_stack.pop();
            self.selected_index = 0;
            self.scroll_offset = 0;
            true
        } else {
            false
        }
    }

    fn list_len(&self) -> usize {
        match self.current_view() {
            View::Boards => self.boards.len(),
            View::Threads => self.threads.len(),
            View::Invites => self.invites.len(),
            View::Members => self.members.len(),
            View::Search => self.search_results.len(),
            View::Messages => self.dm_conversations.len(),
            View::Bookmarks => self.bookmarks.len(),
            View::Mentions => self.mentions.len(),
            View::ServerPicker => self.servers.len(),
            View::Thread | View::MessageThread => 0,
        }
    }

    pub fn clamp_selection(&mut self) {
        let len = self.list_len();
        if len == 0 {
            self.selected_index = 0;
        } else if self.selected_index >= len {
            self.selected_index = len - 1;
        }
    }

    pub fn move_up(&mut self) {
        match self.current_view() {
            View::Thread | View::MessageThread => {
                self.scroll_offset = self.scroll_offset.saturating_sub(1);
            }
            _ => {
                if self.selected_index > 0 {
                    self.selected_index -= 1;
                }
            }
        }
    }

    pub fn move_down(&mut self) {
        match self.current_view() {
            View::Thread | View::MessageThread => {
                self.scroll_offset += 1;
            }
            _ => {
                let len = self.list_len();
                if len > 0 && self.selected_index < len - 1 {
                    self.selected_index += 1;
                }
            }
        }
    }

    pub fn go_top(&mut self) {
        match self.current_view() {
            View::Thread | View::MessageThread => {
                self.scroll_offset = 0;
            }
            _ => {
                self.selected_index = 0;
            }
        }
    }

    pub fn go_bottom(&mut self) {
        match self.current_view() {
            View::Thread | View::MessageThread => {
                self.scroll_offset = usize::MAX;
            }
            _ => {
                let len = self.list_len();
                if len > 0 {
                    self.selected_index = len - 1;
                }
            }
        }
    }

    /// Build a consistent header title: ` Server › Location  ─  @user · status `
    pub fn header_title(&self, location: &str) -> String {
        format!(
            " {} › {}  ─  @{}  ·  {} ",
            self.server_name, location, self.username, self.connection_state.label()
        )
    }

    pub fn page_up(&mut self) {
        match self.current_view() {
            View::Thread | View::MessageThread => {
                self.scroll_offset = self.scroll_offset.saturating_sub(PAGE_SCROLL_SIZE);
            }
            _ => {}
        }
    }

    pub fn page_down(&mut self) {
        match self.current_view() {
            View::Thread | View::MessageThread => {
                self.scroll_offset += PAGE_SCROLL_SIZE;
            }
            _ => {}
        }
    }

    pub fn next_page(&mut self) -> bool {
        if self.current_page < self.total_pages {
            self.current_page += 1;
            true
        } else {
            false
        }
    }

    pub fn prev_page(&mut self) -> bool {
        if self.current_page > 1 {
            self.current_page -= 1;
            true
        } else {
            false
        }
    }
}

pub async fn run_tui(api: ApiClient, server_addr: String, server_name: String, username: String, cache: Cache, reply_context: usize) -> Result<TuiResult, String> {
    enable_raw_mode().map_err(|e| format!("Failed to enable raw mode: {}", e))?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)
        .map_err(|e| format!("Failed to enter alternate screen: {}", e))?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal =
        Terminal::new(backend).map_err(|e| format!("Failed to create terminal: {}", e))?;

    let mut app = App::new(server_addr, server_name, username, cache);
    app.reply_context = reply_context;

    // Load cached boards first
    app.boards = cache::get_cached_boards(&app.cache);

    // Try to fetch from server
    match api.get_boards().await {
        Ok(resp) => {
            cache::cache_boards(&app.cache, &resp.boards);
            app.boards = resp.boards;
            app.connection_state = ConnectionState::Online;
            app.status_message = Some("Press ? for help, S to switch servers".to_string());

            // Refresh server name from server
            if let Ok(v) = api.get_version().await {
                if let Some(name) = v.server_name {
                    app.server_name = name.clone();
                    if let Ok(mut srv_cfg) = config::ServerConfig::load(&app.server_addr) {
                        srv_cfg.server_name = Some(name);
                        let _ = srv_cfg.save();
                    }
                }
            }
        }
        Err(_) => {
            app.connection_state = ConnectionState::Offline;
            app.status_message = Some("Offline — showing cached data. Press r to retry.".to_string());
        }
    }
    app.clamp_selection();

    loop {
        // Check terminal size
        let size = terminal.size().unwrap_or_default();
        if size.width < MIN_TERMINAL_WIDTH || size.height < MIN_TERMINAL_HEIGHT {
            terminal
                .draw(|f| {
                    let msg = Paragraph::new(format!("Terminal too small. Need at least {}x{}.", MIN_TERMINAL_WIDTH, MIN_TERMINAL_HEIGHT));
                    f.render_widget(msg, f.area());
                })
                .ok();

            if let Ok(Event::Key(_)) = event::read() {
                // Wait for resize or keypress
            }
            continue;
        }

        terminal
            .draw(|f| {
                let area = f.area();
                match app.current_view().clone() {
                    View::Boards => crate::tui::boards::render(f, &app, area),
                    View::Threads => crate::tui::threads::render(f, &app, area),
                    View::Thread => crate::tui::thread::render(f, &app, area),
                    View::Invites => crate::tui::invites::render(f, &app, area),
                    View::Members => crate::tui::members::render(f, &app, area),
                    View::Search => crate::tui::search::render(f, &app, area),
                    View::Messages => crate::tui::messages::render_inbox(f, &app, area),
                    View::MessageThread => crate::tui::messages::render_thread(f, &app, area),
                    View::Bookmarks => crate::tui::bookmarks::render(f, &app, area),
                    View::Mentions => crate::tui::mentions::render(f, &app, area),
                    View::ServerPicker => crate::tui::server_picker::render(f, &app, area),
                }

                // Status message
                if let Some(msg) = &app.status_message {
                    let msg_area = Rect {
                        x: area.x + 1,
                        y: area.height.saturating_sub(4),
                        width: area.width.saturating_sub(2),
                        height: 1,
                    };
                    let p = Paragraph::new(msg.as_str());
                    f.render_widget(p, msg_area);
                }

                // Help overlay
                if app.show_help {
                    render_help(f, area);
                }
            })
            .map_err(|e| format!("Draw error: {}", e))?;

        if let Ok(Event::Key(key)) = event::read() {
            let action = input::handle_key(&mut app, key);
            match action {
                Action::None => {}
                Action::Quit => {
                    disable_raw_mode().ok();
                    execute!(terminal.backend_mut(), LeaveAlternateScreen).ok();
                    return Ok(TuiResult::Quit);
                }
                Action::SwitchServer { server_addr } => {
                    disable_raw_mode().ok();
                    execute!(terminal.backend_mut(), LeaveAlternateScreen).ok();
                    return Ok(TuiResult::SwitchServer { server_addr });
                }

                Action::FetchBoards => {
                    match api.get_boards().await {
                        Ok(resp) => {
                            cache::cache_boards(&app.cache, &resp.boards);
                            app.boards = resp.boards;
                            app.connection_state = ConnectionState::Online;
                        }
                        Err(e) => {
                            app.connection_state = ConnectionState::Offline;
                            app.status_message =
                                Some(format!("Offline — {}. Press r to retry.", e));
                        }
                    }
                    app.clamp_selection();
                }
                Action::FetchThreads { slug, page } => {
                    match api.get_threads(&slug, page).await {
                        Ok(resp) => {
                            cache::cache_threads(&app.cache, resp.board.id, &resp.threads);
                            app.threads = resp.threads;
                            app.total_pages = resp.total_pages;
                            app.connection_state = ConnectionState::Online;
                        }
                        Err(e) => {
                            app.status_message = Some(format!("Error: {}", e));
                        }
                    }
                    app.clamp_selection();
                }
                Action::FetchThread { thread_id, page } => {
                    match api.get_thread(thread_id, page).await {
                        Ok(resp) => {
                            cache::cache_posts(&app.cache, thread_id, &resp.posts);
                            if let Some(last) = resp.posts.last() {
                                cache::mark_thread_read(&app.cache, thread_id, last.id);
                            }
                            app.posts = resp.posts;
                            app.total_pages = resp.total_pages;
                        }
                        Err(e) => {
                            app.status_message = Some(format!("Error: {}", e));
                        }
                    }
                    // Clamp post cursor after refresh
                    if let Some(ref mut c) = app.post_cursor {
                        if app.posts.is_empty() {
                            app.post_cursor = None;
                        } else if *c >= app.posts.len() {
                            *c = app.posts.len() - 1;
                        }
                    }
                }
                Action::FetchInvites => {
                    match api.get_invites().await {
                        Ok(resp) => app.invites = resp.invites,
                        Err(e) => {
                            app.status_message = Some(format!("Error: {}", e));
                            continue;
                        }
                    }
                    app.push_view(View::Invites);
                }
                Action::FetchMembers => {
                    match api.get_users().await {
                        Ok(resp) => app.members = resp.users,
                        Err(e) => {
                            app.status_message = Some(format!("Error: {}", e));
                            continue;
                        }
                    }
                    app.push_view(View::Members);
                }
                Action::FetchInbox => {
                    match api.get_inbox().await {
                        Ok(resp) => app.dm_conversations = resp.conversations,
                        Err(e) => {
                            app.status_message = Some(format!("Error: {}", e));
                            continue;
                        }
                    }
                    app.push_view(View::Messages);
                }
                Action::FetchBookmarks => {
                    match api.list_bookmarks().await {
                        Ok(resp) => app.bookmarks = resp.bookmarks,
                        Err(e) => {
                            app.status_message = Some(format!("Error: {}", e));
                            continue;
                        }
                    }
                    app.push_view(View::Bookmarks);
                }
                Action::FetchMentions => {
                    match api.get_mentions(1).await {
                        Ok(resp) => {
                            app.mentions = resp.mentions;
                            app.push_view(View::Mentions);
                        }
                        Err(e) => app.status_message = Some(format!("Error: {}", e)),
                    }
                }
                Action::SearchQuery { query, by } => {
                    match api.search(&query, by.as_deref(), 1).await {
                        Ok(resp) => {
                            app.search_results = resp.results;
                            app.selected_index = 0;
                        }
                        Err(e) => {
                            app.status_message = Some(format!("Search error: {}", e));
                        }
                    }
                }
                Action::ToggleBookmark { thread_id } => {
                    match api.toggle_bookmark(thread_id).await {
                        Ok(resp) => {
                            if resp.bookmarked {
                                app.status_message =
                                    Some(format!("Thread #{} bookmarked", thread_id));
                            } else {
                                app.status_message =
                                    Some(format!("Thread #{} unbookmarked", thread_id));
                            }
                        }
                        Err(e) => app.status_message = Some(format!("Error: {}", e)),
                    }
                }
                Action::ReactToPost { thread_id, post_id, reaction } => {
                    match api.react_post(thread_id, post_id, &reaction).await {
                        Ok(resp) => {
                            let verb = if resp.added { "added" } else { "removed" };
                            app.status_message =
                                Some(format!("Reaction {} {}", resp.reaction, verb));
                            // Refresh thread
                            if let Ok(r) = api.get_thread(thread_id, app.current_page).await {
                                app.posts = r.posts;
                            }
                        }
                        Err(e) => app.status_message = Some(format!("Error: {}", e)),
                    }
                }
                Action::GenerateInvite => {
                    match api.create_invite().await {
                        Ok(resp) => {
                            app.status_message = Some(format!("Invite created: {}", resp.code));
                            match api.get_invites().await {
                                Ok(inv_resp) => app.invites = inv_resp.invites,
                                Err(e) => {
                                    app.status_message =
                                        Some(format!("Error refreshing: {}", e))
                                }
                            }
                            app.clamp_selection();
                        }
                        Err(e) => {
                            app.status_message = Some(format!("Error: {}", e));
                        }
                    }
                }
                Action::NextPage { context } => match context {
                    PageContext::Threads { slug } => {
                        match api.get_threads(&slug, app.current_page).await {
                            Ok(resp) => {
                                cache::cache_threads(
                                    &app.cache,
                                    resp.board.id,
                                    &resp.threads,
                                );
                                app.threads = resp.threads;
                                app.total_pages = resp.total_pages;
                                app.selected_index = 0;
                            }
                            Err(e) => {
                                app.current_page -= 1;
                                app.status_message = Some(format!("Error: {}", e));
                            }
                        }
                    }
                    PageContext::Thread { thread_id } => {
                        match api.get_thread(thread_id, app.current_page).await {
                            Ok(resp) => {
                                cache::cache_posts(&app.cache, thread_id, &resp.posts);
                                app.posts = resp.posts;
                                app.total_pages = resp.total_pages;
                                app.scroll_offset = 0;
                            }
                            Err(e) => {
                                app.current_page -= 1;
                                app.status_message = Some(format!("Error: {}", e));
                            }
                        }
                    }
                },
                Action::PrevPage { context } => match context {
                    PageContext::Threads { slug } => {
                        match api.get_threads(&slug, app.current_page).await {
                            Ok(resp) => {
                                cache::cache_threads(
                                    &app.cache,
                                    resp.board.id,
                                    &resp.threads,
                                );
                                app.threads = resp.threads;
                                app.total_pages = resp.total_pages;
                                app.selected_index = 0;
                            }
                            Err(e) => {
                                app.current_page += 1;
                                app.status_message = Some(format!("Error: {}", e));
                            }
                        }
                    }
                    PageContext::Thread { thread_id } => {
                        match api.get_thread(thread_id, app.current_page).await {
                            Ok(resp) => {
                                cache::cache_posts(&app.cache, thread_id, &resp.posts);
                                app.posts = resp.posts;
                                app.total_pages = resp.total_pages;
                                app.scroll_offset = 0;
                            }
                            Err(e) => {
                                app.current_page += 1;
                                app.status_message = Some(format!("Error: {}", e));
                            }
                        }
                    }
                },
                Action::OpenEditor { kind } => {
                    suspend_tui!();

                    match kind {
                        EditorKind::NewThread { board_slug } => {
                            println!("New thread in: {}", board_slug);
                            print!("Title: ");
                            use std::io::Write;
                            io::stdout().flush().ok();
                            let mut title = String::new();
                            io::stdin().read_line(&mut title).ok();
                            let title = title.trim().to_string();

                            if !title.is_empty() {
                                let content = format!(
                                    "# New thread in: {}\n# Title: {}\n#\n# --- Write your post below this line ---\n\n",
                                    board_slug, title
                                );
                                match editor::open_editor(
                                    &format!("thread_{}", board_slug),
                                    &content,
                                ) {
                                    Ok(Some(body)) => {
                                        match api
                                            .create_thread(&board_slug, &title, body.trim())
                                            .await
                                        {
                                            Ok(resp) => {
                                                println!("Thread created! ID: {}", resp.thread_id);
                                            }
                                            Err(e) => eprintln!("Error: {}", e),
                                        }
                                    }
                                    Ok(None) => println!("Empty post, aborting."),
                                    Err(e) => eprintln!("Editor error: {}", e),
                                }
                            } else {
                                println!("Empty title, aborting.");
                            }
                        }
                        EditorKind::Reply { thread_id, thread_title, board_slug } => {
                            let recent: Vec<_> =
                                if app.posts.len() > app.reply_context {
                                    app.posts[app.posts.len() - app.reply_context..].to_vec()
                                } else {
                                    app.posts.clone()
                                };
                            let context = editor::build_reply_context(
                                &thread_title,
                                thread_id,
                                &board_slug,
                                &recent,
                            );

                            match editor::open_editor(
                                &format!("reply_{}", thread_id),
                                &context,
                            ) {
                                Ok(Some(body)) => {
                                    match api.create_post(thread_id, body.trim()).await {
                                        Ok(resp) => {
                                            println!("Reply posted! Post #{}", resp.post_number);
                                        }
                                        Err(e) => eprintln!("Error: {}", e),
                                    }
                                }
                                Ok(None) => println!("Empty post, aborting."),
                                Err(e) => eprintln!("Editor error: {}", e),
                            }
                        }
                        EditorKind::ReplyTo {
                            thread_id,
                            thread_title,
                            board_slug,
                            parent_post_id,
                            parent_author,
                            parent_body,
                        } => {
                            // Build a minimal Post for build_reply_to_context
                            let parent_post = Post {
                                id: parent_post_id,
                                author: parent_author,
                                body: parent_body,
                                post_number: 0,
                                created_at: String::new(),
                                parent_post_id: None,
                                parent_post_number: None,
                                parent_author: None,
                                is_deleted: false,
                                edited_at: None,
                                reactions: Vec::new(),
                                attachments: Vec::new(),
                            };
                            let context = editor::build_reply_to_context(
                                &thread_title,
                                thread_id,
                                &board_slug,
                                &parent_post,
                            );

                            match editor::open_editor(
                                &format!("reply_{}_{}", thread_id, parent_post_id),
                                &context,
                            ) {
                                Ok(Some(body)) => {
                                    match api
                                        .create_post_reply(thread_id, body.trim(), parent_post_id)
                                        .await
                                    {
                                        Ok(resp) => {
                                            println!("Reply posted! Post #{}", resp.post_number);
                                        }
                                        Err(e) => eprintln!("Error: {}", e),
                                    }
                                }
                                Ok(None) => println!("Empty post, aborting."),
                                Err(e) => eprintln!("Editor error: {}", e),
                            }
                        }
                        EditorKind::EditPost { thread_id, post_id, old_body, thread_title } => {
                            let content = format!(
                                "# Editing post #{} in thread: {}\n# Modify the text below and save.\n\n{}",
                                post_id, thread_title, old_body
                            );
                            match editor::open_editor(
                                &format!("edit_{}_{}", thread_id, post_id),
                                &content,
                            ) {
                                Ok(Some(body)) => {
                                    let body = body.trim();
                                    if body == old_body.trim() {
                                        println!("No changes made.");
                                    } else if body.is_empty() {
                                        println!("Empty body, aborting.");
                                    } else {
                                        match api.edit_post(thread_id, post_id, body).await {
                                            Ok(resp) => println!(
                                                "Post {} edited (edit #{}).",
                                                resp.post_id, resp.edit_count
                                            ),
                                            Err(e) => eprintln!("Error: {}", e),
                                        }
                                    }
                                }
                                Ok(None) => println!("Empty post, aborting."),
                                Err(e) => eprintln!("Editor error: {}", e),
                            }
                        }
                        EditorKind::NewDm => {
                            print!("Recipient username: ");
                            use std::io::Write;
                            io::stdout().flush().ok();
                            let mut r = String::new();
                            io::stdin().read_line(&mut r).ok();
                            let recipient = r.trim().to_string();

                            if !recipient.is_empty() {
                                send_dm_via_editor(&api, &recipient).await;
                            } else {
                                println!("No recipient, aborting.");
                            }
                        }
                        EditorKind::DmToUser { recipient } => {
                            send_dm_via_editor(&api, &recipient).await;
                        }
                    }

                    println!("\nPress Enter to return to forum...");
                    let mut buf = String::new();
                    io::stdin().read_line(&mut buf).ok();

                    resume_tui!(terminal);
                }
                Action::CopyToClipboard { text } => {
                    crate::tui::invites::copy_to_clipboard(&text);
                    app.status_message = Some(format!("Copied to clipboard: {}", text));
                }
                Action::EnterBoard { index } => {
                    if let Some(board) = app.boards.get(index) {
                        let slug = board.slug.clone();
                        let board_id = board.id;
                        app.current_page = 1;
                        match api.get_threads(&slug, 1).await {
                            Ok(resp) => {
                                cache::cache_threads(
                                    &app.cache,
                                    resp.board.id,
                                    &resp.threads,
                                );
                                app.current_board = Some(resp.board);
                                app.threads = resp.threads;
                                app.total_pages = resp.total_pages;
                                app.connection_state = ConnectionState::Online;
                            }
                            Err(_) => {
                                app.threads =
                                    cache::get_cached_threads(&app.cache, board_id);
                                app.current_board = Some(BoardInfo {
                                    id: board_id,
                                    slug: slug.clone(),
                                    name: board.name.clone(),
                                    description: board.description.clone(),
                                });
                                app.total_pages = 1;
                                app.status_message =
                                    Some("Offline — showing cached data.".to_string());
                            }
                        }
                        app.push_view(View::Threads);
                    }
                }
                Action::EnterThread { thread_id } | Action::EnterSearchResult { thread_id } | Action::EnterBookmark { thread_id } | Action::EnterMention { thread_id } => {
                    app.last_read_post_id =
                        cache::get_last_read_post_id(&app.cache, thread_id);
                    app.post_cursor = None;
                    app.current_page = 1;
                    match api.get_thread(thread_id, 1).await {
                        Ok(resp) => {
                            cache::cache_posts(&app.cache, thread_id, &resp.posts);
                            if let Some(last) = resp.posts.last() {
                                cache::mark_thread_read(
                                    &app.cache,
                                    thread_id,
                                    last.id,
                                );
                            }
                            app.current_thread = Some(resp.thread);
                            app.posts = resp.posts;
                            app.total_pages = resp.total_pages;
                            app.push_view(View::Thread);
                        }
                        Err(e) => {
                            // For EnterThread from Threads view, fall back to cache
                            if let Some(thread_summary) = app.threads.iter().find(|t| t.id == thread_id) {
                                app.posts =
                                    cache::get_cached_posts(&app.cache, thread_id);
                                app.current_thread = Some(ThreadDetail {
                                    id: thread_id,
                                    board_id: app
                                        .current_board
                                        .as_ref()
                                        .map(|b| b.id)
                                        .unwrap_or(0),
                                    board_slug: app
                                        .current_board
                                        .as_ref()
                                        .map(|b| b.slug.clone())
                                        .unwrap_or_default(),
                                    title: thread_summary.title.clone(),
                                    author: thread_summary.author.clone(),
                                    created_at: thread_summary.created_at.clone(),
                                    pinned: thread_summary.pinned,
                                    locked: thread_summary.locked,
                                });
                                app.total_pages = 1;
                                app.status_message = Some(
                                    "Offline — showing cached data.".to_string(),
                                );
                                app.push_view(View::Thread);
                            } else {
                                app.status_message = Some(format!("Error: {}", e));
                            }
                        }
                    }
                }
                Action::EnterConversation { username } => {
                    match api.get_conversation(&username, 1).await {
                        Ok(resp) => {
                            let mut decrypted = Vec::new();
                            for msg in &resp.messages {
                                let plaintext = api
                                    .identity()
                                    .decrypt_from(
                                        &resp.partner_public_key,
                                        &msg.ciphertext,
                                        &msg.nonce,
                                    )
                                    .unwrap_or_else(|e| {
                                        format!("<decryption failed: {}>", e)
                                    });
                                decrypted.push((
                                    msg.created_at.clone(),
                                    msg.sender.clone(),
                                    plaintext,
                                ));
                            }
                            app.dm_partner = Some(username);
                            app.dm_decrypted = decrypted;
                            app.push_view(View::MessageThread);
                        }
                        Err(e) => {
                            app.status_message = Some(format!("Error: {}", e));
                        }
                    }
                }
            }
        }
    }
}

async fn send_dm_via_editor(api: &ApiClient, recipient: &str) {
    // Get recipient's public key
    let key_resp = match api.get_user_public_key(recipient).await {
        Ok(r) => r,
        Err(e) => {
            eprintln!("Error: {}", e);
            return;
        }
    };

    let content = format!(
        "# Direct message to: {}\n#\n# --- Write your message below this line ---\n\n",
        recipient
    );

    match crate::editor::open_editor(&format!("dm_{}", recipient), &content) {
        Ok(Some(body)) => {
            let body = body.trim();
            if body.is_empty() {
                println!("Empty message, aborting.");
                return;
            }
            match api.identity().encrypt_for(&key_resp.public_key, body) {
                Ok((ciphertext, nonce)) => {
                    match api.send_dm(recipient, &ciphertext, &nonce).await {
                        Ok(resp) => println!("Message sent to {} (id: {})", recipient, resp.dm_id),
                        Err(e) => eprintln!("Error sending: {}", e),
                    }
                }
                Err(e) => eprintln!("Encryption error: {}", e),
            }
        }
        Ok(None) => println!("Empty message, aborting."),
        Err(e) => eprintln!("Editor error: {}", e),
    }
}

fn render_help(f: &mut Frame, area: Rect) {
    let bold = Style::default().add_modifier(Modifier::BOLD);
    let dim = Style::default().add_modifier(Modifier::DIM);

    let width = 50u16.min(area.width.saturating_sub(4));
    let height = 32u16.min(area.height.saturating_sub(4));
    let x = (area.width.saturating_sub(width)) / 2;
    let y = (area.height.saturating_sub(height)) / 2;
    let help_area = Rect::new(x, y, width, height);

    f.render_widget(Clear, help_area);

    let help_text = vec![
        Line::from(""),
        Line::from(Span::styled("  Navigation", bold)),
        Line::from(Span::raw("  j / k / \u{2191}\u{2193}   Navigate lists")),
        Line::from(Span::raw("  Enter        Open selected item")),
        Line::from(Span::raw("  q / Esc      Go back (quit from home)")),
        Line::from(Span::raw("  ] / [        Next / prev page")),
        Line::from(Span::raw("  g / G        Jump to top / bottom")),
        Line::from(Span::raw("  PgUp / PgDn  Scroll by page")),
        Line::from(""),
        Line::from(Span::styled("  Actions", bold)),
        Line::from(Span::raw("  n            New thread / reply / DM")),
        Line::from(Span::raw("  e            Edit your last post")),
        Line::from(Span::raw("  r            Refresh current view")),
        Line::from(Span::raw("  b            Toggle bookmark")),
        Line::from(Span::raw("  /            Search")),
        Line::from(Span::raw("  y            Copy invite code")),
        Line::from(""),
        Line::from(Span::styled("  Views", bold)),
        Line::from(Span::raw("  i            Invites")),
        Line::from(Span::raw("  w            Members (who's online)")),
        Line::from(Span::raw("  m            Messages (DMs)")),
        Line::from(Span::raw("  @            @Mentions")),
        Line::from(Span::raw("  S            Switch server")),
        Line::from(""),
        Line::from(Span::styled("  Post Mode (Tab)", bold)),
        Line::from(Span::raw("  j / k        Select post")),
        Line::from(Span::raw("  R            Reply to selected post")),
        Line::from(Span::raw("  +            React to selected post")),
        Line::from(Span::raw("  Esc          Exit post mode")),
        Line::from(""),
        Line::from(Span::styled("  ? or Esc to close", dim)),
    ];

    let help = Paragraph::new(help_text)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(" Help "),
        )
        .wrap(Wrap { trim: false });
    f.render_widget(help, help_area);
}
