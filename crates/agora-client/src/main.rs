mod api;
mod cache;
mod cli;
mod config;
mod editor;
mod identity;
mod tui;

use clap::{CommandFactory, Parser, Subcommand};

#[derive(Parser)]
#[command(name = "agora", about = "AGORA — private invite-only forum", version = env!("CARGO_PKG_VERSION"),
    after_help = "\x1b[1mQuick reference:\x1b[0m
  agora                   Launch interactive TUI
  agora setup             First-time setup
  agora boards            List boards
  agora threads <board>   List threads
  agora read <id>         Read a thread
  agora guide             In-terminal documentation")]
struct Cli {
    /// Connect to a specific server (overrides default_server)
    #[arg(long, global = true)]
    server: Option<String>,

    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// First-run setup (identity + registration)
    Setup,
    /// List configured servers
    Servers {
        #[command(subcommand)]
        action: Option<ServerAction>,
    },
    /// List boards
    Boards,
    /// List threads in a board
    Threads {
        /// Board slug
        board_slug: String,
        /// Page number (default: 1)
        #[arg(short, long, default_value = "1")]
        page: i64,
    },
    /// Read a thread
    Read {
        /// Thread ID
        thread_id: i64,
        /// Page number (default: all pages)
        #[arg(short, long)]
        page: Option<i64>,
    },
    /// Create a new thread
    #[command(after_help = "Examples:
  agora post general \"My Title\"             Opens $EDITOR for body
  agora post general \"Title\" -f msg.txt     Body from file
  echo \"hi\" | agora post general \"T\" -f -   Body from stdin")]
    Post {
        /// Board slug
        board_slug: String,
        /// Thread title
        title: String,
        /// Read body from file (use - for stdin)
        #[arg(short, long)]
        file: Option<String>,
    },
    /// Reply to a thread
    #[command(after_help = "Examples:
  agora reply 42                Reply to thread 42 (opens $EDITOR)
  agora reply 42 -f reply.txt   Reply with file content
  agora reply 42 --to 5         Reply to specific post #5")]
    Reply {
        /// Thread ID
        thread_id: i64,
        /// Read body from file (use - for stdin)
        #[arg(short, long)]
        file: Option<String>,
        /// Reply to a specific post number
        #[arg(long)]
        to: Option<i64>,
    },
    /// Edit a post
    #[command(after_help = "Examples:
  agora edit 42 3               Edit post #3 in thread 42 (opens $EDITOR)
  agora edit 42 3 -f new.txt    Replace body with file content")]
    Edit {
        /// Thread ID
        thread_id: i64,
        /// Post number (the [#N] shown in the thread)
        post_number: i64,
        /// Read new body from file (use - for stdin)
        #[arg(short, long)]
        file: Option<String>,
    },
    /// View edit history of a post
    History {
        /// Thread ID
        thread_id: i64,
        /// Post number (the [#N] shown in the thread)
        post_number: i64,
    },
    /// Generate a new invite code
    Invite,
    /// List your invite codes
    Invites,
    /// Check connection to server
    Status,
    /// Clear local cache
    CacheClear,
    /// List all members
    Members,
    /// Show who's online
    Who,
    /// Search posts and threads
    #[command(after_help = "Examples:
  agora search \"rust async\"       Search all posts
  agora search --by alice          Posts by alice
  agora search \"bug\" --by bob     Posts by bob matching \"bug\"")]
    Search {
        /// Search query (optional if --by is provided)
        query: Option<String>,
        /// Filter results by author username
        #[arg(long)]
        by: Option<String>,
        /// Page number (default: 1)
        #[arg(short, long, default_value = "1")]
        page: i64,
    },
    /// Send a direct message
    #[command(after_help = "Examples:
  agora dm alice                   Opens $EDITOR for message
  agora dm alice -f msg.txt        Message from file")]
    Dm {
        /// Recipient username
        username: String,
        /// Read message from file (use - for stdin)
        #[arg(short, long)]
        file: Option<String>,
    },
    /// List DM conversations (inbox)
    Inbox,
    /// Read a DM conversation
    DmRead {
        /// Username of conversation partner
        username: String,
        /// Page number (default: 1)
        #[arg(short, long, default_value = "1")]
        page: i64,
    },
    /// List your bookmarks
    Bookmarks,
    /// Toggle bookmark on a thread
    Bookmark {
        /// Thread ID
        thread_id: i64,
    },
    /// Attach a file to a post
    #[command(after_help = "Example:
  agora attach 42 1 photo.jpg     Attach photo.jpg to post #1 in thread 42")]
    Attach {
        /// Thread ID
        thread_id: i64,
        /// Post number (the [#N] shown in the thread)
        post_number: i64,
        /// Path to file to attach
        file_path: String,
    },
    /// Download an attachment
    Download {
        /// Attachment ID
        attachment_id: i64,
        /// Output path (defaults to original filename)
        #[arg(short, long)]
        output: Option<String>,
    },
    /// Delete an attachment you uploaded
    Detach {
        /// Attachment ID
        attachment_id: i64,
    },
    /// React to a post (toggles — react again to remove)
    #[command(after_help = "Reactions are emoji characters (e.g. 👍, ❤️, 😂). React again to remove.

