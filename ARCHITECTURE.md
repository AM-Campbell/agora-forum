# Agora — Architecture

This document describes the system as built. It is intended for contributors and anyone curious about the internals.

## Overview

Agora is a private, invite-only forum that runs over Tor. It consists of two Rust binaries — a server and a client — and a shared types crate:

```
                 Tor hidden service
Client ──────── SOCKS5 ──────────── Server
(agora)         proxy               (agora-server)
                                    │
CLI ─┐                              │
     ├── ApiClient ──► HTTP ──────► Axum + SQLite
TUI ─┘   (reqwest)    JSON         ed25519 auth
         ed25519                    rate limiting
         signing                    FTS5 search
```

The client has two modes: an interactive TUI (ratatui) for browsing and a CLI (clap) for composing and scripting. Both share the same API layer, identity, and local cache. The server is a single-process Axum application backed by SQLite.

## Workspace Layout

```
agora-forum/
├── Cargo.toml                  # Workspace root
├── install.sh                  # User install script (Tor + binary + PATH)
├── static/                     # Cross-compiled client binaries for download
├── tests/
│   └── test_e2e.py             # 93 integration tests
└── crates/
    ├── agora-common/           # Shared API types (serde structs only)
    │   └── src/lib.rs
    ├── agora-server/
    │   └── src/
    │       ├── main.rs          # Startup, route registration, security headers
    │       ├── db.rs            # Schema migrations, seeding, FTS indexing
    │       ├── auth.rs          # ed25519 signature verification middleware
    │       ├── routes.rs        # All HTTP handlers
    │       ├── rate_limit.rs    # Per-user rate limiting
    │       └── models.rs        # Internal DB row types, pagination params
    └── agora-client/
        └── src/
            ├── main.rs          # CLI parsing, --server flag, command dispatch
            ├── config.rs        # GlobalConfig + ServerConfig, per-server paths
            ├── api.rs           # HTTP client with ed25519 auth signing
            ├── identity.rs      # Keypair generation, signing, DM crypto
            ├── cache.rs         # Per-server SQLite cache
            ├── editor.rs        # $EDITOR integration, temp files, drafts
            ├── cli/
            │   ├── mod.rs       # CLI subcommand dispatch
            │   ├── setup.rs     # Interactive registration with Tor detection
            │   ├── servers.rs   # `agora servers` + `set-default`
            │   ├── boards.rs    # `agora boards`
            │   ├── threads.rs   # `agora threads <board>`
            │   ├── read.rs      # `agora read <id>` + inline image display
            │   ├── post.rs      # `agora post <board> "title"`
            │   ├── reply.rs     # `agora reply <id>`
            │   ├── edit.rs      # `agora edit <tid> <pid>`
            │   ├── invite.rs    # `agora invite` / `agora invites`
            │   ├── search.rs    # `agora search <query>`
            │   ├── members.rs   # `agora members`
            │   ├── bookmark.rs  # `agora bookmark` / `agora bookmarks`
            │   ├── dm.rs        # `agora dm` / `agora inbox`
            │   ├── attach.rs    # `agora attach` / `agora download`
            │   └── image.rs     # Kitty graphics protocol, image detection
            └── tui/
                ├── mod.rs       # TUI initialization
                ├── app.rs       # Event loop, view stack, server in header
                ├── status.rs    # Online/offline indicator
                ├── boards.rs    # Board list view
                ├── threads.rs   # Thread list view
                ├── thread.rs    # Thread view with posts
                ├── search.rs    # Search results view
                ├── bookmarks.rs # Bookmarks view
                ├── members.rs   # Member list view
                ├── invites.rs   # Invite management view
                └── messages.rs  # DM conversations view
```

## Server

### Startup

The server reads two environment variables:

| Variable | Default | Purpose |
|---|---|---|
| `AGORA_DB` | `agora.db` | SQLite database file path |
| `AGORA_BIND` | `127.0.0.1:8080` | Listen address |

On startup it opens (or creates) the SQLite database, runs migrations, seeds default data if empty, starts a rate limiter cleanup task, and listens for HTTP requests.

### Database

SQLite with WAL mode and foreign keys enabled. The schema is applied via `CREATE TABLE IF NOT EXISTS` statements in `db.rs`.

#### Core tables

**users** — registered members

| Column | Type | Notes |
|---|---|---|
| id | INTEGER PK | autoincrement |
| username | TEXT UNIQUE | 3-20 chars, alphanumeric + underscore |
| public_key | TEXT UNIQUE | base64 ed25519 public key |
| role | TEXT | `admin`, `mod`, or `member` |
| is_banned | INTEGER | 0 or 1 |
| invited_by | INTEGER FK | who invited this user |
| created_at | TEXT | UTC datetime |
| last_seen_at | TEXT | updated on every authenticated request |

**boards** — forum categories

| Column | Type | Notes |
|---|---|---|
| id | INTEGER PK | autoincrement |
| slug | TEXT UNIQUE | URL-safe identifier |
| name | TEXT | display name |
| description | TEXT | |
| sort_order | INTEGER | display ordering |

