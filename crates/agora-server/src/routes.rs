use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Json, Response},
};
use rusqlite::params;

use tracing::{error, info};

use crate::auth::AuthUser;
use crate::db;
use crate::models::{ErrorBody, PaginationParams, SearchParams};
use crate::AppState;
use agora_common::*;

const THREADS_PER_PAGE: i64 = 20;
const POSTS_PER_PAGE: i64 = 50;
const SEARCH_PER_PAGE: i64 = 20;
const DM_PER_PAGE: i64 = 50;
const MENTIONS_PER_PAGE: i64 = 20;
const MAX_PAGE: i64 = 10_000;
const MAX_UNUSED_INVITES: i64 = 5;
const MAX_SEARCH_QUERY_LEN: usize = 200;
const MAX_ATTACHMENTS_PER_POST: i64 = 10;
const MAX_FILENAME_LEN: usize = 255;
const ONLINE_THRESHOLD_MINUTES: i64 = 5;

const ROLE_ADMIN: &str = "admin";
const ROLE_MOD: &str = "mod";
const ROLE_MEMBER: &str = "member";

fn err(status: StatusCode, msg: impl Into<String>) -> Response {
    (status, Json(ErrorBody::new(msg))).into_response()
}

/// Unwrap a database Result, returning HTTP 500 on error.
macro_rules! db_try {
    ($expr:expr) => {
        match $expr {
            Ok(v) => v,
            Err(e) => {
                error!("Database error: {}", e);
                return err(StatusCode::INTERNAL_SERVER_ERROR, "Internal server error");
            }
        }
    };
}

/// Calculate pagination: returns (clamped_page, total_pages, offset).
fn paginate(total: i64, page_size: i64, page: i64) -> (i64, i64, i64) {
    let total_pages = ((total as f64) / (page_size as f64)).ceil() as i64;
    let total_pages = total_pages.max(1);
    let page = page.min(total_pages);
    let offset = (page - 1) * page_size;
    (page, total_pages, offset)
}

// --- Public endpoints ---

pub async fn landing() -> impl IntoResponse {
    let addr = std::env::var("AGORA_URL").unwrap_or_else(|_| "<server-address>".to_string());
    let body = format!(
        "AGORA\n\n\
         This is a private, invite-only forum.\n\n\
         To join, you need the Agora client and an invite code from a member.\n\n\
         Download the client for your platform:\n\n\
         \x20 Linux (x86_64):\n\
         \x20   torsocks curl -o agora http://{addr}/download/agora-linux-x86_64\n\
         \x20   chmod +x agora\n\n\
         \x20 Linux (ARM64):\n\
         \x20   torsocks curl -o agora http://{addr}/download/agora-linux-aarch64\n\
         \x20   chmod +x agora\n\n\
         \x20 macOS (Apple Silicon):\n\
         \x20   torsocks curl -o agora http://{addr}/download/agora-macos-aarch64\n\
         \x20   chmod +x agora\n\n\
         Then run:\n\
         \x20   ./agora setup\n\n\
         You will be asked for this server's address and your invite code.\n"
    );
    (StatusCode::OK, [("content-type", "text/plain")], body)
}

pub async fn version() -> impl IntoResponse {
    let server_name = std::env::var("AGORA_NAME").ok();
    (
        StatusCode::OK,
        Json(VersionResponse {
            server_version: db::SERVER_VERSION.to_string(),
            min_client_version: "0.1.0".to_string(),
            server_name,
        }),
    )
}

pub async fn register(
    State(state): State<AppState>,
    Json(req): Json<RegisterRequest>,
) -> Response {
    // Global rate limit on registration
    {
        let mut limiter = state.rate_limiter.lock().await;
        if let Err(msg) = limiter.check_register() {
            return err(StatusCode::TOO_MANY_REQUESTS, msg);
        }
    }

    let db = &state.db;
    // Validate username
    let username = req.username.trim().to_lowercase();
    if username.len() < MIN_USERNAME_LEN || username.len() > MAX_USERNAME_LEN {
        return err(StatusCode::BAD_REQUEST, format!("Username must be {}-{} characters", MIN_USERNAME_LEN, MAX_USERNAME_LEN));
    }
    if !username.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
        return err(
            StatusCode::BAD_REQUEST,
            "Username may only contain alphanumeric characters and underscores",
        );
    }
    if username.starts_with('_') {
        return err(StatusCode::BAD_REQUEST, "Username may not start with an underscore");
    }

    // Validate public key is valid base64 and correct length (32 bytes for ed25519)
    match base64::Engine::decode(&base64::engine::general_purpose::STANDARD, &req.public_key) {
        Ok(bytes) if bytes.len() == 32 => {}
        Ok(_) => return err(StatusCode::BAD_REQUEST, "Invalid public key length"),
        Err(_) => return err(StatusCode::BAD_REQUEST, "Invalid public key encoding"),
    }

    let conn = db.lock().unwrap_or_else(|e| e.into_inner());

    // Check invite code
    let invite = conn.query_row(
        "SELECT id, created_by, used_by FROM invite_codes WHERE code = ?1",
        [&req.invite_code],
        |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, Option<i64>>(2)?,
            ))
        },
    );

    let (invite_id, invited_by, used_by) = match invite {
        Ok(i) => i,
        Err(_) => return err(StatusCode::FORBIDDEN, "Invalid invite code"),
    };

    if used_by.is_some() {
        return err(StatusCode::FORBIDDEN, "Invite code already used");
    }

    // Check username uniqueness
    let exists: bool = conn
        .query_row(
            "SELECT COUNT(*) > 0 FROM users WHERE LOWER(username) = LOWER(?1)",
            [&username],
            |row| row.get(0),
        )
        .unwrap_or(false);
    if exists {
        return err(StatusCode::CONFLICT, "Username already taken");
    }

    // Check public key uniqueness
    let key_exists: bool = conn
        .query_row(
            "SELECT COUNT(*) > 0 FROM users WHERE public_key = ?1",
            [&req.public_key],
            |row| row.get(0),
        )
        .unwrap_or(false);
    if key_exists {
        return err(StatusCode::CONFLICT, "Public key already registered");
    }

    // First real user (invited by system) gets admin role
    let is_first_user = invited_by == 0;
    let role = if is_first_user { ROLE_ADMIN } else { ROLE_MEMBER };

    // Create user
    let invited_by_user = if invited_by == 0 { None } else { Some(invited_by) };
    db_try!(conn.execute(
        "INSERT INTO users (username, public_key, invited_by, role) VALUES (?1, ?2, ?3, ?4)",
        params![&username, &req.public_key, invited_by_user, role],
    ));

    let user_id = conn.last_insert_rowid();

    // Mark invite as used
    db_try!(conn.execute(
        "UPDATE invite_codes SET used_by = ?1, used_at = datetime('now') WHERE id = ?2",
        params![user_id, invite_id],
    ));

    info!(user = %username, role = %role, "User registered");

    (
        StatusCode::CREATED,
        Json(RegisterResponse {
            user_id,
            username,
        }),
    )
        .into_response()
}

pub async fn download(Path(filename): Path<String>) -> Response {
    // Reject obviously malicious filenames
    if filename.contains('\0') || filename.contains('/') || filename.contains('\\') || filename.contains("..") {
        return err(StatusCode::BAD_REQUEST, "Invalid filename");
    }

    let static_dir = match std::fs::canonicalize("static") {
        Ok(p) => p,
        Err(_) => return err(StatusCode::NOT_FOUND, "File not found"),
    };
    let requested = static_dir.join(&filename);
    let resolved = match std::fs::canonicalize(&requested) {
        Ok(p) => p,
        Err(_) => return err(StatusCode::NOT_FOUND, "File not found"),
    };

    // Ensure resolved path is inside the static directory
    if !resolved.starts_with(&static_dir) {
        return err(StatusCode::BAD_REQUEST, "Invalid filename");
    }

    match tokio::fs::read(&resolved).await {
        Ok(data) => (
            StatusCode::OK,
            [("content-type", "application/octet-stream")],
            data,
        )
            .into_response(),
        Err(_) => err(StatusCode::NOT_FOUND, "File not found"),
    }
}

// --- Authenticated endpoints ---