Example:
  agora react 42 1 👍             React with thumbs up to post #1 in thread 42")]
    React {
        /// Thread ID
        thread_id: i64,
        /// Post number (the [#N] shown in the thread)
        post_number: i64,
        /// Reaction name
        reaction: String,
    },
    /// View or set your bio (no args to view, or provide text to set)
    #[command(after_help = "Examples:
  agora bio                       View your bio
  agora bio \"Hello, I'm new!\"    Set your bio")]
    Bio {
        /// Bio text to set (omit to view current bio)
        text: Option<String>,
    },
    /// View posts that mention you
    #[command(name = "mentions")]
    Mentions {
        /// Page number (default: 1)
        #[arg(short, long, default_value = "1")]
        page: i64,
    },
    /// Export or import your profile (identity keys + server configs)
    Profile {
        #[command(subcommand)]
        action: ProfileAction,
    },
    /// Quick-start guide and help topics
    Guide {
        /// Topic: getting-started, tui, cli, servers, moderation
        topic: Option<String>,
    },
    /// Moderation commands
    #[command(after_help = "Actions: pin, unpin, lock, unlock, delete, restore, ban, unban, set-role
Run: agora guide moderation   for full details and examples")]
    Mod {
        #[command(subcommand)]
        action: ModAction,
    },
    /// Generate shell completions
    #[command(after_help = "Examples:
  agora completions bash >> ~/.bashrc
  agora completions zsh >> ~/.zshrc
  agora completions fish > ~/.config/fish/completions/agora.fish")]
    Completions {
        /// Shell to generate completions for
        shell: clap_complete::Shell,
    },
}

#[derive(Subcommand)]
enum ServerAction {
    /// Set the default server
    SetDefault {
        /// Server address to set as default
        server_addr: String,
    },
    /// Update a server's address (e.g. when the .onion changes)
    UpdateAddress {
        /// Current server address
        old_address: String,
        /// New server address
        new_address: String,
    },
    /// Remove a server (deletes local identity and cache, not server data)
    Remove {
        /// Server address to remove
        server_addr: String,
    },
}

#[derive(Subcommand)]
enum ProfileAction {
    /// Export identity and server configs to a file
    Export {
        /// Output file path (default: agora-profile.toml)
        #[arg(short, long)]
        output: Option<String>,
    },
    /// Import identity and server configs from a file
    Import {
        /// Path to the profile export file
        file: String,
        /// Overwrite existing server configs without prompting
        #[arg(long)]
        force: bool,
    },
}

#[derive(Subcommand)]
enum ModAction {
    /// Pin a thread
    Pin {
        thread_id: i64,
    },
    /// Unpin a thread
    Unpin {
        thread_id: i64,
    },
    /// Lock a thread (no new replies)
    Lock {
        thread_id: i64,
    },
    /// Unlock a thread
    Unlock {
        thread_id: i64,
    },
    /// Delete a post (soft delete)
    Delete {
        thread_id: i64,
        post_number: i64,
    },
    /// Restore a deleted post
    Restore {
        thread_id: i64,
        post_number: i64,
    },
    /// Delete a thread (soft delete, hides from listings)
    DeleteThread {
        thread_id: i64,
    },
    /// Restore a deleted thread
    RestoreThread {
        thread_id: i64,
    },
    /// Ban a user
    Ban {
        username: String,
    },
    /// Unban a user
    Unban {
        username: String,
    },
    /// Set a user's role (admin only)
    SetRole {
        username: String,
        /// Role: member, mod, or admin
        role: String,
    },
}

#[tokio::main]
async fn main() {
    let cli = Cli::parse();

    let result = match cli.command {
        Some(Commands::Setup) => cli::setup::run(cli.server.as_deref()).await,
        Some(Commands::Servers { action }) => match action {
            None => cli::servers::run(),
            Some(ServerAction::SetDefault { server_addr }) => {
                config::set_default_server(&server_addr).map(|()| {
                    println!("Default server set to: {}", server_addr);
                })
            }
            Some(ServerAction::UpdateAddress {
                old_address,
                new_address,
            }) => cli::servers::update_address(&old_address, &new_address),
            Some(ServerAction::Remove { server_addr }) => {
                cli::servers::remove(&server_addr)
            }
        },
        Some(Commands::Profile { action }) => match action {
            ProfileAction::Export { output } => cli::profile::export(output.as_deref()),
            ProfileAction::Import { file, force } => cli::profile::import(&file, force),
        },
        Some(Commands::Completions { shell }) => {
            clap_complete::generate(
                shell,
                &mut Cli::command(),
                "agora",
                &mut std::io::stdout(),
            );
            Ok(())
        }
        Some(Commands::Guide { topic }) => cli::guide::run(topic.as_deref()),
        Some(cmd) => run_authenticated(cmd, cli.server.as_deref()).await,
        None => run_tui(cli.server.as_deref()).await,
    };

    if let Err(e) = result {
        eprintln!("Error: {}", e);
        std::process::exit(1);
    }
}

