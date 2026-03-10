use rand::Rng;
use rusqlite::{params, Connection};
use std::sync::{Arc, Mutex};
use tracing::info;

pub const SERVER_VERSION: &str = env!("CARGO_PKG_VERSION");
pub const MAX_ATTACHMENT_SIZE: usize = 5 * 1024 * 1024; // 5 MB
// Reaction validation: server accepts any emoji string up to MAX_REACTION_LEN bytes

pub type Db = Arc<Mutex<Connection>>;

pub fn open(path: &str) -> Db {
    let conn = Connection::open(path).expect("failed to open database");
    conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON;")
        .expect("failed to set pragmas");
    info!("Database opened: {}", path);
    Arc::new(Mutex::new(conn))
}

pub fn migrate(db: &Db) {
    let conn = db.lock().unwrap_or_else(|e| e.into_inner());
    conn.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS users (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            username TEXT NOT NULL UNIQUE,
            public_key TEXT NOT NULL UNIQUE,
            invited_by INTEGER REFERENCES users(id),
            created_at TEXT NOT NULL DEFAULT (datetime('now'))
        );

        CREATE TABLE IF NOT EXISTS invite_codes (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            code TEXT NOT NULL UNIQUE,
            created_by INTEGER NOT NULL REFERENCES users(id),
            used_by INTEGER REFERENCES users(id),
            created_at TEXT NOT NULL DEFAULT (datetime('now')),
            used_at TEXT
        );

        CREATE TABLE IF NOT EXISTS boards (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            slug TEXT NOT NULL UNIQUE,
            name TEXT NOT NULL,
            description TEXT NOT NULL DEFAULT '',
            sort_order INTEGER NOT NULL DEFAULT 0,
            created_at TEXT NOT NULL DEFAULT (datetime('now'))
        );

        CREATE TABLE IF NOT EXISTS threads (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            board_id INTEGER NOT NULL REFERENCES boards(id),
            author_id INTEGER NOT NULL REFERENCES users(id),
            title TEXT NOT NULL,
            created_at TEXT NOT NULL DEFAULT (datetime('now')),
            last_post_at TEXT NOT NULL DEFAULT (datetime('now'))
        );

        CREATE TABLE IF NOT EXISTS posts (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            thread_id INTEGER NOT NULL REFERENCES threads(id),
            author_id INTEGER NOT NULL REFERENCES users(id),
            body TEXT NOT NULL,
            created_at TEXT NOT NULL DEFAULT (datetime('now'))
        );

        CREATE INDEX IF NOT EXISTS idx_threads_board ON threads(board_id, last_post_at DESC);
        CREATE INDEX IF NOT EXISTS idx_posts_thread ON posts(thread_id, created_at ASC);
        CREATE INDEX IF NOT EXISTS idx_invite_codes_code ON invite_codes(code);

        CREATE TABLE IF NOT EXISTS direct_messages (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            sender_id INTEGER NOT NULL REFERENCES users(id),
            recipient_id INTEGER NOT NULL REFERENCES users(id),
            ciphertext TEXT NOT NULL,
            nonce TEXT NOT NULL,
            created_at TEXT NOT NULL DEFAULT (datetime('now'))
        );

        CREATE INDEX IF NOT EXISTS idx_dm_sender ON direct_messages(sender_id, created_at);
        CREATE INDEX IF NOT EXISTS idx_dm_recipient ON direct_messages(recipient_id, created_at);

        -- Post edit history
        CREATE TABLE IF NOT EXISTS post_edits (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            post_id INTEGER NOT NULL REFERENCES posts(id),
            old_body TEXT NOT NULL,
            edited_at TEXT NOT NULL DEFAULT (datetime('now'))
        );
        CREATE INDEX IF NOT EXISTS idx_post_edits_post ON post_edits(post_id, edited_at ASC);

        -- Bookmarks
        CREATE TABLE IF NOT EXISTS bookmarks (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            user_id INTEGER NOT NULL REFERENCES users(id),
            thread_id INTEGER NOT NULL REFERENCES threads(id),
            created_at TEXT NOT NULL DEFAULT (datetime('now')),
            UNIQUE(user_id, thread_id)
        );
        CREATE INDEX IF NOT EXISTS idx_bookmarks_user ON bookmarks(user_id, created_at DESC);

        -- File attachments
        CREATE TABLE IF NOT EXISTS attachments (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            post_id INTEGER NOT NULL REFERENCES posts(id),
            filename TEXT NOT NULL,
            content_type TEXT NOT NULL DEFAULT 'application/octet-stream',
            size_bytes INTEGER NOT NULL,
            data BLOB NOT NULL,
            created_at TEXT NOT NULL DEFAULT (datetime('now'))
        );
        CREATE INDEX IF NOT EXISTS idx_attachments_post ON attachments(post_id);
        ",
    )
    .expect("failed to run migrations");

    // Add last_seen_at column (idempotent)
    conn.execute("ALTER TABLE users ADD COLUMN last_seen_at TEXT", [])
        .ok();

    // Add role column (idempotent)
    conn.execute("ALTER TABLE users ADD COLUMN role TEXT NOT NULL DEFAULT 'member'", [])
        .ok();

    // Add is_banned column (idempotent)
    conn.execute("ALTER TABLE users ADD COLUMN is_banned INTEGER NOT NULL DEFAULT 0", [])
        .ok();

    // Add edited_at column on posts (idempotent)
    conn.execute("ALTER TABLE posts ADD COLUMN edited_at TEXT", [])
        .ok();

    // Add is_deleted column on posts (idempotent)
    conn.execute("ALTER TABLE posts ADD COLUMN is_deleted INTEGER NOT NULL DEFAULT 0", [])
        .ok();

    // Add pinned column on threads (idempotent)
    conn.execute("ALTER TABLE threads ADD COLUMN pinned INTEGER NOT NULL DEFAULT 0", [])
        .ok();

    // Add locked_at column on threads (idempotent)
    conn.execute("ALTER TABLE threads ADD COLUMN locked_at TEXT", [])
        .ok();

    // Add is_deleted column on threads (idempotent)
    conn.execute("ALTER TABLE threads ADD COLUMN is_deleted INTEGER NOT NULL DEFAULT 0", [])
        .ok();

    // Add parent_post_id column on posts (Feature: reply-to threading)
    conn.execute(
        "ALTER TABLE posts ADD COLUMN parent_post_id INTEGER",
        [],
    )
    .ok();

    // Reactions table (Feature: reactions)
    conn.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS reactions (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            post_id INTEGER NOT NULL REFERENCES posts(id),
            user_id INTEGER NOT NULL REFERENCES users(id),
            reaction TEXT NOT NULL,
            created_at TEXT NOT NULL DEFAULT (datetime('now')),
            UNIQUE(post_id, user_id, reaction)
        );
        CREATE INDEX IF NOT EXISTS idx_reactions_post ON reactions(post_id);
        ",
    )
    .expect("failed to create reactions table");

    // Add bio column on users (Feature: user bios)
    conn.execute(
        "ALTER TABLE users ADD COLUMN bio TEXT NOT NULL DEFAULT ''",
        [],
    )
    .ok();

    // Add edited_by column to post_edits (idempotent)
    conn.execute("ALTER TABLE post_edits ADD COLUMN edited_by TEXT", [])
        .ok();

    // FTS5 search index
    conn.execute_batch(
        "
        CREATE VIRTUAL TABLE IF NOT EXISTS search_index USING fts5(text_content, tokenize='porter unicode61');
        CREATE TABLE IF NOT EXISTS search_map (
            id INTEGER PRIMARY KEY,
            kind TEXT NOT NULL,
            thread_id INTEGER NOT NULL,
            post_id INTEGER NOT NULL DEFAULT 0
        );
        ",
    )
    .expect("failed to create search tables");

    // Backfill existing content if search_map is empty
    let map_count: i64 = conn
        .query_row("SELECT COUNT(*) FROM search_map", [], |r| r.get(0))
        .unwrap_or(0);

    info!("Migrations complete");

    if map_count == 0 {
        // Index existing threads
        let mut stmt = conn
            .prepare("SELECT id, title FROM threads")
            .unwrap();
        let threads: Vec<(i64, String)> = stmt
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
            .unwrap()
            .filter_map(|r| r.ok())
            .collect();

        for (thread_id, title) in &threads {
            index_search_inner(&conn, title, "thread", *thread_id, 0);
        }

        // Index existing posts
        let mut stmt = conn
            .prepare("SELECT id, thread_id, body FROM posts")
            .unwrap();
        let posts: Vec<(i64, i64, String)> = stmt
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))
            .unwrap()
            .filter_map(|r| r.ok())
            .collect();

        for (post_id, thread_id, body) in &posts {
            index_search_inner(&conn, body, "post", *thread_id, *post_id);
        }
    }
}