pub async fn list_boards(
    State(state): State<AppState>,
    _user: axum::Extension<AuthUser>,
) -> Response {
    let conn = state.db.lock().unwrap_or_else(|e| e.into_inner());

    let mut stmt = db_try!(conn
        .prepare(
            "SELECT b.id, b.slug, b.name, b.description,
                    (SELECT COUNT(*) FROM threads WHERE board_id = b.id) as thread_count,
                    (SELECT MAX(last_post_at) FROM threads WHERE board_id = b.id) as last_post_at
             FROM boards b ORDER BY b.sort_order, b.name",
        ));

    let boards: Vec<Board> = db_try!(stmt
        .query_map([], |row| {
            Ok(Board {
                id: row.get(0)?,
                slug: row.get(1)?,
                name: row.get(2)?,
                description: row.get(3)?,
                thread_count: row.get(4)?,
                last_post_at: row.get(5)?,
            })
        }))
        .filter_map(|r| r.ok())
        .collect();

    (StatusCode::OK, Json(BoardListResponse { boards })).into_response()
}

pub async fn list_threads(
    State(state): State<AppState>,
    _user: axum::Extension<AuthUser>,
    Path(slug): Path<String>,
    Query(params): Query<PaginationParams>,
) -> Response {
    let conn = state.db.lock().unwrap_or_else(|e| e.into_inner());
    let page = params.page.unwrap_or(1).max(1).min(MAX_PAGE);

    // Find board
    let board = conn.query_row(
        "SELECT id, slug, name, description FROM boards WHERE slug = ?1",
        [&slug],
        |row| {
            Ok(BoardInfo {
                id: row.get(0)?,
                slug: row.get(1)?,
                name: row.get(2)?,
                description: row.get(3)?,
            })
        },
    );

    let board = match board {
        Ok(b) => b,
        Err(_) => return err(StatusCode::NOT_FOUND, "Board not found"),
    };

    // Count total threads
    let total: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM threads WHERE board_id = ?1",
            [board.id],
            |row| row.get(0),
        )
        .unwrap_or(0);

    let (page, total_pages, offset) = paginate(total, THREADS_PER_PAGE, page);

    let mut stmt = db_try!(conn
        .prepare(
            "SELECT t.id, t.title, u.username, t.created_at, t.last_post_at,
                    (SELECT COUNT(*) FROM posts WHERE thread_id = t.id AND is_deleted = 0) as post_count,
                    COALESCE(t.pinned, 0), t.locked_at
             FROM threads t
             JOIN users u ON t.author_id = u.id
             WHERE t.board_id = ?1
             ORDER BY COALESCE(t.pinned, 0) DESC, t.last_post_at DESC
             LIMIT ?2 OFFSET ?3",
        ));

    let threads: Vec<ThreadSummary> = db_try!(stmt
        .query_map(params![board.id, THREADS_PER_PAGE, offset], |row| {
            let pinned: i64 = row.get(6)?;
            let locked_at: Option<String> = row.get(7)?;
            Ok(ThreadSummary {
                id: row.get(0)?,
                title: row.get(1)?,
                author: row.get(2)?,
                created_at: row.get(3)?,
                last_post_at: row.get(4)?,
                post_count: row.get(5)?,
                pinned: pinned != 0,
                locked: locked_at.is_some(),
            })
        }))
        .filter_map(|r| r.ok())
        .collect();

    (
        StatusCode::OK,
        Json(ThreadListResponse {
            board,
            threads,
            page,
            total_pages,
        }),
    )
        .into_response()
}

pub async fn get_thread(
    State(state): State<AppState>,
    _user: axum::Extension<AuthUser>,
    Path(thread_id): Path<i64>,
    Query(params): Query<PaginationParams>,
) -> Response {
    let conn = state.db.lock().unwrap_or_else(|e| e.into_inner());
    let page = params.page.unwrap_or(1).max(1).min(MAX_PAGE);

    // Get thread with board info
    let thread = conn.query_row(
        "SELECT t.id, t.board_id, b.slug, t.title, u.username, t.created_at,
                COALESCE(t.pinned, 0), t.locked_at
         FROM threads t
         JOIN users u ON t.author_id = u.id
         JOIN boards b ON t.board_id = b.id
         WHERE t.id = ?1",
        [thread_id],
        |row| {
            let pinned: i64 = row.get(6)?;
            let locked_at: Option<String> = row.get(7)?;
            Ok(ThreadDetail {
                id: row.get(0)?,
                board_id: row.get(1)?,
                board_slug: row.get(2)?,
                title: row.get(3)?,
                author: row.get(4)?,
                created_at: row.get(5)?,
                pinned: pinned != 0,
                locked: locked_at.is_some(),
            })
        },
    );

    let thread = match thread {
        Ok(t) => t,
        Err(_) => return err(StatusCode::NOT_FOUND, "Thread not found"),
    };

    // Count total posts (including deleted — they show as [deleted])
    let total: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM posts WHERE thread_id = ?1",
            [thread_id],
            |row| row.get(0),
        )
        .unwrap_or(0);

    let (page, total_pages, offset) = paginate(total, POSTS_PER_PAGE, page);

    let mut stmt = db_try!(conn
        .prepare(
            "SELECT p.id, p.post_number, u.username, p.body, p.created_at,
                    p.edited_at, COALESCE(p.is_deleted, 0), p.parent_post_id
             FROM (
                 SELECT id, thread_id, author_id, body, created_at, edited_at, is_deleted,
                        parent_post_id,
                        ROW_NUMBER() OVER (ORDER BY created_at ASC, id ASC) as post_number
                 FROM posts
                 WHERE thread_id = ?1
             ) p
             JOIN users u ON p.author_id = u.id
             ORDER BY p.post_number ASC
             LIMIT ?2 OFFSET ?3",
        ));

    let mut posts: Vec<Post> = db_try!(stmt
        .query_map(params![thread_id, POSTS_PER_PAGE, offset], |row| {
            let is_deleted: i64 = row.get(6)?;
            Ok(Post {
                id: row.get(0)?,
                post_number: row.get(1)?,
                author: row.get(2)?,
                body: if is_deleted != 0 { "[deleted]".to_string() } else { row.get(3)? },
                created_at: row.get(4)?,
                edited_at: row.get(5)?,
                is_deleted: is_deleted != 0,
                attachments: Vec::new(),
                parent_post_id: row.get(7)?,
                parent_post_number: None,
                parent_author: None,
                reactions: Vec::new(),
            })
        }))
        .filter_map(|r| r.ok())
        .collect();

    // Resolve parent_post_number and parent_author
    // Build lookup from post id -> (post_number, author) for posts on this page
    let post_lookup: std::collections::HashMap<i64, (i64, String)> = posts
        .iter()
        .map(|p| (p.id, (p.post_number, p.author.clone())))
        .collect();

    for post in &mut posts {
        if let Some(parent_id) = post.parent_post_id {
            if let Some((num, author)) = post_lookup.get(&parent_id) {
                post.parent_post_number = Some(*num);
                post.parent_author = Some(author.clone());
            } else {
                // Parent not on this page — query DB using same ordering as ROW_NUMBER
                let parent_info = conn.query_row(
                    "SELECT
                        (SELECT COUNT(*) FROM posts pp
                         WHERE pp.thread_id = ?1
                         AND (pp.created_at < p.created_at
                              OR (pp.created_at = p.created_at AND pp.id <= p.id))
                        ) as pn,
                        (SELECT username FROM users WHERE id = p.author_id)
                     FROM posts p WHERE p.id = ?2",
                    params![thread_id, parent_id],
                    |row| Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?)),
                );
                if let Ok((num, author)) = parent_info {
                    post.parent_post_number = Some(num);
                    post.parent_author = Some(author);
                }
            }
        }
    }

    // Load attachments for each post
    for post in &mut posts {
        let mut att_stmt = db_try!(conn
            .prepare(
                "SELECT id, filename, content_type, size_bytes FROM attachments WHERE post_id = ?1",
            ));
        post.attachments = db_try!(att_stmt
            .query_map([post.id], |row| {
                Ok(AttachmentInfo {
                    id: row.get(0)?,
                    filename: row.get(1)?,
                    content_type: row.get(2)?,
                    size_bytes: row.get(3)?,
                })
            }))
            .filter_map(|r| r.ok())
            .collect();
    }

    // Load reactions for each post
    let post_ids: Vec<i64> = posts.iter().map(|p| p.id).collect();
    if !post_ids.is_empty() {
        // Use numbered placeholders: ?1 = user_id, ?2..?N+1 = post_ids
        let placeholders: String = (0..post_ids.len())
            .map(|i| format!("?{}", i + 2))
            .collect::<Vec<_>>()
            .join(",");
        let query = format!(
            "SELECT post_id, reaction, COUNT(*) as cnt,
                    SUM(CASE WHEN user_id = ?1 THEN 1 ELSE 0 END) as my
             FROM reactions WHERE post_id IN ({}) GROUP BY post_id, reaction",
            placeholders
        );
        let mut reaction_stmt = db_try!(conn.prepare(&query));
        let mut param_values: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();
        param_values.push(Box::new(_user.user_id));
        for pid in &post_ids {
            param_values.push(Box::new(*pid));
        }
        let param_refs: Vec<&dyn rusqlite::types::ToSql> =
            param_values.iter().map(|p| p.as_ref()).collect();
        let reaction_rows: Vec<(i64, String, i64, i64)> = db_try!(reaction_stmt
            .query_map(param_refs.as_slice(), |row| {
                Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))
            }))
            .filter_map(|r| r.ok())
            .collect();

        for (post_id, reaction, count, my) in reaction_rows {
            if let Some(post) = posts.iter_mut().find(|p| p.id == post_id) {
                post.reactions.push(ReactionCount {
                    reaction,
                    count,
                    reacted_by_me: my > 0,
                });
            }
        }
    }

    (
        StatusCode::OK,
        Json(ThreadViewResponse {
            thread,
            posts,
            page,
            total_pages,
        }),
    )
        .into_response()
}