/// Resolve which server address to use: --server flag > last_server > default_server > error.
fn resolve_server(override_server: Option<&str>) -> Result<String, String> {
    if let Some(s) = override_server {
        return Ok(s.to_string());
    }
    let global = config::GlobalConfig::load()?;
    global
        .last_server
        .or(global.default_server)
        .ok_or_else(|| {
            "Welcome to Agora!\n\n\
             No server configured yet. To get started:\n\
             \x20 1. Get an invite code and server address from someone\n\
             \x20 2. Run: agora setup\n\n\
             For more info: agora guide getting-started"
                .to_string()
        })
}

fn build_api(
    server: &str,
    socks_proxy: &str,
    identity: identity::Identity,
) -> Result<api::ApiClient, String> {
    if server.contains(".onion") {
        api::ApiClient::new(server, socks_proxy, identity)
    } else {
        api::ApiClient::new_direct(server, identity)
    }
}

/// Resolve a post number (the [#N] users see) to the database post ID.
async fn resolve_post_id(api: &api::ApiClient, thread_id: i64, post_number: i64) -> Result<i64, String> {
    let mut page = 1;
    loop {
        let resp = api.get_thread(thread_id, page).await?;
        if let Some(post) = resp.posts.iter().find(|p| p.post_number == post_number) {
            return Ok(post.id);
        }
        if page >= resp.total_pages {
            break;
        }
        page += 1;
    }
    Err(format!("Post #{} not found in thread {}", post_number, thread_id))
}