**threads**

| Column | Type | Notes |
|---|---|---|
| id | INTEGER PK | autoincrement |
| board_id | INTEGER FK | |
| author_id | INTEGER FK | |
| title | TEXT | max 200 chars |
| pinned | INTEGER | 0 or 1 |
| locked_at | TEXT | non-null = locked |
| created_at | TEXT | |
| last_post_at | TEXT | bumped on new reply |

**posts**

| Column | Type | Notes |
|---|---|---|
| id | INTEGER PK | autoincrement |
| thread_id | INTEGER FK | |
| author_id | INTEGER FK | |
| body | TEXT | max 10,000 chars |
| edited_at | TEXT | non-null = has been edited |
| is_deleted | INTEGER | soft delete by moderators |
| created_at | TEXT | |

#### Supporting tables

**invite_codes** — single-use registration codes (16 chars, alphanumeric). Max 5 unused per user.

**post_edits** — edit history. Stores the `old_body` and `edited_at` timestamp each time a post is edited.

**bookmarks** — per-user thread bookmarks with UNIQUE(user_id, thread_id).

**attachments** — file uploads stored as BLOBs. Fields: post_id, filename, content_type (allowlisted), size_bytes, data. Max 5 MB per file, max 10 per post.

**direct_messages** — encrypted DMs. Fields: sender_id, recipient_id, ciphertext, nonce. End-to-end encrypted with NaCl crypto_box.

#### Full-text search

**search_index** — FTS5 virtual table with Porter stemmer and Unicode tokenizer.

**search_map** — maps FTS rowids to content. Fields: kind (`thread` or `post`), thread_id, post_id. Updated on thread creation, post creation, post edit, and post delete/restore.

#### Indexes

```
idx_threads_board (board_id, last_post_at DESC)
idx_posts_thread (thread_id, created_at ASC)
idx_invite_codes_code (code)
idx_bookmarks_user (user_id, created_at DESC)
idx_attachments_post (post_id)
idx_post_edits_post (post_id, edited_at ASC)
idx_dm_sender (sender_id, created_at)
idx_dm_recipient (recipient_id, created_at)
```

### Seeding

On first run with an empty database, the server:

1. Creates three default boards: `general`, `meta`, `off-topic`
2. Creates a system user (id=0, username `_system`)
3. Generates a bootstrap invite code and prints it to stdout: `BOOTSTRAP INVITE CODE: <code>`
4. The first user to register with this code becomes admin

### Authentication

Every authenticated request carries three headers:

```
X-Agora-PublicKey: <base64 ed25519 public key>
X-Agora-Timestamp: <unix seconds>
X-Agora-Signature: <base64 ed25519 signature>
```

The signing string is: `{METHOD}\n{PATH}\n{TIMESTAMP}\n{BODY}` (body is empty string for GET requests). The server rejects timestamps older than 60 seconds.

The auth middleware (`auth.rs`) extracts these headers, looks up the user by public key, verifies the signature, checks the user is not banned, checks rate limits, and updates `last_seen_at`. The resulting `AuthUser { user_id, username }` is passed to handlers via Axum extractors.

Request body size limit: 8 MB (to accommodate base64-encoded file uploads).

### Rate Limiting

Per-user, tracked by user_id:

- General requests: 120 per 60 seconds
- POST requests (thread/post creation, DMs): 10 per 60 seconds

Returns HTTP 429 when exceeded. A background task cleans up expired windows every 5 minutes.

### Security Hardening

- Response headers: `X-Content-Type-Options: nosniff`, `X-Frame-Options: DENY`
- Content-Disposition filename sanitization (strip control chars, quotes, path separators)
- Content-type allowlist for attachments (images, documents, archives; everything else becomes `application/octet-stream`)
- FTS query sanitization (each word wrapped in double quotes to prevent FTS5 syntax injection)
- Soft-delete hides post content from edit history endpoint
- Self-moderation prevention (cannot ban/demote yourself)
- Ban checks on all moderation and write endpoints
- Thread lock checks on reply and attachment upload
- Transaction wrapping for multi-statement operations (thread creation)
- Static file download path sanitization (strips `..` and `/`)

### API

All responses are JSON. All timestamps are UTC ISO 8601.

#### Public endpoints

| Method | Path | Purpose |
|---|---|---|
| GET | `/` | Plain text landing page with download instructions |
| GET | `/version` | Server version (`{"server_version": "0.4.2", "min_client_version": "0.1.0", "server_name": null}`) |
| POST | `/register` | Register with username, public_key, invite_code |
| GET | `/download/:filename` | Download client binaries from `static/` directory |

#### Authenticated endpoints