pub async fn create_thread(
    State(state): State<AppState>,
    user: axum::Extension<AuthUser>,
    Path(slug): Path<String>,
    Json(req): Json<CreateThreadRequest>,
) -> Response {
    let title = req.title.trim().to_string();
    let body = req.body.trim().to_string();

    if title.is_empty() || title.len() > MAX_TITLE_LEN {
        return err(
            StatusCode::BAD_REQUEST,
            format!("Title must be 1-{} characters", MAX_TITLE_LEN),
        );
    }
    if body.is_empty() || body.len() > MAX_BODY_LEN {
        return err(
            StatusCode::BAD_REQUEST,
            format!("Body must be 1-{} characters", MAX_BODY_LEN),
        );
    }

    let conn = state.db.lock().unwrap_or_else(|e| e.into_inner());

    // Find board
    let board_id: Result<i64, _> = conn.query_row(
        "SELECT id FROM boards WHERE slug = ?1",
        [&slug],
        |row| row.get(0),
    );
    let board_id = match board_id {
        Ok(id) => id,
        Err(_) => return err(StatusCode::NOT_FOUND, "Board not found"),
    };

    if let Err(e) = conn.execute_batch("BEGIN") {
        return err(StatusCode::INTERNAL_SERVER_ERROR, format!("Transaction start failed: {}", e));
    }

    let result = (|| -> Result<(i64, i64), rusqlite::Error> {
        conn.execute(
            "INSERT INTO threads (board_id, author_id, title) VALUES (?1, ?2, ?3)",
            params![board_id, user.user_id, &title],
        )?;
        let thread_id = conn.last_insert_rowid();

        conn.execute(
            "INSERT INTO posts (thread_id, author_id, body) VALUES (?1, ?2, ?3)",
            params![thread_id, user.user_id, &body],
        )?;
        let post_id = conn.last_insert_rowid();

        // Index in FTS
        db::index_search(&conn, &title, "thread", thread_id, 0);
        db::index_search(&conn, &body, "post", thread_id, post_id);

        conn.execute_batch("COMMIT")?;
        Ok((thread_id, post_id))
    })();

    let (thread_id, post_id) = match result {
        Ok(ids) => ids,
        Err(e) => {
            conn.execute_batch("ROLLBACK").ok();
            error!("Failed to create thread: {}", e);
            return err(StatusCode::INTERNAL_SERVER_ERROR, "Internal server error");
        }
    };

    (
        StatusCode::CREATED,
        Json(CreateThreadResponse {
            thread_id,
            post_id,
        }),
    )
        .into_response()
}

pub async fn create_post(
    State(state): State<AppState>,
    user: axum::Extension<AuthUser>,
    Path(thread_id): Path<i64>,
    Json(req): Json<CreatePostRequest>,
) -> Response {
    let body = req.body.trim().to_string();

    if body.is_empty() || body.len() > MAX_BODY_LEN {
        return err(
            StatusCode::BAD_REQUEST,
            format!("Body must be 1-{} characters", MAX_BODY_LEN),
        );
    }

    let conn = state.db.lock().unwrap_or_else(|e| e.into_inner());


    // Verify thread exists and is not locked
    let thread_info = conn.query_row(
        "SELECT id, locked_at FROM threads WHERE id = ?1",
        [thread_id],
        |row| Ok((row.get::<_, i64>(0)?, row.get::<_, Option<String>>(1)?)),
    );

    match thread_info {
        Ok((_, Some(_))) => return err(StatusCode::FORBIDDEN, "Thread is locked"),
        Ok(_) => {}
        Err(_) => return err(StatusCode::NOT_FOUND, "Thread not found"),
    }

    // Validate parent_post_id if provided
    if let Some(parent_id) = req.parent_post_id {
        let parent_ok: bool = conn
            .query_row(
                "SELECT COUNT(*) FROM posts WHERE id = ?1 AND thread_id = ?2",
                params![parent_id, thread_id],
                |row| row.get::<_, i64>(0),
            )
            .map(|c| c > 0)
            .unwrap_or(false);
        if !parent_ok {
            return err(
                StatusCode::BAD_REQUEST,
                "parent_post_id must reference a post in the same thread",
            );
        }
    }

    db_try!(conn.execute(
        "INSERT INTO posts (thread_id, author_id, body, parent_post_id) VALUES (?1, ?2, ?3, ?4)",
        params![thread_id, user.user_id, &body, req.parent_post_id],
    ));
    let post_id = conn.last_insert_rowid();

    // Update last_post_at
    db_try!(conn.execute(
        "UPDATE threads SET last_post_at = datetime('now') WHERE id = ?1",
        [thread_id],
    ));

    // Index in FTS
    db::index_search(&conn, &body, "post", thread_id, post_id);

    // Get post_number
    let post_number: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM posts WHERE thread_id = ?1 AND id <= ?2",
            params![thread_id, post_id],
            |row| row.get(0),
        )
        .unwrap_or(1);

    (
        StatusCode::CREATED,
        Json(CreatePostResponse {
            post_id,
            post_number,
        }),
    )
        .into_response()
}

// --- Post editing ---

pub async fn edit_post(
    State(state): State<AppState>,
    user: axum::Extension<AuthUser>,
    Path((thread_id, post_id)): Path<(i64, i64)>,
    Json(req): Json<EditPostRequest>,
) -> Response {
    let new_body = req.body.trim().to_string();

    if new_body.is_empty() || new_body.len() > MAX_BODY_LEN {
        return err(StatusCode::BAD_REQUEST, format!("Body must be 1-{} characters", MAX_BODY_LEN));
    }

    let conn = state.db.lock().unwrap_or_else(|e| e.into_inner());


    // Get the post and verify ownership
    let post = conn.query_row(
        "SELECT id, author_id, body, thread_id, COALESCE(is_deleted, 0) FROM posts WHERE id = ?1 AND thread_id = ?2",
        params![post_id, thread_id],
        |row| Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?, row.get::<_, String>(2)?, row.get::<_, i64>(3)?, row.get::<_, i64>(4)?)),
    );

    let (_, author_id, old_body, _, is_deleted) = match post {
        Ok(p) => p,
        Err(_) => return err(StatusCode::NOT_FOUND, "Post not found"),
    };

    if is_deleted != 0 {
        return err(StatusCode::FORBIDDEN, "Cannot edit a deleted post");
    }

    // Only the author or a mod/admin can edit
    let user_role = get_user_role(&conn, user.user_id);
    if author_id != user.user_id && !is_mod_or_admin(&user_role) {
        return err(StatusCode::FORBIDDEN, "You can only edit your own posts");
    }

    // Save old version to edit history (with who edited)
    db_try!(conn.execute(
        "INSERT INTO post_edits (post_id, old_body, edited_by) VALUES (?1, ?2, ?3)",
        params![post_id, &old_body, &user.username],
    ));

    // Update the post
    db_try!(conn.execute(
        "UPDATE posts SET body = ?1, edited_at = datetime('now') WHERE id = ?2",
        params![&new_body, post_id],
    ));

    // Update FTS search index: remove old entry and add new one
    let old_map_id: Option<i64> = conn
        .query_row(
            "SELECT id FROM search_map WHERE kind = 'post' AND post_id = ?1",
            [post_id],
            |row| row.get(0),
        )
        .ok();
    if let Some(map_id) = old_map_id {
        conn.execute("DELETE FROM search_index WHERE rowid = ?1", [map_id]).ok();
        conn.execute("DELETE FROM search_map WHERE id = ?1", [map_id]).ok();
    }
    db::index_search(&conn, &new_body, "post", thread_id, post_id);

    // Count edits
    let edit_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM post_edits WHERE post_id = ?1",
            [post_id],
            |row| row.get(0),
        )
        .unwrap_or(0);

    (
        StatusCode::OK,
        Json(EditPostResponse { post_id, edit_count }),
    )
        .into_response()
}

