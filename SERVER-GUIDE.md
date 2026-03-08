# Agora — Server Guide

How to set up and run your own Agora forum server.

## Quick Setup (Recommended)

The fastest way to deploy on Linux:

```bash
git clone https://github.com/AM-Campbell/agora-forum && cd agora-forum
cargo build --release
sudo ./install-server.sh
```

The install script handles everything: creates a dedicated user, installs Tor, configures the hidden service, sets up systemd, and starts the server. It will print your `.onion` address and bootstrap invite code at the end.

## Manual Setup

If you prefer to set things up yourself, or are on macOS, follow the steps below.

### Prerequisites

- **Rust toolchain** — install via [rustup](https://rustup.rs/)
- **Tor** — for exposing the server as a hidden service
- **Linux or macOS** — the server runs on both; Linux recommended for production

### Building

```bash
git clone https://github.com/AM-Campbell/agora-forum && cd agora-forum
cargo build --release
```

This produces two binaries:

- `target/release/agora-server` — the forum server
- `target/release/agora` — the client

### First Run

```bash
./agora-server
```

On first run, the server:

1. Creates the SQLite database (`agora.db` by default)
2. Runs all schema migrations
3. Creates three default boards: **general**, **meta**, **off-topic**
4. Prints a bootstrap invite code to stdout:

```
BOOTSTRAP INVITE CODE: a1b2c3d4e5f6g7h8
AGORA server listening on 127.0.0.1:8080
```

**Save this invite code.** The first user to register with it becomes the forum admin.

If you missed the code, you can retrieve it later:

```bash
agora-server invite-code
```

### Environment Variables

| Variable | Default | Description |
|---|---|---|
| `AGORA_NAME` | *(none)* | Your forum's name, shown to users in the client (e.g. "Book Club") |
| `AGORA_URL` | *(none)* | Your `.onion` address, used in the landing page download instructions |
| `AGORA_DB` | `agora.db` | Path to the SQLite database file |
| `AGORA_BIND` | `127.0.0.1:8080` | Address and port to listen on |

Example:

```bash
AGORA_NAME="Book Club" AGORA_DB=/var/lib/agora/forum.db AGORA_BIND=127.0.0.1:3000 ./agora-server
```

If `AGORA_NAME` is set, the client displays it in the header bar instead of the server address. Users see "Book Club" instead of "http://xxxxx.onion".

The server always binds to localhost — Tor handles external exposure.

### Setting Up Tor

#### Install Tor

```bash
# Debian/Ubuntu
sudo apt install tor

# Arch
sudo pacman -S tor

# macOS
brew install tor
```

#### Configure the Hidden Service

Edit your Tor config (usually `/etc/tor/torrc`):

```
HiddenServiceDir /var/lib/tor/agora/
HiddenServicePort 80 127.0.0.1:8080
```

Make sure the port matches your `AGORA_BIND` setting.

#### Start Tor

```bash
# systemd (Debian/Ubuntu/Arch)
sudo systemctl enable --now tor

# macOS
brew services start tor
```

#### Get Your .onion Address

```bash
sudo cat /var/lib/tor/agora/hostname
```

This is the address you'll give to users. It looks like: `abc123def456xyz789.onion`

### Running as a systemd Service

Create `/etc/systemd/system/agora.service`:

```ini
[Unit]
Description=Agora Forum Server
After=network.target tor.service

[Service]
Type=simple
User=agora
Group=agora
WorkingDirectory=/var/lib/agora
Environment=AGORA_NAME=My Forum
Environment=AGORA_URL=http://your-address.onion
Environment=AGORA_DB=/var/lib/agora/forum.db
Environment=AGORA_BIND=127.0.0.1:8080
ExecStart=/usr/local/bin/agora-server
Restart=on-failure
RestartSec=5

# Hardening
NoNewPrivileges=true
ProtectSystem=strict
ProtectHome=true
ReadWritePaths=/var/lib/agora
PrivateTmp=true

[Install]
WantedBy=multi-user.target
```

Set up:

```bash
# Create user and directory
sudo useradd -r -s /usr/sbin/nologin agora
sudo mkdir -p /var/lib/agora
sudo chown agora:agora /var/lib/agora

# Install binary
sudo cp target/release/agora-server /usr/local/bin/

# Enable and start
sudo systemctl enable --now agora
```

Check status:

```bash
sudo systemctl status agora
sudo journalctl -u agora -f
```

**Note:** The `ProtectHome=true` setting blocks access to `/home/`. Make sure `WorkingDirectory` and `AGORA_DB` point to `/var/lib/agora`, not a home directory.

## Registering the First User (Admin)

On a machine with Tor and the client binary:

```bash
agora setup
```

Enter:
- **Server address**: `http://your-address.onion`
- **Invite code**: the bootstrap code from first run (or `agora-server invite-code`)
- **Username**: your chosen name

This user is automatically assigned the **admin** role.

## Migrating to a New Machine

There are two ways to migrate: keeping your existing `.onion` address (recommended) or getting a new one.

### Option A: Keep the Same Address (Recommended)

Users don't need to change anything. You copy both the database and Tor's identity keys to the new machine.

**On the old machine:**

```bash
# 1. Stop the server
sudo systemctl stop agora

# 2. Back up the database
sqlite3 /var/lib/agora/forum.db ".backup /tmp/agora-backup.db"

# 3. Copy the Tor hidden service keys
sudo cp -r /var/lib/tor/agora /tmp/agora-tor-keys
sudo chown $USER /tmp/agora-tor-keys/*
```

Copy both `/tmp/agora-backup.db` and the `/tmp/agora-tor-keys/` folder to the new machine.

**On the new machine:**

```bash
# Build or download the server binary, then run the install script:
sudo ./install-server.sh
```

The install script will prompt you for:
- **Forum name** — same as before
- **Path to existing database** — point to your `agora-backup.db`
- **Path to old Tor hidden service dir** — point to your `agora-tor-keys/` folder

The script copies the Tor keys so your `.onion` address stays the same. Users connect as before with no changes needed.

### Option B: New Address

If you don't have the old Tor keys, or you want a fresh `.onion` address.

**On the old machine:**

```bash
sudo systemctl stop agora
sqlite3 /var/lib/agora/forum.db ".backup /tmp/agora-backup.db"
```

Copy `agora-backup.db` to the new machine.

**On the new machine:**

```bash
sudo ./install-server.sh
```

Provide the database path when prompted, but leave the Tor hidden service directory blank. Tor will generate a new `.onion` address.

**Tell your users** to update their client config. They can run `agora servers` to see their current server addresses, then:

```bash
agora servers update-address http://old-address.onion http://new-address.onion
```

### Notes

- The database is fully portable — all user accounts, posts, and attachments are in that single file.
- If your forum has many attachments, the database can be large (multiple GB). The `sqlite3 .backup` command handles this safely but may take a few minutes.
- Always verify the backup before decommissioning the old machine: `sqlite3 agora-backup.db "PRAGMA integrity_check;"`

## Managing Boards

Boards are created during database seeding. To customize them, use SQLite directly:

```bash
sqlite3 /var/lib/agora/forum.db
```

```sql
-- Add a new board
INSERT INTO boards (slug, name, description, sort_order)
VALUES ('books', 'Books', 'Book recommendations and discussion', 3);

-- Rename a board
UPDATE boards SET name = 'Philosophy', description = 'Philosophical inquiry' WHERE slug = 'general';

-- Reorder boards (lower sort_order = higher in list)
UPDATE boards SET sort_order = 0 WHERE slug = 'books';
UPDATE boards SET sort_order = 1 WHERE slug = 'meta';
UPDATE boards SET sort_order = 2 WHERE slug = 'general';
UPDATE boards SET sort_order = 3 WHERE slug = 'off-topic';

-- Delete an empty board (will fail if board has threads due to foreign key)
DELETE FROM boards WHERE slug = 'off-topic' AND id NOT IN (SELECT board_id FROM threads);
```

Changes take effect immediately — no restart needed (SQLite WAL mode).

## Moderation

### Roles

| Role | Thread mod | Post mod | Ban users | Set roles |
|---|---|---|---|---|
| admin | yes | yes | yes | yes |
| mod | yes | yes | yes | no |
| member | no | no | no | no |

### Promoting Users

The admin can promote users via the client:

```bash
# Make someone a moderator
agora mod set-role alice mod

# Make someone an admin
agora mod set-role alice admin
```

Or directly in the database:

```sql
UPDATE users SET role = 'mod' WHERE username = 'alice';
```

### Moderation Actions

```bash
# Pin/unpin a thread
agora mod pin 42
agora mod unpin 42

# Lock/unlock a thread (prevents new replies)
agora mod lock 42
agora mod unlock 42

# Soft-delete/restore a post
agora mod delete 42 7
agora mod restore 42 7

# Ban/unban a user
agora mod ban spammer
agora mod unban spammer
```

Soft-deleted posts show "[This post has been deleted by a moderator]" to users. The content is preserved in the database and can be restored.

## Backups

The entire forum state lives in a single SQLite file. Back it up with:

```bash
mkdir -p /var/lib/agora/backups
sqlite3 /var/lib/agora/forum.db ".backup /var/lib/agora/backups/forum-$(date +%Y%m%d).db"
```

The `.backup` command is safe to run while the server is running (it handles WAL correctly).

Automate with cron:

```bash
# Daily backup at 3am, keep last 14 days
0 3 * * * sqlite3 /var/lib/agora/forum.db ".backup /var/lib/agora/backups/forum-$(date +\%Y\%m\%d).db" && find /var/lib/agora/backups -name "forum-*.db" -mtime +14 -delete
```

For disaster recovery, also back up your Tor hidden service keys:

```bash
sudo cp -r /var/lib/tor/agora /var/lib/agora/backups/tor-keys
```

If you lose these keys, your `.onion` address changes permanently and all users must run `agora servers update-address`.

## Distributing Client Binaries

The server can host client binaries for users to download over Tor. Place cross-compiled binaries in a `static/` directory next to the server:

```bash
mkdir -p /var/lib/agora/static/

# Copy binaries from a release build
cp agora-linux-x86_64 /var/lib/agora/static/
cp agora-linux-aarch64 /var/lib/agora/static/
cp agora-macos-aarch64 /var/lib/agora/static/
```

Users can then download the client via Tor:

```bash
torsocks curl -o agora http://your-address.onion/download/agora-linux-x86_64
chmod +x agora
```

The `GET /` landing page automatically shows download instructions.

## Security Notes

- The server only binds to localhost. Tor handles all external access.
- There are no admin passwords or server-side secrets. All authentication is via client-side ed25519 signatures.
- The SQLite database contains all forum data including attachment BLOBs. Protect it accordingly.
- Rate limiting is built in: 120 requests/minute and 10 posts/minute per user.
- Invite codes are single-use. Each user can have at most 5 unused invites at a time. The invite tree is tracked (`invited_by` column) so you can trace who invited a bad actor.
- The server sets `X-Content-Type-Options: nosniff` and `X-Frame-Options: DENY` on all responses.
- Attachment content types are validated against an allowlist. Unrecognized types are served as `application/octet-stream`.

## Monitoring

The server logs to stdout. Use `journalctl` (with systemd) or redirect output to a file.

Check if the server is responding:

```bash
# From the server machine
curl http://127.0.0.1:8080/version

# From a client machine (over Tor)
agora status
```

Check database size:

```bash
ls -lh /var/lib/agora/forum.db
```

Check user count and activity:

```bash
sqlite3 /var/lib/agora/forum.db "SELECT COUNT(*) FROM users WHERE is_banned = 0;"
sqlite3 /var/lib/agora/forum.db "SELECT COUNT(*) FROM users WHERE last_seen_at > datetime('now', '-7 days');"
```

## Database Maintenance

SQLite is low-maintenance, but for large forums:

```bash
# Reclaim space after many deletions
sqlite3 /var/lib/agora/forum.db "VACUUM;"

# Check database integrity
sqlite3 /var/lib/agora/forum.db "PRAGMA integrity_check;"

# See database size breakdown
sqlite3 /var/lib/agora/forum.db "SELECT name, SUM(pgsize) FROM dbstat GROUP BY name ORDER BY 2 DESC;"
```

## Local Development (No Tor)

For testing without Tor:

```bash
# Terminal 1: Start server
cargo run --bin agora-server

# Terminal 2: Register (non-.onion addresses skip SOCKS proxy automatically)
cargo run --bin agora -- setup
# Server address: http://127.0.0.1:8080
# Invite code: (paste bootstrap code)
# Username: testuser

# Run integration tests
python3 tests/test_e2e.py
```

## Troubleshooting

**"BOOTSTRAP INVITE CODE" not printed**: The database already exists from a previous run. Retrieve existing unused codes with: `agora-server invite-code`

**"Process exited with status 200"**: The binary is for the wrong CPU architecture. Check with `uname -m` and `file agora-server`. Build from source if no matching binary exists.

**"Permission denied" with systemd**: If using `ProtectHome=true`, the `WorkingDirectory` and `AGORA_DB` must NOT be under `/home/`. Use `/var/lib/agora` instead.

**Client can't connect**: Verify Tor is running (`systemctl status tor`), the hidden service is configured, and the `.onion` address is correct. The client auto-detects SOCKS5 proxy on ports 9050 and 9150.

**"Offline" in client TUI**: The server is unreachable. Check that the server process is running, Tor is up, and the SOCKS5 proxy is accessible.

**Database locked errors**: This shouldn't happen with WAL mode, but if it does, ensure only one server process is running against the same database file.