async fn run_authenticated(cmd: Commands, override_server: Option<&str>) -> Result<(), String> {
    let server_addr = resolve_server(override_server)?;
    let global = config::GlobalConfig::load_or_default();
    let _srv_cfg = config::ServerConfig::load(&server_addr)?;
    let id = identity::Identity::load_for(&server_addr)?;
    let db = cache::open_for(&server_addr);
    let api = build_api(&server_addr, &global.socks_proxy, id)?;

    match cmd {
        Commands::Boards => cli::boards::run(&api, &db).await,
        Commands::Threads { board_slug, page } => cli::threads::run(&api, &db, &board_slug, page).await,
        Commands::Read { thread_id, page } => cli::read::run(&api, &db, thread_id, page).await,
        Commands::Post {
            board_slug,
            title,
            file,
        } => cli::post::run(&api, &board_slug, &title, file.as_deref()).await,
        Commands::Reply { thread_id, file, to } => {
            cli::reply::run(&api, &db, thread_id, file.as_deref(), global.reply_context, to).await
        }
        Commands::Edit {
            thread_id,
            post_number,
            file,
        } => {
            let post_id = resolve_post_id(&api, thread_id, post_number).await?;
            cli::edit::run(&api, thread_id, post_id, file.as_deref()).await
        }
        Commands::History {
            thread_id,
            post_number,
        } => {
            let post_id = resolve_post_id(&api, thread_id, post_number).await?;
            cli::edit::history(&api, thread_id, post_id).await
        }
        Commands::Invite => cli::invite::generate(&api).await,
        Commands::Invites => cli::invite::list(&api).await,
        Commands::Status => {
            match api.check_connection().await {
                Ok(()) => {
                    println!("Connected to server.");
                    let me = api.get_me().await?;
                    println!("Logged in as: {}", me.username);
                    println!("Role: {}", me.role);
                    println!("Member since: {}", me.created_at);
                    if let Some(inviter) = me.invited_by {
                        println!("Invited by: {}", inviter);
                    }
                    if !me.bio.is_empty() {
                        println!("Bio: {}", me.bio);
                    }
                }
                Err(e) => println!("Offline: {}", e),
            }
            Ok(())
        }
        Commands::CacheClear => {
            cache::clear_cache_for(&server_addr)?;
            println!("Cache cleared.");
            Ok(())
        }
        Commands::Members => cli::members::members(&api).await,
        Commands::Who => cli::members::who(&api).await,
        Commands::Search { query, by, page } => {
            let q = query.as_deref().unwrap_or("");
            if q.is_empty() && by.is_none() {
                return Err("Provide a search query or --by <username>".to_string());
            }
            cli::search::run(&api, q, by.as_deref(), page).await
        }
        Commands::Dm { username, file } => {
            let id = identity::Identity::load_for(&server_addr)?;
            cli::dm::send(&api, &id, &username, file.as_deref()).await
        }
        Commands::Inbox => cli::dm::inbox(&api).await,
        Commands::DmRead { username, page } => {
            let id = identity::Identity::load_for(&server_addr)?;
            cli::dm::read_conversation(&api, &id, &username, page).await
        }
        Commands::Bookmarks => cli::bookmark::list(&api).await,
        Commands::Bookmark { thread_id } => cli::bookmark::toggle(&api, thread_id).await,
        Commands::Attach {
            thread_id,
            post_number,
            file_path,
        } => {
            let post_id = resolve_post_id(&api, thread_id, post_number).await?;
            cli::attach::upload(&api, thread_id, post_id, &file_path).await
        }
        Commands::Download {
            attachment_id,
            output,
        } => cli::attach::download(&api, attachment_id, output.as_deref()).await,
        Commands::Detach { attachment_id } => {
            match api.delete_attachment(attachment_id).await {
                Ok(resp) => {
                    println!("{}", resp.message);
                    Ok(())
                }
                Err(e) => Err(e),
            }
        }
        Commands::React {
            thread_id,
            post_number,
            reaction,
        } => {
            let post_id = resolve_post_id(&api, thread_id, post_number).await?;
            cli::react::run(&api, thread_id, post_id, &reaction).await
        }
        Commands::Bio { text } => cli::bio::run(&api, text.as_deref()).await,
        Commands::Mentions { page } => cli::mentions::run(&api, page).await,
        Commands::Mod { action } => {
            match action {
                ModAction::Pin { thread_id } => {
                    let resp = api.mod_thread(thread_id, "pin").await?;
                    println!("{}", resp.message);
                }
                ModAction::Unpin { thread_id } => {
                    let resp = api.mod_thread(thread_id, "unpin").await?;
                    println!("{}", resp.message);
                }
                ModAction::Lock { thread_id } => {
                    let resp = api.mod_thread(thread_id, "lock").await?;
                    println!("{}", resp.message);
                }
                ModAction::Unlock { thread_id } => {
                    let resp = api.mod_thread(thread_id, "unlock").await?;
                    println!("{}", resp.message);
                }
                ModAction::Delete { thread_id, post_number } => {
                    let post_id = resolve_post_id(&api, thread_id, post_number).await?;
                    let resp = api.mod_post(thread_id, post_id, "delete").await?;
                    println!("{}", resp.message);
                }
                ModAction::Restore { thread_id, post_number } => {
                    let post_id = resolve_post_id(&api, thread_id, post_number).await?;
                    let resp = api.mod_post(thread_id, post_id, "restore").await?;
                    println!("{}", resp.message);
                }
                ModAction::DeleteThread { thread_id } => {
                    let resp = api.mod_thread(thread_id, "delete").await?;
                    println!("{}", resp.message);
                }
                ModAction::RestoreThread { thread_id } => {
                    let resp = api.mod_thread(thread_id, "restore").await?;
                    println!("{}", resp.message);
                }
                ModAction::Ban { username } => {
                    let resp = api.mod_user(&username, "ban", None).await?;
                    println!("{}", resp.message);
                }
                ModAction::Unban { username } => {
                    let resp = api.mod_user(&username, "unban", None).await?;
                    println!("{}", resp.message);
                }
                ModAction::SetRole { username, role } => {
                    let resp = api.mod_user(&username, "set_role", Some(&role)).await?;
                    println!("{}", resp.message);
                }
            }
            Ok(())
        }
        Commands::Setup | Commands::Servers { .. } | Commands::Profile { .. } | Commands::Guide { .. } | Commands::Completions { .. } => unreachable!(),
    }
}

async fn run_tui(override_server: Option<&str>) -> Result<(), String> {
    let mut server_addr = resolve_server(override_server)?;

    loop {
        let global = config::GlobalConfig::load_or_default();
        let srv_cfg = config::ServerConfig::load(&server_addr)?;
        let id = identity::Identity::load_for(&server_addr)?;
        let db = cache::open_for(&server_addr);
        let api = build_api(&server_addr, &global.socks_proxy, id)?;

        let server_name = srv_cfg
            .server_name
            .clone()
            .unwrap_or_else(|| "UNNAMED-SERVER".to_string());

        config::set_last_server(&server_addr)?;

        match tui::app::run_tui(api, server_addr.clone(), server_name, srv_cfg.username.clone(), db, global.reply_context).await? {
            tui::app::TuiResult::Quit => break,
            tui::app::TuiResult::SwitchServer { server_addr: new } => {
                server_addr = new;
            }
        }
    }

    Ok(())
}