pub async fn post_history(
    State(state): State<AppState>,
    _user: axum::Extension<AuthUser>,
    Path((_thread_id, post_id)): Path<(i64, i64)>,
) -> Response {
    let conn = state.db.lock().unwrap_or_else(|e| e.into_inner());

    let post_info = conn.query_row(
        "SELECT body, COALESCE(is_deleted, 0) FROM posts WHERE id = ?1",
        [post_id],
        |row| Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?)),
    );

    let (current_body, is_deleted) = match post_info {
        Ok(p) => p,
        Err(_) => return err(StatusCode::NOT_FOUND, "Post not found"),
    };

    // Don't expose content of deleted posts through history
    if is_deleted != 0 {
        return err(StatusCode::FORBIDDEN, "Post has been deleted");
    }

    let mut stmt = db_try!(conn
        .prepare("SELECT old_body, edited_at, edited_by FROM post_edits WHERE post_id = ?1 ORDER BY edited_at ASC"));

    let edits: Vec<PostEdit> = db_try!(stmt
        .query_map([post_id], |row| {
            Ok(PostEdit {
                old_body: row.get(0)?,
                edited_at: row.get(1)?,
                edited_by: row.get(2)?,
            })
        }))
        .filter_map(|r| r.ok())
        .collect();

    (
        StatusCode::OK,
        Json(PostHistoryResponse {
            post_id,
            current_body,
            edits,
        }),
    )
        .into_response()
}

// --- Moderation ---

fn get_user_role(conn: &rusqlite::Connection, user_id: i64) -> String {
    conn.query_row(
        "SELECT COALESCE(role, 'member') FROM users WHERE id = ?1",
        [user_id],
        |row| row.get::<_, String>(0),
    )
    .unwrap_or_else(|_| ROLE_MEMBER.to_string())
}

fn is_mod_or_admin(role: &str) -> bool {
    role == ROLE_MOD || role == ROLE_ADMIN
}

/// Only allow safe content types for attachment downloads.
fn sanitize_content_type(ct: &str) -> String {
    let ct_lower = ct.to_lowercase();
    let allowed = [
        "image/png", "image/jpeg", "image/gif", "image/webp",
        "application/pdf", "text/plain", "text/markdown",
        "application/json", "application/toml", "application/yaml",
        "application/zip", "application/x-tar", "application/gzip",
    ];
    if allowed.contains(&ct_lower.as_str()) {
        ct.to_string()
    } else {
        "application/octet-stream".to_string()
    }
}



pub async fn mod_thread(
    State(state): State<AppState>,
    user: axum::Extension<AuthUser>,
    Path(thread_id): Path<i64>,
    Json(req): Json<ModActionRequest>,
) -> Response {
    let conn = state.db.lock().unwrap_or_else(|e| e.into_inner());


    let role = get_user_role(&conn, user.user_id);

    if !is_mod_or_admin(&role) {
        return err(StatusCode::FORBIDDEN, "Moderator or admin role required");
    }

    // Verify thread exists
    let exists: bool = conn
        .query_row("SELECT COUNT(*) > 0 FROM threads WHERE id = ?1", [thread_id], |row| row.get(0))
        .unwrap_or(false);
    if !exists {
        return err(StatusCode::NOT_FOUND, "Thread not found");
    }

    let message = match req.action.as_str() {
        "pin" => {
            db_try!(conn.execute("UPDATE threads SET pinned = 1 WHERE id = ?1", [thread_id]));
            "Thread pinned"
        }
        "unpin" => {
            db_try!(conn.execute("UPDATE threads SET pinned = 0 WHERE id = ?1", [thread_id]));
            "Thread unpinned"
        }
        "lock" => {
            db_try!(conn.execute("UPDATE threads SET locked_at = datetime('now') WHERE id = ?1", [thread_id]));
            "Thread locked"
        }
        "unlock" => {
            db_try!(conn.execute("UPDATE threads SET locked_at = NULL WHERE id = ?1", [thread_id]));
            "Thread unlocked"
        }
        _ => return err(StatusCode::BAD_REQUEST, "Invalid action. Use: pin, unpin, lock, unlock"),
    };

    info!(action = %req.action, thread = %thread_id, by = %user.username, "Thread moderation");

    (StatusCode::OK, Json(ModActionResponse { success: true, message: message.to_string() })).into_response()
}

pub async fn mod_post(
    State(state): State<AppState>,
    user: axum::Extension<AuthUser>,
    Path((thread_id, post_id)): Path<(i64, i64)>,
    Json(req): Json<ModActionRequest>,
) -> Response {
    let conn = state.db.lock().unwrap_or_else(|e| e.into_inner());


    let role = get_user_role(&conn, user.user_id);

    if !is_mod_or_admin(&role) {
        return err(StatusCode::FORBIDDEN, "Moderator or admin role required");
    }

    // Verify post exists in thread
    let exists: bool = conn
        .query_row(
            "SELECT COUNT(*) > 0 FROM posts WHERE id = ?1 AND thread_id = ?2",
            params![post_id, thread_id],
            |row| row.get(0),
        )
        .unwrap_or(false);
    if !exists {
        return err(StatusCode::NOT_FOUND, "Post not found");
    }

    let message = match req.action.as_str() {
        "delete" => {
            db_try!(conn.execute("UPDATE posts SET is_deleted = 1 WHERE id = ?1", [post_id]));
            // Remove from search index
            let old_map_id: Option<i64> = conn
                .query_row(
                    "SELECT id FROM search_map WHERE kind = 'post' AND post_id = ?1",
                    [post_id],
                    |row| row.get(0),
                )
                .ok();
            if let Some(map_id) = old_map_id {
                conn.execute("DELETE FROM search_index WHERE rowid = ?1", [map_id]).ok();
                conn.execute("DELETE FROM search_map WHERE id = ?1", [map_id]).ok();
            }
            "Post deleted"
        }
        "restore" => {
            db_try!(conn.execute("UPDATE posts SET is_deleted = 0 WHERE id = ?1", [post_id]));
            // Re-index in search
            let body: Option<String> = conn
                .query_row("SELECT body, thread_id FROM posts WHERE id = ?1", [post_id], |row| {
                    Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
                })
                .ok()
                .map(|(b, _)| b);
            let tid: i64 = conn
                .query_row("SELECT thread_id FROM posts WHERE id = ?1", [post_id], |row| row.get(0))
                .unwrap_or(thread_id);
            if let Some(body) = body {
                db::index_search(&conn, &body, "post", tid, post_id);
            }
            "Post restored"
        }
        _ => return err(StatusCode::BAD_REQUEST, "Invalid action. Use: delete, restore"),
    };

    info!(action = %req.action, thread = %thread_id, post = %post_id, by = %user.username, "Post moderation");

    (StatusCode::OK, Json(ModActionResponse { success: true, message: message.to_string() })).into_response()
}

