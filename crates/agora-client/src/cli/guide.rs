const TOPICS: &[(&str, &str)] = &[
    ("getting-started", "First-time setup and orientation"),
    ("tui", "TUI keyboard shortcuts and navigation"),
    ("cli", "CLI commands with examples"),
    ("servers", "Multi-server management"),
    ("moderation", "Moderation roles and commands"),
];

const GETTING_STARTED: &str = r#"
  GETTING STARTED WITH AGORA
  ==========================

  Agora is a private, invite-only forum that runs over Tor.
  Think of it as a lightweight Discord you access from your terminal.

  WHAT YOU NEED
    - An invite code from someone already on a server
    - The server's .onion address
    - Tor running on your machine (the installer handles this)

  SETUP
    Run:
      agora setup

    This will:
      1. Auto-detect your Tor SOCKS proxy
      2. Ask for the server address and invite code
      3. Generate your ed25519 identity keypair
      4. Register your username on the server

  USING AGORA
    Launch the TUI (interactive mode):
      agora

    Or use CLI commands directly:
      agora boards           List all boards
      agora threads general  List threads in "general"
      agora read 42          Read thread #42

  NEXT STEPS
    - Press ? in the TUI for keyboard shortcuts
    - Run: agora guide tui       for TUI navigation
    - Run: agora guide cli       for all CLI commands
    - Run: agora guide servers    for multi-server setup
"#;

const TUI: &str = r#"
  TUI KEYBOARD SHORTCUTS
  ======================

  GLOBAL
    q / Esc     Go back / quit
    ?           Show help overlay
    S           Open server picker
    r           Refresh current view
    /           Search posts and threads

  BOARD LIST
    j / Down    Move down
    k / Up      Move up
    Enter       Open board (list threads)
    i           View invites
    m           View members
    M           View messages (DMs)
    B           View bookmarks
    @           View mentions

  THREAD LIST
    j / Down    Move down
    k / Up      Move up
    Enter       Open thread
    n           New thread
    [ / ]       Previous / next page

  THREAD VIEW
    j / Down    Scroll down
    k / Up      Scroll up
    Space       Page down
    g / G       Jump to top / bottom
    n           Reply to thread
    s           Select post mode
    b           Toggle bookmark

  POST SELECTION (press s in thread view)
    j / k       Move between posts
    r           Reply to selected post
    e           Edit your post
    y           Copy post text
    a           Attach file to post
    +           React to post
    Esc         Exit selection mode

  SERVER PICKER (press S)
    j / k       Move between servers
    Enter       Switch to selected server
    Esc         Cancel
"#;

const CLI: &str = r#"
  CLI COMMANDS WITH EXAMPLES
  ==========================

  BROWSING
    agora boards                          List all boards
    agora threads general                 Threads in "general"
    agora read 42                         Read thread #42

  POSTING
    agora post general "My Title"         Opens $EDITOR for body
    agora post general "Title" -f msg.txt Body from file
    echo "hello" | agora post general "Title" -f -
                                          Body from stdin

  REPLYING
    agora reply 42                        Reply to thread 42
    agora reply 42 -f reply.txt           Reply from file
    agora reply 42 --to 5                 Reply to post #5

  EDITING
    agora edit 42 3                       Edit post 3 in thread 42
    agora edit 42 3 -f updated.txt        Edit from file

  SEARCHING
    agora search "rust async"             Search all posts
    agora search --by alice               Posts by alice
    agora search "bug" --by bob           Posts by bob matching "bug"

  DIRECT MESSAGES
    agora dm alice                        Opens $EDITOR for message
    agora dm alice -f msg.txt             Message from file
    agora inbox                           List DM conversations
    agora dm-read alice                   Read conversation with alice

  FILES & REACTIONS
    agora attach 42 1 photo.jpg           Attach file to post
    agora download 7                      Download attachment #7
    agora download 7 -o pic.jpg           Download to specific path
    agora react 42 1 heart                React to post

  ACCOUNT & META
    agora status                          Check connection + identity
    agora invite                          Generate invite code
    agora invites                         List your invites
    agora members                         List all members
    agora who                             Who's online
    agora bio "I like Rust"               Set your bio
    agora bookmarks                       List bookmarked threads
    agora bookmark 42                     Toggle bookmark
    agora mentions                        View posts mentioning you
    agora cache-clear                     Clear local cache

  SERVERS & PROFILE
    agora servers                         List configured servers
    agora servers set-default <addr>      Set default server
    agora servers update-address <o> <n>  Update a server's address
    agora servers remove <addr>           Remove a server
    agora profile export                  Back up identities
    agora profile import backup.toml      Restore identities
