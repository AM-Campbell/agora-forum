use rusqlite::{params, Connection};
use std::sync::{Arc, Mutex};

use crate::config;

pub type Cache = Arc<Mutex<Connection>>;

pub fn open_for(server_addr: &str) -> Cache {
    let path = config::server_cache_path(server_addr);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).ok();
    }
    let conn = Connection::open(&path).expect("failed to open cache database");
    conn.execute_batch("PRAGMA journal_mode=WAL;").ok();

    conn.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS boards (
            id INTEGER PRIMARY KEY,
            slug TEXT NOT NULL,
            name TEXT NOT NULL,
            description TEXT NOT NULL DEFAULT '',
            sort_order INTEGER NOT NULL DEFAULT 0,
            thread_count INTEGER NOT NULL DEFAULT 0,
            last_post_at TEXT,
            last_fetched TEXT
        );

        CREATE TABLE IF NOT EXISTS threads (
            id INTEGER PRIMARY KEY,
            board_id INTEGER NOT NULL,
            author TEXT NOT NULL,
            title TEXT NOT NULL,
            created_at TEXT NOT NULL,
            last_post_at TEXT NOT NULL,
            post_count INTEGER NOT NULL DEFAULT 0,
            pinned INTEGER NOT NULL DEFAULT 0,
            locked INTEGER NOT NULL DEFAULT 0,
            last_fetched TEXT
        );

        CREATE TABLE IF NOT EXISTS posts (
            id INTEGER PRIMARY KEY,
            thread_id INTEGER NOT NULL,
            author TEXT NOT NULL,
            body TEXT NOT NULL,
            created_at TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS read_state (
            thread_id INTEGER PRIMARY KEY,
            last_read_post_id INTEGER NOT NULL,
            last_read_at TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS drafts (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            draft_type TEXT NOT NULL,
            board_slug TEXT,
            thread_id INTEGER,
            title TEXT,
            body TEXT NOT NULL,
            created_at TEXT NOT NULL DEFAULT (datetime('now'))
        );
        ",
    )
    .expect("failed to create cache tables");

    // Add columns (idempotent, for existing caches)
    conn.execute("ALTER TABLE threads ADD COLUMN pinned INTEGER NOT NULL DEFAULT 0", []).ok();
    conn.execute("ALTER TABLE threads ADD COLUMN locked INTEGER NOT NULL DEFAULT 0", []).ok();
    conn.execute("ALTER TABLE threads ADD COLUMN latest_post_id INTEGER NOT NULL DEFAULT 0", []).ok();

    Arc::new(Mutex::new(conn))
}

pub fn cache_boards(cache: &Cache, boards: &[agora_common::Board]) {
    let conn = cache.lock().expect("cache mutex poisoned");
    let now = chrono::Utc::now().to_rfc3339();
    for b in boards {
        conn.execute(
            "INSERT OR REPLACE INTO boards (id, slug, name, description, sort_order, thread_count, last_post_at, last_fetched)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![b.id, b.slug, b.name, b.description, 0, b.thread_count, b.last_post_at, now],
        ).ok();
    }
}

pub fn get_cached_boards(cache: &Cache) -> Vec<agora_common::Board> {
    let conn = cache.lock().expect("cache mutex poisoned");
    let mut stmt = conn
        .prepare("SELECT id, slug, name, description, thread_count, last_post_at FROM boards ORDER BY sort_order, name")
        .unwrap();
    stmt.query_map([], |row| {
        Ok(agora_common::Board {
            id: row.get(0)?,
            slug: row.get(1)?,
            name: row.get(2)?,
            description: row.get(3)?,
            thread_count: row.get(4)?,
            last_post_at: row.get(5)?,
        })
    })
    .unwrap()
    .filter_map(|r| r.ok())
    .collect()
}

pub fn cache_threads(cache: &Cache, board_id: i64, threads: &[agora_common::ThreadSummary]) {
    let conn = cache.lock().expect("cache mutex poisoned");
    let now = chrono::Utc::now().to_rfc3339();
    for t in threads {
        conn.execute(
            "INSERT OR REPLACE INTO threads (id, board_id, author, title, created_at, last_post_at, post_count, pinned, locked, latest_post_id, last_fetched)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
            params![t.id, board_id, t.author, t.title, t.created_at, t.last_post_at, t.post_count, t.pinned as i64, t.locked as i64, t.latest_post_id, now],
        ).ok();
    }
}

pub fn get_cached_threads(cache: &Cache, board_id: i64) -> Vec<agora_common::ThreadSummary> {
    let conn = cache.lock().expect("cache mutex poisoned");
    let mut stmt = conn
        .prepare(
            "SELECT id, title, author, created_at, last_post_at, post_count, pinned, locked, latest_post_id
             FROM threads WHERE board_id = ?1 ORDER BY pinned DESC, last_post_at DESC",
        )
        .unwrap();
    stmt.query_map([board_id], |row| {
        let pinned: i64 = row.get(6)?;
        let locked: i64 = row.get(7)?;
        Ok(agora_common::ThreadSummary {
            id: row.get(0)?,
            title: row.get(1)?,
            author: row.get(2)?,
            created_at: row.get(3)?,
            last_post_at: row.get(4)?,
            post_count: row.get(5)?,
            pinned: pinned != 0,
            locked: locked != 0,
            latest_post_id: row.get(8)?,
        })
    })
    .unwrap()
    .filter_map(|r| r.ok())
    .collect()
}