pub async fn mod_user(
    State(state): State<AppState>,
    user: axum::Extension<AuthUser>,
    Path(username): Path<String>,
    Json(req): Json<ModActionRequest>,
) -> Response {
    let conn = state.db.lock().unwrap_or_else(|e| e.into_inner());


    let role = get_user_role(&conn, user.user_id);

    if !is_mod_or_admin(&role) {
        return err(StatusCode::FORBIDDEN, "Moderator or admin role required");
    }

    // Look up target user
    let target = conn.query_row(
        "SELECT id, COALESCE(role, 'member') FROM users WHERE username = ?1 AND id > 0",
        [&username],
        |row| Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?)),
    );
    let (target_id, target_role) = match target {
        Ok(t) => t,
        Err(_) => return err(StatusCode::NOT_FOUND, "User not found"),
    };

    // Only admins can moderate other admins or mods
    if (target_role == ROLE_ADMIN || target_role == ROLE_MOD) && role != ROLE_ADMIN {
        return err(StatusCode::FORBIDDEN, "Only admins can moderate mods and admins");
    }

    // Prevent self-moderation (can't ban/demote yourself)
    if target_id == user.user_id {
        return err(StatusCode::BAD_REQUEST, "Cannot moderate yourself");
    }

    let message = match req.action.as_str() {
        "ban" => {
            db_try!(conn.execute("UPDATE users SET is_banned = 1 WHERE id = ?1", [target_id]));
            format!("User {} banned", username)
        }
        "unban" => {
            db_try!(conn.execute("UPDATE users SET is_banned = 0 WHERE id = ?1", [target_id]));
            format!("User {} unbanned", username)
        }
        "set_role" => {
            if role != ROLE_ADMIN {
                return err(StatusCode::FORBIDDEN, "Only admins can change roles");
            }
            let new_role = req.role.as_deref().unwrap_or(ROLE_MEMBER);
            if ![ROLE_MEMBER, ROLE_MOD, ROLE_ADMIN].contains(&new_role) {
                return err(StatusCode::BAD_REQUEST, "Invalid role. Use: member, mod, admin");
            }
            db_try!(conn.execute("UPDATE users SET role = ?1 WHERE id = ?2", params![new_role, target_id]));
            format!("User {} role set to {}", username, new_role)
        }
        _ => return err(StatusCode::BAD_REQUEST, "Invalid action. Use: ban, unban, set_role"),
    };

    info!(action = %req.action, target = %username, by = %user.username, "User moderation");

    (StatusCode::OK, Json(ModActionResponse { success: true, message })).into_response()
}

// --- Bookmarks ---

pub async fn list_bookmarks(
    State(state): State<AppState>,
    user: axum::Extension<AuthUser>,
) -> Response {
    let conn = state.db.lock().unwrap_or_else(|e| e.into_inner());

    let mut stmt = db_try!(conn
        .prepare(
            "SELECT bm.thread_id, t.title, b.slug, bm.created_at
             FROM bookmarks bm
             JOIN threads t ON bm.thread_id = t.id
             JOIN boards b ON t.board_id = b.id
             WHERE bm.user_id = ?1
             ORDER BY bm.created_at DESC",
        ));

    let bookmarks: Vec<BookmarkInfo> = db_try!(stmt
        .query_map([user.user_id], |row| {
            Ok(BookmarkInfo {
                thread_id: row.get(0)?,
                thread_title: row.get(1)?,
                board_slug: row.get(2)?,
                created_at: row.get(3)?,
            })
        }))
        .filter_map(|r| r.ok())
        .collect();

    (StatusCode::OK, Json(BookmarkListResponse { bookmarks })).into_response()
}

pub async fn toggle_bookmark(
    State(state): State<AppState>,
    user: axum::Extension<AuthUser>,
    Path(thread_id): Path<i64>,
) -> Response {
    let conn = state.db.lock().unwrap_or_else(|e| e.into_inner());


    // Verify thread exists
    let exists: bool = conn
        .query_row("SELECT COUNT(*) > 0 FROM threads WHERE id = ?1", [thread_id], |row| row.get(0))
        .unwrap_or(false);
    if !exists {
        return err(StatusCode::NOT_FOUND, "Thread not found");
    }

    // Check if already bookmarked
    let already: bool = conn
        .query_row(
            "SELECT COUNT(*) > 0 FROM bookmarks WHERE user_id = ?1 AND thread_id = ?2",
            params![user.user_id, thread_id],
            |row| row.get(0),
        )
        .unwrap_or(false);

    if already {
        db_try!(conn.execute(
            "DELETE FROM bookmarks WHERE user_id = ?1 AND thread_id = ?2",
            params![user.user_id, thread_id],
        ));
        (StatusCode::OK, Json(BookmarkToggleResponse { bookmarked: false })).into_response()
    } else {
        db_try!(conn.execute(
            "INSERT INTO bookmarks (user_id, thread_id) VALUES (?1, ?2)",
            params![user.user_id, thread_id],
        ));
        (StatusCode::OK, Json(BookmarkToggleResponse { bookmarked: true })).into_response()
    }
}

// --- Attachments ---

pub async fn upload_attachment(
    State(state): State<AppState>,
    user: axum::Extension<AuthUser>,
    Path((thread_id, post_id)): Path<(i64, i64)>,
    Json(req): Json<UploadAttachmentRequest>,
) -> Response {
    let conn = state.db.lock().unwrap_or_else(|e| e.into_inner());


    // Check thread is not locked
    let locked: bool = conn
        .query_row(
            "SELECT locked_at IS NOT NULL FROM threads WHERE id = ?1",
            [thread_id],
            |row| row.get(0),
        )
        .unwrap_or(false);
    if locked {
        return err(StatusCode::FORBIDDEN, "Thread is locked");
    }

    // Verify post exists and belongs to user (or user is mod)
    let post_info = conn.query_row(
        "SELECT author_id FROM posts WHERE id = ?1 AND thread_id = ?2",
        params![post_id, thread_id],
        |row| row.get::<_, i64>(0),
    );
    let author_id = match post_info {
        Ok(id) => id,
        Err(_) => return err(StatusCode::NOT_FOUND, "Post not found"),
    };

    let role = get_user_role(&conn, user.user_id);
    if author_id != user.user_id && !is_mod_or_admin(&role) {
        return err(StatusCode::FORBIDDEN, "You can only attach files to your own posts");
    }

    // Decode base64 data
    let engine = base64::engine::general_purpose::STANDARD;
    let data = match base64::Engine::decode(&engine, &req.data_base64) {
        Ok(d) => d,
        Err(_) => return err(StatusCode::BAD_REQUEST, "Invalid base64 data"),
    };

    if data.len() > db::MAX_ATTACHMENT_SIZE {
        return err(StatusCode::BAD_REQUEST, format!("File too large (max {} MB)", db::MAX_ATTACHMENT_SIZE / (1024 * 1024)));
    }

    // Limit attachments per post
    let att_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM attachments WHERE post_id = ?1",
            [post_id],
            |row| row.get(0),
        )
        .unwrap_or(0);
    if att_count >= MAX_ATTACHMENTS_PER_POST {
        return err(StatusCode::BAD_REQUEST, format!("Maximum {} attachments per post", MAX_ATTACHMENTS_PER_POST));
    }

    let filename: String = req.filename.trim()
        .chars()
        .filter(|c| !c.is_control() && *c != '/' && *c != '\\')
        .collect();
    if filename.is_empty() || filename.len() > MAX_FILENAME_LEN {
        return err(StatusCode::BAD_REQUEST, format!("Filename must be 1-{} characters", MAX_FILENAME_LEN));
    }

    // Sanitize content type — verify against magic bytes for image types
    let declared_ct = sanitize_content_type(&req.content_type);
    let content_type = match declared_ct.as_str() {
        "image/png" if !data.starts_with(&[0x89, 0x50, 0x4E, 0x47]) => "application/octet-stream".to_string(),
        "image/jpeg" if !data.starts_with(&[0xFF, 0xD8, 0xFF]) => "application/octet-stream".to_string(),
        "image/gif" if !data.starts_with(b"GIF8") => "application/octet-stream".to_string(),
        "image/webp" if data.len() < 12 || &data[8..12] != b"WEBP" => "application/octet-stream".to_string(),
        _ => declared_ct,
    };
    let size_bytes = data.len() as i64;

    db_try!(conn.execute(
        "INSERT INTO attachments (post_id, filename, content_type, size_bytes, data) VALUES (?1, ?2, ?3, ?4, ?5)",
        params![post_id, &filename, &content_type, size_bytes, &data],
    ));

    let attachment_id = conn.last_insert_rowid();

    (
        StatusCode::CREATED,
        Json(UploadAttachmentResponse { attachment_id, filename }),
    )
        .into_response()
}