fn index_search_inner(conn: &Connection, text: &str, kind: &str, thread_id: i64, post_id: i64) {
    conn.execute(
        "INSERT INTO search_index (text_content) VALUES (?1)",
        [text],
    )
    .ok();
    let rowid = conn.last_insert_rowid();
    conn.execute(
        "INSERT INTO search_map (id, kind, thread_id, post_id) VALUES (?1, ?2, ?3, ?4)",
        params![rowid, kind, thread_id, post_id],
    )
    .ok();
}

/// Index content for full-text search (called from route handlers).
pub fn index_search(conn: &Connection, text: &str, kind: &str, thread_id: i64, post_id: i64) {
    index_search_inner(conn, text, kind, thread_id, post_id);
}

/// Seed default boards and bootstrap invite if DB is empty.
/// Returns the bootstrap invite code if one was created.
pub fn seed(db: &Db) -> Option<String> {
    let conn = db.lock().unwrap_or_else(|e| e.into_inner());

    let board_count: i64 = conn
        .query_row("SELECT COUNT(*) FROM boards", [], |r| r.get(0))
        .unwrap_or(0);

    if board_count > 0 {
        return None;
    }

    info!("Database seeded with default boards");

    // Create default boards
    let defaults = [
        ("general", "General", "General discussion", 0),
        ("meta", "Meta", "Discussion about the forum itself", 1),
        ("off-topic", "Off-Topic", "Everything else", 2),
    ];

    for (slug, name, desc, order) in &defaults {
        conn.execute(
            "INSERT INTO boards (slug, name, description, sort_order) VALUES (?1, ?2, ?3, ?4)",
            params![slug, name, desc, order],
        )
        .expect("failed to insert default board");
    }

    // Create a bootstrap user (system) for the invite
    conn.execute(
        "INSERT INTO users (id, username, public_key) VALUES (0, '_system', '_system')",
        [],
    )
    .expect("failed to create system user");

    // Generate bootstrap invite code
    let code = generate_invite_code();
    conn.execute(
        "INSERT INTO invite_codes (code, created_by) VALUES (?1, 0)",
        params![&code],
    )
    .expect("failed to create bootstrap invite");

    Some(code)
}