pub fn cache_posts(cache: &Cache, thread_id: i64, posts: &[agora_common::Post]) {
    let conn = cache.lock().expect("cache mutex poisoned");
    for p in posts {
        conn.execute(
            "INSERT OR REPLACE INTO posts (id, thread_id, author, body, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![p.id, thread_id, p.author, p.body, p.created_at],
        )
        .ok();
    }
}

pub fn get_cached_posts(cache: &Cache, thread_id: i64) -> Vec<agora_common::Post> {
    let conn = cache.lock().expect("cache mutex poisoned");
    let mut stmt = conn
        .prepare(
            "SELECT id, thread_id, author, body, created_at
             FROM posts WHERE thread_id = ?1 ORDER BY created_at ASC, id ASC",
        )
        .unwrap();
    let mut post_number = 0i64;
    stmt.query_map([thread_id], |row| {
        Ok((
            row.get::<_, i64>(0)?,
            row.get::<_, i64>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, String>(3)?,
            row.get::<_, String>(4)?,
        ))
    })
    .unwrap()
    .filter_map(|r| r.ok())
    .map(|(id, _thread_id, author, body, created_at)| {
        post_number += 1;
        agora_common::Post {
            id,
            post_number,
            author,
            body,
            created_at,
            edited_at: None,
            is_deleted: false,
            attachments: Vec::new(),
            parent_post_id: None,
            parent_post_number: None,
            parent_author: None,
            reactions: Vec::new(),
        }
    })
    .collect()
}

pub fn mark_thread_read(cache: &Cache, thread_id: i64, last_post_id: i64) {
    let conn = cache.lock().expect("cache mutex poisoned");
    let now = chrono::Utc::now().to_rfc3339();
    conn.execute(
        "INSERT OR REPLACE INTO read_state (thread_id, last_read_post_id, last_read_at)
         VALUES (?1, ?2, ?3)",
        params![thread_id, last_post_id, now],
    )
    .ok();
}

pub fn get_last_read_post_id(cache: &Cache, thread_id: i64) -> Option<i64> {
    let conn = cache.lock().expect("cache mutex poisoned");
    conn.query_row(
        "SELECT last_read_post_id FROM read_state WHERE thread_id = ?1",
        [thread_id],
        |row| row.get(0),
    )
    .ok()
}

pub fn get_unread_count(cache: &Cache, board_id: i64) -> i64 {
    let conn = cache.lock().expect("cache mutex poisoned");
    conn.query_row(
        "SELECT COUNT(*) FROM threads t
         WHERE t.board_id = ?1
         AND t.latest_post_id > 0
         AND (
             NOT EXISTS (SELECT 1 FROM read_state rs WHERE rs.thread_id = t.id)
             OR t.latest_post_id > (SELECT rs.last_read_post_id FROM read_state rs WHERE rs.thread_id = t.id)
         )",
        [board_id],
        |row| row.get(0),
    )
    .unwrap_or(0)
}

pub fn is_thread_unread(cache: &Cache, thread_id: i64, latest_post_id: i64) -> bool {
    if latest_post_id == 0 {
        return false; // no posts yet
    }
    let last_read = get_last_read_post_id(cache, thread_id);
    match last_read {
        None => true, // never opened
        Some(last_read_id) => latest_post_id > last_read_id,
    }
}

pub fn get_recent_reactions(cache: &Cache) -> Vec<String> {
    let conn = cache.lock().expect("cache mutex poisoned");
    conn.execute(
        "CREATE TABLE IF NOT EXISTS recent_reactions (
            shortcode TEXT PRIMARY KEY,
            used_at TEXT NOT NULL
        )",
        [],
    ).ok();
    let mut stmt = conn
        .prepare("SELECT shortcode FROM recent_reactions ORDER BY used_at DESC LIMIT 10")
        .unwrap();
    stmt.query_map([], |row| row.get(0))
        .unwrap()
        .filter_map(|r| r.ok())
        .collect()
}

pub fn record_reaction(cache: &Cache, shortcode: &str) {
    let conn = cache.lock().expect("cache mutex poisoned");
    conn.execute(
        "CREATE TABLE IF NOT EXISTS recent_reactions (
            shortcode TEXT PRIMARY KEY,
            used_at TEXT NOT NULL
        )",
        [],
    ).ok();
    let now = chrono::Utc::now().to_rfc3339();
    conn.execute(
        "INSERT OR REPLACE INTO recent_reactions (shortcode, used_at) VALUES (?1, ?2)",
        params![shortcode, now],
    ).ok();
}

pub fn clear_cache_for(server_addr: &str) -> Result<(), String> {
    let path = config::server_cache_path(server_addr);
    if path.exists() {
        std::fs::remove_file(&path).map_err(|e| format!("Failed to clear cache: {}", e))?;
    }
    Ok(())
}