pub async fn download_attachment(
    State(state): State<AppState>,
    _user: axum::Extension<AuthUser>,
    Path(attachment_id): Path<i64>,
) -> Response {
    let conn = state.db.lock().unwrap_or_else(|e| e.into_inner());

    let result = conn.query_row(
        "SELECT filename, content_type, data FROM attachments WHERE id = ?1",
        [attachment_id],
        |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, Vec<u8>>(2)?,
            ))
        },
    );

    match result {
        Ok((filename, content_type, data)) => {
            // Sanitize filename: strip control chars, quotes, path separators
            let safe_filename: String = filename
                .chars()
                .filter(|c| !c.is_control() && *c != '"' && *c != '/' && *c != '\\')
                .collect();
            let safe_filename = if safe_filename.is_empty() { "download".to_string() } else { safe_filename };
            let disposition = format!("attachment; filename=\"{}\"", safe_filename);
            // Force safe content types — don't serve text/html or other active content
            let safe_ct = sanitize_content_type(&content_type);
            (
                StatusCode::OK,
                [
                    ("content-type".to_string(), safe_ct),
                    ("content-disposition".to_string(), disposition),
                ],
                data,
            )
                .into_response()
        }
        Err(_) => err(StatusCode::NOT_FOUND, "Attachment not found"),
    }
}

// --- Invites ---

pub async fn list_invites(
    State(state): State<AppState>,
    user: axum::Extension<AuthUser>,
) -> Response {
    let conn = state.db.lock().unwrap_or_else(|e| e.into_inner());

    let mut stmt = db_try!(conn
        .prepare(
            "SELECT ic.code, u.username, ic.created_at
             FROM invite_codes ic
             LEFT JOIN users u ON ic.used_by = u.id
             WHERE ic.created_by = ?1
             ORDER BY ic.created_at DESC",
        ));

    let invites: Vec<InviteInfo> = db_try!(stmt
        .query_map([user.user_id], |row| {
            Ok(InviteInfo {
                code: row.get(0)?,
                used_by: row.get(1)?,
                created_at: row.get(2)?,
            })
        }))
        .filter_map(|r| r.ok())
        .collect();

    (StatusCode::OK, Json(InviteListResponse { invites })).into_response()
}

pub async fn create_invite(
    State(state): State<AppState>,
    user: axum::Extension<AuthUser>,
) -> Response {
    let conn = state.db.lock().unwrap_or_else(|e| e.into_inner());


    // Check unused invite count
    let unused: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM invite_codes WHERE created_by = ?1 AND used_by IS NULL",
            [user.user_id],
            |row| row.get(0),
        )
        .unwrap_or(0);

    if unused >= MAX_UNUSED_INVITES {
        return err(
            StatusCode::FORBIDDEN,
            format!("Maximum {} unused invite codes allowed", MAX_UNUSED_INVITES),
        );
    }

    let code = db::generate_invite_code();
    db_try!(conn.execute(
        "INSERT INTO invite_codes (code, created_by) VALUES (?1, ?2)",
        params![&code, user.user_id],
    ));

    (StatusCode::CREATED, Json(InviteCreateResponse { code })).into_response()
}

pub async fn me(
    State(state): State<AppState>,
    user: axum::Extension<AuthUser>,
) -> Response {
    let conn = state.db.lock().unwrap_or_else(|e| e.into_inner());

    let result = conn.query_row(
        "SELECT u.id, u.username, u.created_at,
                (SELECT username FROM users WHERE id = u.invited_by) as invited_by,
                COALESCE(u.role, 'member'),
                COALESCE(u.bio, '')
         FROM users u WHERE u.id = ?1",
        [user.user_id],
        |row| {
            Ok(MeResponse {
                user_id: row.get(0)?,
                username: row.get(1)?,
                created_at: row.get(2)?,
                invited_by: row.get(3)?,
                role: row.get(4)?,
                bio: row.get(5)?,
            })
        },
    );

    match result {
        Ok(resp) => (StatusCode::OK, Json(resp)).into_response(),
        Err(_) => err(StatusCode::NOT_FOUND, "User not found"),
    }
}

// --- Member list / who's online ---

pub async fn list_users(
    State(state): State<AppState>,
    _user: axum::Extension<AuthUser>,
) -> Response {
    let conn = state.db.lock().unwrap_or_else(|e| e.into_inner());

    let mut stmt = db_try!(conn
        .prepare(
            "SELECT u.id, u.username, u.created_at, u.last_seen_at,
                    (SELECT username FROM users WHERE id = u.invited_by) as invited_by,
                    (SELECT COUNT(*) FROM posts WHERE author_id = u.id) as post_count,
                    COALESCE(u.role, 'member'),
                    COALESCE(u.bio, '')
             FROM users u
             WHERE u.id > 0
             ORDER BY u.created_at ASC
             LIMIT 500",
        ));

    let users: Vec<UserInfo> = db_try!(stmt
        .query_map([], |row| {
            let last_seen: Option<String> = row.get(3)?;
            let is_online = last_seen
                .as_ref()
                .map(|ls| {
                    chrono::NaiveDateTime::parse_from_str(ls, "%Y-%m-%d %H:%M:%S")
                        .map(|dt| {
                            let now = chrono::Utc::now().naive_utc();
                            (now - dt).num_minutes() < ONLINE_THRESHOLD_MINUTES
                        })
                        .unwrap_or(false)
                })
                .unwrap_or(false);

            Ok(UserInfo {
                username: row.get(1)?,
                joined_at: row.get(2)?,
                last_seen_at: last_seen,
                invited_by: row.get(4)?,
                post_count: row.get(5)?,
                is_online,
                role: row.get(6)?,
                bio: row.get(7)?,
            })
        }))
        .filter_map(|r| r.ok())
        .collect();

    (StatusCode::OK, Json(UserListResponse { users })).into_response()
}

// --- Search ---