pub fn generate_invite_code() -> String {
    let mut rng = rand::thread_rng();
    let chars: Vec<char> = "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789"
        .chars()
        .collect();
    (0..16).map(|_| chars[rng.gen_range(0..chars.len())]).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generate_invite_code_length_and_charset() {
        let code = generate_invite_code();
        assert_eq!(code.len(), 16);
        assert!(code.chars().all(|c| c.is_ascii_alphanumeric()));
    }

    #[test]
    fn generate_invite_code_unique() {
        let code1 = generate_invite_code();
        let code2 = generate_invite_code();
        assert_ne!(code1, code2);
    }

    #[test]
    fn open_and_migrate_in_memory() {
        let db = open(":memory:");
        migrate(&db);
        // If we got here without panic, migrations succeeded.
        // Verify a table exists by querying it.
        let conn = db.lock().unwrap();
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM boards", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 0);
    }

    #[test]
    fn seed_returns_code_first_time_then_none() {
        let db = open(":memory:");
        migrate(&db);

        let first = seed(&db);
        assert!(first.is_some());
        let code = first.unwrap();
        assert_eq!(code.len(), 16);
        assert!(code.chars().all(|c| c.is_ascii_alphanumeric()));

        // Second call should return None (boards already exist)
        let second = seed(&db);
        assert!(second.is_none());
    }

    #[test]
    fn index_search_does_not_panic() {
        let db = open(":memory:");
        migrate(&db);
        seed(&db);

        let conn = db.lock().unwrap();
        // Create a thread so we have a valid thread_id
        conn.execute(
            "INSERT INTO threads (board_id, author_id, title) VALUES (1, 0, 'Test thread')",
            [],
        )
        .unwrap();

        index_search(&conn, "hello world", "thread", 1, 0);
        index_search(&conn, "post body text", "post", 1, 1);

        // Verify entries were indexed (exactly 2 from our explicit calls)
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM search_map", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 2);
    }
}