"#;

const SERVERS: &str = r#"
  MULTI-SERVER MANAGEMENT
  =======================

  Agora supports connecting to multiple servers. Each server gets
  its own identity, cache, and config stored separately.

  LIST SERVERS
    agora servers                         Show all configured servers

  SET DEFAULT
    agora servers set-default <addr>      Set which server to connect to

  UPDATE ADDRESS (server moved to new .onion)
    agora servers update-address <old> <new>

  REMOVE A SERVER
    agora servers remove <addr>           Deletes local identity + cache

  CONNECT TO SPECIFIC SERVER
    agora --server <addr> boards          Use --server flag on any command
    agora --server <addr>                 Launch TUI for specific server

  PROFILE BACKUP & RESTORE
    agora profile export                  Export all identities to a file
    agora profile export -o backup.toml   Export to specific path
    agora profile import backup.toml      Restore on a new device
    agora profile import backup.toml --force
                                          Overwrite without prompting

  HOW IT WORKS
    - Each server has its own directory under ~/.agora/servers/
    - Your identity (keypair) is unique per server
    - Your cache is separate per server
    - The last server you connected to is remembered

  TUI SERVER SWITCHING
    Press S in the TUI to open the server picker and switch
    between configured servers without restarting.

  ADDING A NEW SERVER
    Just run setup with the new server address:
      agora setup
    Or:
      agora --server <new-addr> setup

  SHELL COMPLETIONS
    agora completions bash >> ~/.bashrc
    agora completions zsh >> ~/.zshrc
    agora completions fish > ~/.config/fish/completions/agora.fish
"#;

const MODERATION: &str = r#"
  MODERATION
  ==========

  Agora has three roles: member, mod, and admin.
  The first user to register is automatically promoted to admin.

  ROLES
    member    Can post, reply, react, DM — standard access
    mod       Can also pin/lock threads, delete/restore posts, ban users
    admin     Can do everything mods can + set user roles

  THREAD MODERATION
    agora mod pin 42                      Pin thread 42
    agora mod unpin 42                    Unpin thread 42
    agora mod lock 42                     Lock thread (no new replies)
    agora mod unlock 42                   Unlock thread

  POST MODERATION
    agora mod delete 42 3                 Soft-delete post 3 in thread 42
    agora mod restore 42 3               Restore deleted post

  USER MODERATION
    agora mod ban alice                   Ban user alice
    agora mod unban alice                 Unban user alice
    agora mod set-role alice mod          Promote alice to mod
    agora mod set-role alice member       Demote to member

  NOTES
    - Deleted posts show "[deleted]" but are preserved in the database
    - Banned users cannot post, reply, or create threads
    - Only admins can change roles
"#;

pub fn run(topic: Option<&str>) -> Result<(), String> {
    match topic {
        None => {
            println!();
            println!("  AGORA GUIDE");
            println!("  ==========");
            println!();
            println!("  Usage: agora guide <topic>");
            println!();
            for (name, desc) in TOPICS {
                println!("    {:<20} {}", name, desc);
            }
            println!();
            println!("  Tip: pipe to less for scrollable output:");
            println!("    agora guide tui | less");
            println!();
            Ok(())
        }
        Some(t) => {
            let content = match t {
                "getting-started" => GETTING_STARTED,
                "tui" => TUI,
                "cli" => CLI,
                "servers" => SERVERS,
                "moderation" => MODERATION,
                _ => {
                    return Err(format!(
                        "Unknown topic: {}\nAvailable: {}",
                        t,
                        TOPICS.iter().map(|(n, _)| *n).collect::<Vec<_>>().join(", ")
                    ));
                }
            };
            print!("{}", content);
            Ok(())
        }
    }
}