pub async fn search(
    State(state): State<AppState>,
    _user: axum::Extension<AuthUser>,
    Query(params): Query<SearchParams>,
) -> Response {
    let raw_query = params.q.as_deref().unwrap_or("").trim().to_string();
    let by_user = params.by.as_deref().unwrap_or("").trim().to_string();

    if raw_query.is_empty() && by_user.is_empty() {
        return err(StatusCode::BAD_REQUEST, "Search query cannot be empty");
    }

    if raw_query.len() > MAX_SEARCH_QUERY_LEN {
        return err(StatusCode::BAD_REQUEST, format!("Search query too long (max {} characters)", MAX_SEARCH_QUERY_LEN));
    }

    let page = params.page.unwrap_or(1).max(1).min(MAX_PAGE);

    let conn = state.db.lock().unwrap_or_else(|e| e.into_inner());

    // If filtering by username, resolve the user ID
    let by_user_id: Option<i64> = if !by_user.is_empty() {
        conn.query_row(
            "SELECT id FROM users WHERE LOWER(username) = LOWER(?1)",
            [&by_user],
            |row| row.get(0),
        )
        .ok()
    } else {
        None
    };

    if !by_user.is_empty() && by_user_id.is_none() {
        return (
            StatusCode::OK,
            Json(SearchResponse {
                results: vec![],
                page,
                total_pages: 1,
            }),
        )
            .into_response();
    }

    // Author-only search (no FTS query)
    if raw_query.is_empty() {
        let uid = by_user_id.expect("by_user_id guaranteed Some by prior check");

        let total: i64 = conn
            .query_row(
                "SELECT (SELECT COUNT(*) FROM threads WHERE author_id = ?1)
                      + (SELECT COUNT(*) FROM posts WHERE author_id = ?1 AND is_deleted = 0)",
                [uid],
                |row| row.get(0),
            )
            .unwrap_or(0);

        let (page, total_pages, offset) = paginate(total, SEARCH_PER_PAGE, page);

        // Return threads and posts by this user, newest first
        let mut stmt = db_try!(conn
            .prepare(
                "SELECT kind, thread_id, post_id, snippet, title, author FROM (
                    SELECT 'thread' as kind, t.id as thread_id, 0 as post_id,
                           '' as snippet, t.title, u.username as author, t.created_at as sort_date
                    FROM threads t
                    JOIN users u ON t.author_id = u.id
                    WHERE t.author_id = ?1
                    UNION ALL
                    SELECT 'post' as kind, p.thread_id, p.id as post_id,
                           SUBSTR(p.body, 1, 200) as snippet, t.title, u.username as author, p.created_at as sort_date
                    FROM posts p
                    JOIN threads t ON p.thread_id = t.id
                    JOIN users u ON p.author_id = u.id
                    WHERE p.author_id = ?1 AND p.is_deleted = 0
                ) ORDER BY sort_date DESC
                LIMIT ?2 OFFSET ?3",
            ));

        let results: Vec<SearchResult> = db_try!(stmt
            .query_map(rusqlite::params![uid, SEARCH_PER_PAGE, offset], |row| {
                Ok(SearchResult {
                    kind: row.get(0)?,
                    thread_id: row.get(1)?,
                    post_id: row.get(2)?,
                    snippet: row.get(3)?,
                    thread_title: row.get(4)?,
                    author: row.get(5)?,
                })
            }))
            .filter_map(|r| r.ok())
            .collect();

        return (
            StatusCode::OK,
            Json(SearchResponse {
                results,
                page,
                total_pages,
            }),
        )
            .into_response();
    }

    // FTS search (optionally filtered by author)

    // Sanitize FTS5 query: wrap each word in quotes to prevent FTS syntax injection
    let query: String = raw_query
        .split_whitespace()
        .map(|word| {
            let safe: String = word.chars().filter(|c| *c != '"').collect();
            format!("\"{}\"", safe)
        })
        .collect::<Vec<_>>()
        .join(" ");

    // Check if FTS table exists
    let fts_exists: bool = conn
        .query_row(
            "SELECT COUNT(*) > 0 FROM sqlite_master WHERE type='table' AND name='search_index'",
            [],
            |row| row.get(0),
        )
        .unwrap_or(false);

    if !fts_exists {
        return (
            StatusCode::OK,
            Json(SearchResponse {
                results: vec![],
                page,
                total_pages: 1,
            }),
        )
            .into_response();
    }

    if let Some(uid) = by_user_id {
        // FTS + author filter
        let total: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM search_index si
                 JOIN search_map sm ON si.rowid = sm.id
                 LEFT JOIN posts p ON sm.kind = 'post' AND sm.post_id = p.id
                 LEFT JOIN threads t ON sm.thread_id = t.id
                 WHERE search_index MATCH ?1
                   AND CASE sm.kind
                       WHEN 'thread' THEN t.author_id = ?2
                       WHEN 'post' THEN p.author_id = ?2
                       END",
                rusqlite::params![&query, uid],
                |row| row.get(0),
            )
            .unwrap_or(0);

        let (page, total_pages, offset) = paginate(total, SEARCH_PER_PAGE, page);

        let mut stmt = db_try!(conn
            .prepare(
                "SELECT sm.kind, sm.thread_id, sm.post_id,
                        snippet(search_index, 0, '>>>', '<<<', '...', 48) as snippet,
                        t.title, u.username
                 FROM search_index si
                 JOIN search_map sm ON si.rowid = sm.id
                 LEFT JOIN threads t ON sm.thread_id = t.id
                 LEFT JOIN posts p ON sm.kind = 'post' AND sm.post_id = p.id
                 LEFT JOIN users u ON t.author_id = u.id
                 WHERE search_index MATCH ?1
                   AND CASE sm.kind
                       WHEN 'thread' THEN t.author_id = ?2
                       WHEN 'post' THEN p.author_id = ?2
                       END
                 ORDER BY rank
                 LIMIT ?3 OFFSET ?4",
            ));

        let results: Vec<SearchResult> = db_try!(stmt
            .query_map(rusqlite::params![&query, uid, SEARCH_PER_PAGE, offset], |row| {
                Ok(SearchResult {
                    kind: row.get(0)?,
                    thread_id: row.get(1)?,
                    post_id: row.get(2)?,
                    snippet: row.get(3)?,
                    thread_title: row.get(4)?,
                    author: row.get(5)?,
                })
            }))
            .filter_map(|r| r.ok())
            .collect();

        return (
            StatusCode::OK,
            Json(SearchResponse {
                results,
                page,
                total_pages,
            }),
        )
            .into_response();
    }

    // FTS search without author filter
    let total: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM search_index si
             JOIN search_map sm ON si.rowid = sm.id
             WHERE search_index MATCH ?1",
            [&query],
            |row| row.get(0),
        )
        .unwrap_or(0);

    let (page, total_pages, offset) = paginate(total, SEARCH_PER_PAGE, page);

    let mut stmt = db_try!(conn
        .prepare(
            "SELECT sm.kind, sm.thread_id, sm.post_id,
                    snippet(search_index, 0, '>>>', '<<<', '...', 48) as snippet,
                    t.title, u.username
             FROM search_index si
             JOIN search_map sm ON si.rowid = sm.id
             LEFT JOIN threads t ON sm.thread_id = t.id
             LEFT JOIN users u ON t.author_id = u.id
             WHERE search_index MATCH ?1
             ORDER BY rank
             LIMIT ?2 OFFSET ?3",
        ));

    let results: Vec<SearchResult> = db_try!(stmt
        .query_map(params![&query, SEARCH_PER_PAGE, offset], |row| {
            Ok(SearchResult {
                kind: row.get(0)?,
                thread_id: row.get(1)?,
                post_id: row.get(2)?,
                snippet: row.get(3)?,
                thread_title: row.get(4)?,
                author: row.get(5)?,
            })
        }))
        .filter_map(|r| r.ok())
        .collect();

    (
        StatusCode::OK,
        Json(SearchResponse {
            results,
            page,
            total_pages,
        }),
    )
        .into_response()
}

// --- Direct Messages ---

pub async fn send_dm(
    State(state): State<AppState>,
    user: axum::Extension<AuthUser>,
    Json(req): Json<SendDmRequest>,
) -> Response {
    let conn = state.db.lock().unwrap_or_else(|e| e.into_inner());


    // Validate DM field lengths
    // XSalsa20 nonce is 24 bytes = 32 chars base64
    if req.nonce.len() > 64 {
        return err(StatusCode::BAD_REQUEST, "Invalid nonce length");
    }
    // Max DM ciphertext: 32KB base64 (~24KB plaintext)
    const MAX_CIPHERTEXT_LEN: usize = 32 * 1024;
    if req.ciphertext.len() > MAX_CIPHERTEXT_LEN {
        return err(StatusCode::BAD_REQUEST, "Message too large");
    }

    // Look up recipient (exclude system user)
    let recipient = conn.query_row(
        "SELECT id FROM users WHERE username = ?1 AND id > 0",
        [&req.recipient],
        |row| row.get::<_, i64>(0),
    );

    let recipient_id = match recipient {
        Ok(id) => id,
        Err(_) => return err(StatusCode::NOT_FOUND, "Recipient not found"),
    };

    if recipient_id == user.user_id {
        return err(StatusCode::BAD_REQUEST, "Cannot send DM to yourself");
    }

    db_try!(conn.execute(
        "INSERT INTO direct_messages (sender_id, recipient_id, ciphertext, nonce) VALUES (?1, ?2, ?3, ?4)",
        params![user.user_id, recipient_id, &req.ciphertext, &req.nonce],
    ));

    let dm_id = conn.last_insert_rowid();

    (
        StatusCode::CREATED,
        Json(SendDmResponse { dm_id }),
    )
        .into_response()
}

pub async fn dm_inbox(
    State(state): State<AppState>,
    user: axum::Extension<AuthUser>,
) -> Response {
    let conn = state.db.lock().unwrap_or_else(|e| e.into_inner());

    let mut stmt = db_try!(conn
        .prepare(
            "SELECT u.username, u.public_key, MAX(dm.created_at) as last_message_at,
                    COUNT(*) as message_count
             FROM direct_messages dm
             JOIN users u ON u.id = CASE
                 WHEN dm.sender_id = ?1 THEN dm.recipient_id
                 ELSE dm.sender_id
             END
             WHERE dm.sender_id = ?1 OR dm.recipient_id = ?1
             GROUP BY u.id
             ORDER BY last_message_at DESC",
        ));

    let conversations: Vec<DmConversationSummary> = db_try!(stmt
        .query_map([user.user_id], |row| {
            Ok(DmConversationSummary {
                username: row.get(0)?,
                public_key: row.get(1)?,
                last_message_at: row.get(2)?,
                message_count: row.get(3)?,
            })
        }))
        .filter_map(|r| r.ok())
        .collect();

    (StatusCode::OK, Json(DmInboxResponse { conversations })).into_response()
}