| Method | Path | Purpose |
|---|---|---|
| GET | `/boards` | List all boards with thread counts |
| GET | `/boards/:slug` | List threads in board (paginated, 20/page) |
| GET | `/threads/:id` | Get thread with posts (paginated, 50/page) |
| POST | `/boards/:slug/threads` | Create thread (title + body) |
| POST | `/threads/:id/posts` | Reply to thread |
| PUT | `/threads/:tid/posts/:pid` | Edit own post |
| GET | `/threads/:tid/posts/:pid/history` | Get edit history |
| POST | `/threads/:id/mod` | Moderate thread (pin/unpin/lock/unlock) |
| POST | `/threads/:tid/posts/:pid/mod` | Moderate post (delete/restore) |
| POST | `/users/:username/mod` | Moderate user (ban/unban/set-role) |
| GET | `/bookmarks` | List bookmarked threads |
| POST | `/bookmarks/:thread_id` | Toggle bookmark |
| POST | `/threads/:tid/posts/:pid/attachments` | Upload attachment (base64 JSON) |
| GET | `/attachments/:id` | Download attachment (raw bytes) |
| GET | `/invites` | List your invite codes |
| POST | `/invites` | Generate new invite code |
| GET | `/me` | Current user profile |
| GET | `/users` | List all users with online status |
| GET | `/users/:username/key` | Get user's public key (for DM encryption) |
| GET | `/search?q=&page=` | Full-text search (paginated, 20/page) |
| GET | `/dm` | DM inbox (conversation summaries) |
| POST | `/dm` | Send encrypted DM |
| GET | `/dm/:username` | Conversation with user (paginated) |

### Roles

| Role | Thread mod | Post mod | User ban | Set role |
|---|---|---|---|---|
| admin | yes | yes | yes | yes |
| mod | yes | yes | yes | no |
| member | no | no | no | no |

The first user registered via the bootstrap invite becomes admin automatically.

## Client

### Multi-Server Identity

The client supports connecting to multiple independent servers, each with a separate identity:

```
~/.agora/
├── config.toml              # Global: socks_proxy, editor, reply_context, default_server
└── servers/
    └── <sha256[:16]>/       # First 16 chars of SHA256(server_address)
        ├── server.toml      # server URL + username
        ├── identity.key     # ed25519 keypair (base64, 64 bytes: secret + public)
        └── cache.db         # SQLite cache for this server
```

Server resolution order: `--server` CLI flag > `default_server` in config.toml.

### API Client

`api.rs` wraps reqwest with:

- Automatic SOCKS5 proxy for `.onion` addresses (skips proxy for localhost/IP addresses)
- ed25519 request signing (constructs signing string, attaches auth headers)
- Typed response deserialization via agora-common structs

### Local Cache

Per-server SQLite database caching boards, threads, posts, read state, and drafts. Enables offline reading in the TUI when the server is unreachable.

### Editor Integration

`agora post`, `agora reply`, and `agora edit` open `$EDITOR` (falls back to `$VISUAL`, then `vi`) with a temp file. For replies, the file is prepopulated with recent posts as comment lines (prefix `# `). Lines starting with `# ` are stripped before submission. Failed submissions save the draft to `~/.agora/drafts/`.

### DM Encryption

Direct messages are end-to-end encrypted:

1. Fetch recipient's ed25519 public key from `/users/:username/key`
2. Convert both parties' ed25519 keys to x25519 (via SHA-512 clamping)
3. Encrypt with NaCl `crypto_box` (XSalsa20-Poly1305)
4. Store ciphertext + nonce on server; server cannot read message content

### Image Display

Image attachments (PNG, JPEG, GIF, WebP) are displayed inline using the kitty graphics protocol in supported terminals (kitty, ghostty, wezterm). Base64-encoded image data is sent in 4096-byte chunks via escape sequences. Unsupported terminals show a download hint: `agora download <id> (image; use kitty/ghostty/wezterm for inline display)`.

### TUI

Stack-based navigation: Board List > Thread List > Thread View. Composing is always done in `$EDITOR` — the TUI suspends itself, launches the editor, submits on exit, and resumes.

The TUI shows the connected server address in the header bar. Connection state (online/offline) is displayed in the status area. Offline mode shows cached data in read-only mode.

### CLI

All CLI output is plain text, designed for piping:

```bash
agora read 42 | less
agora threads general | head -20
echo "Agreed." | agora reply 42 -f -
```

## Dependencies

### Server
axum 0.7, tokio 1, rusqlite 0.31 (bundled), serde/serde_json, ed25519-dalek 2, base64 0.22, tower-http 0.5, rand 0.8, chrono 0.4

### Client
clap 4, ratatui 0.28, crossterm 0.28, reqwest 0.12 (socks), rusqlite 0.31 (bundled), serde/serde_json/toml, ed25519-dalek 2, base64 0.22, crypto_box 0.9, x25519-dalek 2, sha2 0.10, curve25519-dalek 4, dirs 5, tempfile 3, tokio 1

### Common
serde 1 (derive only)

## Testing

93 end-to-end integration tests in `tests/test_e2e.py`. The test suite starts a real server instance, registers users, and exercises all API endpoints through the CLI binary. Tests cover: registration, boards, threads, posts, editing, moderation, bookmarks, attachments, search, invites, user listing, cross-user isolation, and authorization boundaries.

```bash
python3 tests/test_e2e.py
```