pub async fn dm_conversation(
    State(state): State<AppState>,
    user: axum::Extension<AuthUser>,
    Path(username): Path<String>,
    Query(params): Query<PaginationParams>,
) -> Response {
    let conn = state.db.lock().unwrap_or_else(|e| e.into_inner());
    let page = params.page.unwrap_or(1).max(1).min(MAX_PAGE);

    // Look up partner
    let partner = conn.query_row(
        "SELECT id, public_key FROM users WHERE username = ?1",
        [&username],
        |row| Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?)),
    );

    let (partner_id, partner_public_key) = match partner {
        Ok(p) => p,
        Err(_) => return err(StatusCode::NOT_FOUND, "User not found"),
    };

    // Count total
    let total: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM direct_messages
             WHERE (sender_id = ?1 AND recipient_id = ?2) OR (sender_id = ?2 AND recipient_id = ?1)",
            params![user.user_id, partner_id],
            |row| row.get(0),
        )
        .unwrap_or(0);

    let (page, total_pages, offset) = paginate(total, DM_PER_PAGE, page);

    let mut stmt = db_try!(conn
        .prepare(
            "SELECT dm.id, u.username, dm.ciphertext, dm.nonce, dm.created_at
             FROM direct_messages dm
             JOIN users u ON dm.sender_id = u.id
             WHERE (dm.sender_id = ?1 AND dm.recipient_id = ?2)
                OR (dm.sender_id = ?2 AND dm.recipient_id = ?1)
             ORDER BY dm.created_at ASC
             LIMIT ?3 OFFSET ?4",
        ));

    let messages: Vec<DmMessage> = db_try!(stmt
        .query_map(
            params![user.user_id, partner_id, DM_PER_PAGE, offset],
            |row| {
                Ok(DmMessage {
                    id: row.get(0)?,
                    sender: row.get(1)?,
                    ciphertext: row.get(2)?,
                    nonce: row.get(3)?,
                    created_at: row.get(4)?,
                })
            },
        ))
        .filter_map(|r| r.ok())
        .collect();

    (
        StatusCode::OK,
        Json(DmConversationResponse {
            partner: username,
            partner_public_key,
            messages,
            page,
            total_pages,
        }),
    )
        .into_response()
}

pub async fn get_user_public_key(
    State(state): State<AppState>,
    _user: axum::Extension<AuthUser>,
    Path(username): Path<String>,
) -> Response {
    let conn = state.db.lock().unwrap_or_else(|e| e.into_inner());

    let result = conn.query_row(
        "SELECT public_key FROM users WHERE username = ?1 AND id > 0",
        [&username],
        |row| row.get::<_, String>(0),
    );

    match result {
        Ok(public_key) => (
            StatusCode::OK,
            Json(UserPublicKeyResponse { public_key }),
        )
            .into_response(),
        Err(_) => err(StatusCode::NOT_FOUND, "User not found"),
    }
}

// --- Reactions ---

pub async fn react_post(
    State(state): State<AppState>,
    user: axum::Extension<AuthUser>,
    Path((thread_id, post_id)): Path<(i64, i64)>,
    Json(req): Json<ReactRequest>,
) -> Response {
    let conn = state.db.lock().unwrap_or_else(|e| e.into_inner());


    if !db::ALLOWED_REACTIONS.contains(&req.reaction.as_str()) {
        return err(
            StatusCode::BAD_REQUEST,
            format!(
                "Invalid reaction. Allowed: {}",
                db::ALLOWED_REACTIONS.join(", ")
            ),
        );
    }

    // Verify post exists in thread
    let exists: bool = conn
        .query_row(
            "SELECT COUNT(*) FROM posts WHERE id = ?1 AND thread_id = ?2",
            params![post_id, thread_id],
            |row| row.get::<_, i64>(0),
        )
        .map(|c| c > 0)
        .unwrap_or(false);

    if !exists {
        return err(StatusCode::NOT_FOUND, "Post not found in thread");
    }

    // Toggle: try insert, if conflict then delete
    let existing: Option<i64> = conn
        .query_row(
            "SELECT id FROM reactions WHERE post_id = ?1 AND user_id = ?2 AND reaction = ?3",
            params![post_id, user.user_id, &req.reaction],
            |row| row.get(0),
        )
        .ok();

    let added = if let Some(rid) = existing {
        db_try!(conn.execute("DELETE FROM reactions WHERE id = ?1", [rid]));
        false
    } else {
        db_try!(conn.execute(
            "INSERT INTO reactions (post_id, user_id, reaction) VALUES (?1, ?2, ?3)",
            params![post_id, user.user_id, &req.reaction],
        ));
        true
    };

    (
        StatusCode::OK,
        Json(ReactResponse {
            added,
            reaction: req.reaction,
        }),
    )
        .into_response()
}

// --- User Bio ---

pub async fn update_bio(
    State(state): State<AppState>,
    user: axum::Extension<AuthUser>,
    Json(req): Json<UpdateBioRequest>,
) -> Response {
    let bio = req.bio.trim().to_string();
    if bio.chars().count() > MAX_BIO_LEN {
        return err(StatusCode::BAD_REQUEST, format!("Bio must be {} characters or less", MAX_BIO_LEN));
    }

    let conn = state.db.lock().unwrap_or_else(|e| e.into_inner());

    db_try!(conn.execute(
        "UPDATE users SET bio = ?1 WHERE id = ?2",
        params![&bio, user.user_id],
    ));

    (StatusCode::OK, Json(UpdateBioResponse { bio })).into_response()
}

// --- Mentions ---

pub async fn get_mentions(
    State(state): State<AppState>,
    user: axum::Extension<AuthUser>,
    Query(params): Query<PaginationParams>,
) -> Response {
    let conn = state.db.lock().unwrap_or_else(|e| e.into_inner());
    let page = params.page.unwrap_or(1).max(1).min(MAX_PAGE);

    // Get the current user's username
    let username: String = match conn.query_row(
        "SELECT username FROM users WHERE id = ?1",
        [user.user_id],
        |row| row.get(0),
    ) {
        Ok(u) => u,
        Err(_) => return err(StatusCode::NOT_FOUND, "User not found"),
    };

    // Escape LIKE special characters in username
    let escaped_username = username.replace('\\', "\\\\").replace('%', "\\%").replace('_', "\\_");
    let pattern = format!("%@{}%", escaped_username);

    let total: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM posts WHERE body LIKE ?1 ESCAPE '\\' AND author_id != ?2",
            params![&pattern, user.user_id],
            |row| row.get(0),
        )
        .unwrap_or(0);

    let (page, total_pages, offset) = paginate(total, MENTIONS_PER_PAGE, page);

    let mut stmt = db_try!(conn
        .prepare(
            "SELECT p.id, p.thread_id, t.title, u.username, p.body, p.created_at
             FROM posts p
             JOIN threads t ON p.thread_id = t.id
             JOIN users u ON p.author_id = u.id
             WHERE p.body LIKE ?1 ESCAPE '\\' AND p.author_id != ?4
             ORDER BY p.created_at DESC
             LIMIT ?2 OFFSET ?3",
        ));

    let mentions: Vec<MentionResult> = db_try!(stmt
        .query_map(params![&pattern, MENTIONS_PER_PAGE, offset, user.user_id], |row| {
            let body: String = row.get(4)?;
            // UTF-8 safe truncation
            let snippet = if body.chars().count() > 100 {
                let truncated: String = body.chars().take(100).collect();
                format!("{}...", truncated)
            } else {
                body
            };
            Ok(MentionResult {
                post_id: row.get(0)?,
                thread_id: row.get(1)?,
                thread_title: row.get(2)?,
                author: row.get(3)?,
                snippet,
                created_at: row.get(5)?,
            })
        }))
        .filter_map(|r| r.ok())
        .collect();

    (
        StatusCode::OK,
        Json(MentionsResponse {
            mentions,
            page,
            total_pages,
        }),
    )
        .into_response()
}
