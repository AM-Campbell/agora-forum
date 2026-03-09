#!/usr/bin/env python3
"""
Agora dev environment — start a fresh server + configured client in one command.

Usage:
    python dev.py                  # Build, start server, register user, launch TUI
    python dev.py --no-tui         # Same but don't launch the TUI (just print info)
    python dev.py --username bob   # Use a specific username (default: devuser)
    python dev.py --port 9090      # Use a specific port (default: 9090)
    python dev.py --keep           # Keep the dev DB between runs (don't wipe)
"""

import argparse
import base64
import hashlib
import json
import os
import subprocess
import sys
import time
import urllib.request
from pathlib import Path

def generate_ed25519_keypair():
    """Generate an Ed25519 keypair using openssl. Returns (secret_32bytes, public_32bytes)."""
    # Generate private key in DER format
    priv_result = subprocess.run(
        ["openssl", "genpkey", "-algorithm", "Ed25519", "-outform", "DER"],
        capture_output=True,
    )
    if priv_result.returncode != 0:
        print("  Error: Failed to generate key with openssl.")
        print("  Make sure openssl is installed and supports Ed25519.")
        sys.exit(1)

    priv_der = priv_result.stdout
    # Ed25519 DER private key is 48 bytes; raw secret is bytes 16..48
    secret = priv_der[16:48]

    # Derive public key
    pub_result = subprocess.run(
        ["openssl", "pkey", "-inform", "DER", "-pubout", "-outform", "DER"],
        input=priv_der,
        capture_output=True,
    )
    if pub_result.returncode != 0:
        print("  Error: Failed to derive public key.")
        sys.exit(1)

    pub_der = pub_result.stdout
    # Ed25519 DER public key is 44 bytes; raw key is bytes 12..44
    public = pub_der[12:44]

    return secret, public


def server_hash(server_addr):
    """Match the Rust server_hash: first 8 bytes of SHA256, hex-encoded."""
    h = hashlib.sha256(server_addr.encode()).digest()
    return h[:8].hex()


def wait_for_server(port, timeout=30):
    """Wait for the server to be ready by polling /version."""
    url = f"http://127.0.0.1:{port}/version"
    start = time.time()
    while time.time() - start < timeout:
        try:
            resp = urllib.request.urlopen(url, timeout=2)
            if resp.status == 200:
                return json.loads(resp.read())
        except Exception:
            time.sleep(0.3)
    return None


def register_user(port, username, public_key_b64, invite_code):
    """Register a user via the API."""
    url = f"http://127.0.0.1:{port}/register"
    payload = json.dumps(
        {
            "username": username,
            "public_key": public_key_b64,
            "invite_code": invite_code,
        }
    ).encode()
    req = urllib.request.Request(
        url, data=payload, headers={"Content-Type": "application/json"}
    )
    resp = urllib.request.urlopen(req, timeout=10)
    return json.loads(resp.read())


def write_client_config(agora_home, server_addr, username, server_name):
    """Write the client config files into the isolated dev AGORA_HOME directory."""
    agora_dir = Path(agora_home)
    shash = server_hash(server_addr)
    srv_dir = agora_dir / "servers" / shash

    srv_dir.mkdir(parents=True, exist_ok=True)

    # Generate and save identity
    secret, public = generate_ed25519_keypair()
    key_bytes = secret + public
    identity_b64 = base64.b64encode(key_bytes).decode()

    identity_path = srv_dir / "identity.key"
    identity_path.write_text(identity_b64)
    identity_path.chmod(0o600)

    # Save server config
    server_toml = f'server = "{server_addr}"\nusername = "{username}"\n'
    if server_name:
        server_toml += f'server_name = "{server_name}"\n'
    (srv_dir / "server.toml").write_text(server_toml)

    # Write global config
    config_path = agora_dir / "config.toml"
    global_toml = f"""socks_proxy = "127.0.0.1:9050"
reply_context = 3
default_server = "{server_addr}"
last_server = "{server_addr}"
"""
    config_path.write_text(global_toml)

    return base64.b64encode(public).decode()


def make_png(width, height, pattern="gradient"):
    """Generate a valid PNG of given dimensions (no external deps)."""
    import struct
    import zlib

    raw_rows = b""
    for y in range(height):
        raw_rows += b"\x00"  # filter byte (none)
        for x in range(width):
            if pattern == "gradient":
                r = int(255 * x / max(width - 1, 1))
                g = int(255 * y / max(height - 1, 1))
                b = 128
            elif pattern == "checkerboard":
                on = ((x // 8) + (y // 8)) % 2 == 0
                r = g = b = 240 if on else 40
            elif pattern == "stripes":
                stripe = (y // 4) % 3
                r = 220 if stripe == 0 else 60
                g = 60 if stripe == 1 else 180
                b = 60 if stripe == 2 else 100
            else:
                r = g = b = 128
            raw_rows += struct.pack("BBBB", r, g, b, 255)

    def png_chunk(chunk_type, data):
        chunk = chunk_type + data
        return struct.pack(">I", len(data)) + chunk + struct.pack(">I", zlib.crc32(chunk) & 0xFFFFFFFF)

    ihdr = struct.pack(">IIBBBBB", width, height, 8, 6, 0, 0, 0)  # 8-bit RGBA
    return (
        b"\x89PNG\r\n\x1a\n"
        + png_chunk(b"IHDR", ihdr)
        + png_chunk(b"IDAT", zlib.compress(raw_rows))
        + png_chunk(b"IEND", b"")
    )


def seed_database(db_path, user_id):
    """Seed the dev database with sample threads, posts, and an image."""
    import sqlite3

    conn = sqlite3.connect(str(db_path))

    # Create extra users (user_id 1 is the dev user, created via API)
    extra_users = [
        ("alice", "alice_placeholder_key_1"),
        ("bob", "bob_placeholder_key_2"),
        ("charlie", "charlie_placeholder_key_3"),
    ]
    for uname, pubkey in extra_users:
        conn.execute(
            "INSERT OR IGNORE INTO users (username, public_key, invited_by, role) VALUES (?, ?, ?, 'member')",
            (uname, pubkey, user_id),
        )
    conn.commit()

    # Get user IDs
    alice_id = conn.execute("SELECT id FROM users WHERE username = 'alice'").fetchone()[0]
    bob_id = conn.execute("SELECT id FROM users WHERE username = 'bob'").fetchone()[0]
    charlie_id = conn.execute("SELECT id FROM users WHERE username = 'charlie'").fetchone()[0]

    # board IDs: 1=general, 2=meta, 3=off-topic

    # --- Thread 1: Welcome thread (general) ---
    conn.execute("INSERT INTO threads (board_id, author_id, title) VALUES (1, ?, 'Welcome to Agora Dev!')", (user_id,))
    t1 = conn.execute("SELECT last_insert_rowid()").fetchone()[0]

    posts_t1 = [
        (user_id, "Hey everyone, welcome to the dev instance! This is a test forum for development.\n\nFeel free to poke around and break things."),
        (alice_id, "Thanks for setting this up! The UI is looking great so far."),
        (bob_id, "Agreed. Quick question — does **markdown** work in posts?\n\n- bullet one\n- bullet two\n- bullet three\n\nLooks like it does!"),
        (charlie_id, "> does **markdown** work in posts?\n\nYep! And quoting too apparently."),
        (user_id, "Nice. Let me also test a longer post with some code:\n\n```rust\nfn main() {\n    println!(\"Hello from Agora!\");\n}\n```\n\nThat should render nicely."),
        (alice_id, "The code formatting looks clean. What about images? Can we attach those?"),
    ]
    post_ids_t1 = []
    for author_id, body in posts_t1:
        conn.execute("INSERT INTO posts (thread_id, author_id, body) VALUES (?, ?, ?)", (t1, author_id, body))
        pid = conn.execute("SELECT last_insert_rowid()").fetchone()[0]
        post_ids_t1.append(pid)
        # Index for search
        conn.execute("INSERT INTO search_index (text_content) VALUES (?)", (body,))
        rowid = conn.execute("SELECT last_insert_rowid()").fetchone()[0]
        conn.execute("INSERT INTO search_map (id, kind, thread_id, post_id) VALUES (?, 'post', ?, ?)", (rowid, t1, pid))

    # Add a small image attachment to alice's post about images
    small_png = make_png(64, 64, "gradient")
    conn.execute(
        "INSERT INTO attachments (post_id, filename, content_type, size_bytes, data) VALUES (?, 'small-square.png', 'image/png', ?, ?)",
        (post_ids_t1[-1], len(small_png), small_png),
    )

    # Add some reactions
    conn.execute("INSERT OR IGNORE INTO reactions (post_id, user_id, reaction) VALUES (?, ?, 'thumbsup')", (post_ids_t1[0], alice_id))
    conn.execute("INSERT OR IGNORE INTO reactions (post_id, user_id, reaction) VALUES (?, ?, 'thumbsup')", (post_ids_t1[0], bob_id))
    conn.execute("INSERT OR IGNORE INTO reactions (post_id, user_id, reaction) VALUES (?, ?, 'heart')", (post_ids_t1[0], charlie_id))
    conn.execute("INSERT OR IGNORE INTO reactions (post_id, user_id, reaction) VALUES (?, ?, 'check')", (post_ids_t1[4], alice_id))

    # --- Thread 2: Image testing (general) ---
    conn.execute("INSERT INTO threads (board_id, author_id, title) VALUES (1, ?, 'Image display testing')", (user_id,))
    t_img = conn.execute("SELECT last_insert_rowid()").fetchone()[0]

    # Various image shapes and sizes
    test_images = [
        ("square-small.png", 32, 32, "gradient", "Small square (32x32)"),
        ("square-medium.png", 128, 128, "checkerboard", "Medium square (128x128) with checkerboard pattern"),
        ("wide-banner.png", 256, 64, "stripes", "Wide banner (256x64) — tests horizontal images"),
        ("tall-portrait.png", 64, 256, "gradient", "Tall portrait (64x256) — tests vertical images"),
        ("large-square.png", 256, 256, "checkerboard", "Large square (256x256)"),
        ("tiny.png", 8, 8, "gradient", "Tiny image (8x8) — should still be visible"),
        ("ultrawide.png", 400, 40, "stripes", "Ultra-wide (400x40) — extreme aspect ratio"),
    ]

    for filename, w, h, pattern, description in test_images:
        body = f"**{description}**\n\nDimensions: {w}x{h} pixels"
        conn.execute("INSERT INTO posts (thread_id, author_id, body) VALUES (?, ?, ?)", (t_img, user_id, body))
        pid = conn.execute("SELECT last_insert_rowid()").fetchone()[0]
        conn.execute("INSERT INTO search_index (text_content) VALUES (?)", (body,))
        rowid = conn.execute("SELECT last_insert_rowid()").fetchone()[0]
        conn.execute("INSERT INTO search_map (id, kind, thread_id, post_id) VALUES (?, 'post', ?, ?)", (rowid, t_img, pid))

        png_data = make_png(w, h, pattern)
        conn.execute(
            "INSERT INTO attachments (post_id, filename, content_type, size_bytes, data) VALUES (?, ?, 'image/png', ?, ?)",
            (pid, filename, len(png_data), png_data),
        )

    # Post with multiple images of different colors
    multi_body = "**Multiple images in one post**\n\nThis post has 3 images attached to test multi-image layout."
    conn.execute("INSERT INTO posts (thread_id, author_id, body) VALUES (?, ?, ?)", (t_img, alice_id, multi_body))
    multi_pid = conn.execute("SELECT last_insert_rowid()").fetchone()[0]
    conn.execute("INSERT INTO search_index (text_content) VALUES (?)", (multi_body,))
    rowid = conn.execute("SELECT last_insert_rowid()").fetchone()[0]
    conn.execute("INSERT INTO search_map (id, kind, thread_id, post_id) VALUES (?, 'post', ?, ?)", (rowid, t_img, multi_pid))
    n_img_posts = len(test_images) + 1

    for fname, w, h, pat in [("red-gradient.png", 100, 80, "gradient"), ("blue-checker.png", 100, 80, "checkerboard"), ("green-stripes.png", 100, 80, "stripes")]:
        png_data = make_png(w, h, pat)
        conn.execute(
            "INSERT INTO attachments (post_id, filename, content_type, size_bytes, data) VALUES (?, ?, 'image/png', ?, ?)",
            (multi_pid, fname, len(png_data), png_data),
        )

    conn.execute("INSERT INTO search_index (text_content) VALUES (?)", ("Image display testing",))
    rowid = conn.execute("SELECT last_insert_rowid()").fetchone()[0]
    conn.execute("INSERT INTO search_map (id, kind, thread_id, post_id) VALUES (?, 'thread', ?, 0)", (rowid, t_img))

    # --- Thread 3: Bug reports (meta) ---
    conn.execute("INSERT INTO threads (board_id, author_id, title) VALUES (2, ?, 'Known bugs and issues')", (bob_id,))
    t2 = conn.execute("SELECT last_insert_rowid()").fetchone()[0]

    posts_t2 = [
        (bob_id, "Tracking known issues here:\n\n1. Footer wraps weirdly on narrow terminals\n2. Image spacing has too much whitespace\n3. Edit only works on last post"),
        (alice_id, "I think #1 got fixed recently. The footer wraps properly now."),
        (charlie_id, "Can confirm #1 is fixed. #2 still happening for me though."),
        (user_id, "Working on #2 and #3 now. Should have a fix soon."),
    ]
    for author_id, body in posts_t2:
        conn.execute("INSERT INTO posts (thread_id, author_id, body) VALUES (?, ?, ?)", (t2, author_id, body))
        pid = conn.execute("SELECT last_insert_rowid()").fetchone()[0]
        conn.execute("INSERT INTO search_index (text_content) VALUES (?)", (body,))
        rowid = conn.execute("SELECT last_insert_rowid()").fetchone()[0]
        conn.execute("INSERT INTO search_map (id, kind, thread_id, post_id) VALUES (?, 'post', ?, ?)", (rowid, t2, pid))

    # --- Thread 4: Random chat (off-topic) ---
    conn.execute("INSERT INTO threads (board_id, author_id, title) VALUES (3, ?, 'What are you all working on?')", (charlie_id,))
    t3 = conn.execute("SELECT last_insert_rowid()").fetchone()[0]

    posts_t3 = [
        (charlie_id, "Just curious what everyone's up to. I'm building a small CLI tool in Rust."),
        (alice_id, "I've been learning about cryptography. Ed25519 is fascinating stuff."),
        (bob_id, "Working on my homelab setup. Got a Chromebook running as a server, lol."),
        (user_id, "Building this forum obviously. It's been a fun project."),
        (charlie_id, "That's awesome. How's the Tor integration working out?"),
        (bob_id, "It works surprisingly well! The latency is noticeable but totally usable."),
    ]
    for author_id, body in posts_t3:
        conn.execute("INSERT INTO posts (thread_id, author_id, body) VALUES (?, ?, ?)", (t3, author_id, body))
        pid = conn.execute("SELECT last_insert_rowid()").fetchone()[0]
        conn.execute("INSERT INTO search_index (text_content) VALUES (?)", (body,))
        rowid = conn.execute("SELECT last_insert_rowid()").fetchone()[0]
        conn.execute("INSERT INTO search_map (id, kind, thread_id, post_id) VALUES (?, 'post', ?, ?)", (rowid, t3, pid))

    # --- Thread 5: A pinned announcement (general) ---
    conn.execute("INSERT INTO threads (board_id, author_id, title, pinned) VALUES (1, ?, 'Forum rules and guidelines', 1)", (user_id,))
    t4 = conn.execute("SELECT last_insert_rowid()").fetchone()[0]

    conn.execute("INSERT INTO posts (thread_id, author_id, body) VALUES (?, ?, ?)", (t4, user_id,
        "Welcome! A few simple rules:\n\n1. Be respectful\n2. No spam\n3. Keep things on-topic in the right boards\n4. Have fun!\n\nThis thread is pinned for reference."))
    pid = conn.execute("SELECT last_insert_rowid()").fetchone()[0]
    conn.execute("INSERT INTO search_index (text_content) VALUES (?)", ("Forum rules and guidelines",))
    rowid = conn.execute("SELECT last_insert_rowid()").fetchone()[0]
    conn.execute("INSERT INTO search_map (id, kind, thread_id, post_id) VALUES (?, 'thread', ?, 0)", (rowid, t4))

    # Index thread titles
    for tid, title in [(t1, "Welcome to Agora Dev!"), (t2, "Known bugs and issues"), (t3, "What are you all working on?")]:
        conn.execute("INSERT INTO search_index (text_content) VALUES (?)", (title,))
        rowid = conn.execute("SELECT last_insert_rowid()").fetchone()[0]
        conn.execute("INSERT INTO search_map (id, kind, thread_id, post_id) VALUES (?, 'thread', ?, 0)", (rowid, tid))

    conn.commit()
    conn.close()

    total_posts = len(posts_t1) + n_img_posts + len(posts_t2) + len(posts_t3) + 1
    n_images = len(test_images) + 1 + 3  # +1 for alice's small-square, +3 for multi-image post
    return 5, total_posts, n_images


def main():
    parser = argparse.ArgumentParser(description="Agora dev environment")
    parser.add_argument("--username", default="devuser", help="Username (default: devuser)")
    parser.add_argument("--port", type=int, default=9090, help="Server port (default: 9090)")
    parser.add_argument("--no-tui", action="store_true", help="Don't launch the TUI")
    parser.add_argument("--keep", action="store_true", help="Keep DB between runs")
    parser.add_argument("--release", action="store_true", help="Use release builds")
    args = parser.parse_args()

    project_root = Path(__file__).resolve().parent
    server_addr = f"http://127.0.0.1:{args.port}"
    db_path = project_root / "dev.db"
    agora_home = project_root / ".agora-dev"

    print()
    print("  ╔═══════════════════════════════════╗")
    print("  ║       AGORA — Dev Environment     ║")
    print("  ╚═══════════════════════════════════╝")
    print()

    # Step 1: Build
    profile = "--release" if args.release else ""
    print("  [1/5] Building...", flush=True)
    build_cmd = f"cargo build -p agora-server -p agora-client {profile}".split()
    result = subprocess.run(build_cmd, cwd=project_root)
    if result.returncode != 0:
        print("  Build failed!")
        sys.exit(1)

    build_dir = "release" if args.release else "debug"
    server_bin = project_root / "target" / build_dir / "agora-server"
    client_bin = project_root / "target" / build_dir / "agora"

    # Step 2: Clean old state (unless --keep)
    if not args.keep:
        if db_path.exists():
            db_path.unlink()
        for suffix in ["-wal", "-shm"]:
            p = db_path.with_name(db_path.name + suffix)
            if p.exists():
                p.unlink()
        # Clean dev client config too
        if agora_home.exists():
            import shutil
            shutil.rmtree(agora_home)
        print("  [2/5] Cleaned dev environment")
    else:
        print("  [2/5] Using existing database" if db_path.exists() else "  [2/5] Fresh database")

    # Step 3: Start server
    # Kill anything already on the port
    subprocess.run(["fuser", "-k", f"{args.port}/tcp"], capture_output=True)
    time.sleep(0.5)
    print(f"  [3/5] Starting server on port {args.port}...", flush=True)
    env = os.environ.copy()
    env["AGORA_DB"] = str(db_path)
    env["AGORA_BIND"] = f"127.0.0.1:{args.port}"
    env["AGORA_NAME"] = "Agora Dev"
    env["RUST_LOG"] = "agora_server=debug,tower_http=debug"

    server_proc = subprocess.Popen(
        [str(server_bin)],
        cwd=project_root,
        env=env,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        text=True,
    )

    # Capture the bootstrap invite code from server output
    invite_code = None
    start_time = time.time()

    while time.time() - start_time < 15:
        line = server_proc.stdout.readline()
        if not line:
            if server_proc.poll() is not None:
                print("  Server exited unexpectedly!")
                sys.exit(1)
            continue
        sys.stderr.write(f"  server> {line}")
        if "BOOTSTRAP INVITE CODE:" in line:
            # Strip ANSI escape codes before extracting the code
            import re
            clean = re.sub(r'\x1b\[[0-9;]*m', '', line)
            invite_code = clean.split("BOOTSTRAP INVITE CODE:")[-1].strip()
        if "listening on" in line.lower():
            break

    # Wait for server to be ready
    version_info = wait_for_server(args.port, timeout=10)
    if not version_info:
        print("  Server didn't become ready in time!")
        server_proc.terminate()
        sys.exit(1)

    server_name = version_info.get("server_name", "Agora Dev")
    print(f"        Server ready: {server_name}")

    # Step 4: Configure client and register
    if not args.keep or invite_code:
        if not invite_code:
            # Server was already seeded (--keep mode), try to get invite code
            try:
                result = subprocess.run(
                    [str(server_bin), "invite-code"],
                    cwd=project_root,
                    env=env,
                    capture_output=True,
                    text=True,
                )
                for line in result.stdout.splitlines():
                    line = line.strip()
                    if line and len(line) == 16 and line.isalnum():
                        invite_code = line
                        break
            except Exception:
                pass

        if invite_code:
            print(f"  [4/5] Registering as '{args.username}'...", flush=True)
            public_key_b64 = write_client_config(str(agora_home), server_addr, args.username, server_name)
            try:
                resp = register_user(args.port, args.username, public_key_b64, invite_code)
                print(f"        Registered: {resp['username']} (id: {resp['user_id']}, admin)")
            except urllib.error.HTTPError as e:
                body = e.read().decode()
                if "already taken" in body or "already registered" in body:
                    print(f"        User '{args.username}' already exists (--keep mode)")
                else:
                    print(f"        Registration failed: {body}")
                    server_proc.terminate()
                    sys.exit(1)
        else:
            print("  [4/5] No invite code available (existing DB)")
    else:
        print("  [4/5] Skipping registration (--keep mode)")

    # Seed database with sample content (only on fresh installs)
    if not args.keep:
        try:
            n_threads, n_posts, n_images = seed_database(db_path, 1)  # user_id 1 = first registered user
            print(f"        Seeded: {n_threads} threads, {n_posts} posts, {n_images} images")
        except Exception as e:
            print(f"        Seeding failed (non-fatal): {e}")

    print()
    print(f"  Server:  {server_addr}")
    print(f"  User:    {args.username}")
    print(f"  DB:      {db_path}")
    print(f"  Config:  {agora_home}")
    if invite_code:
        print(f"  Invite:  {invite_code}")
    print()

    if args.no_tui:
        print("  [5/5] Server running. Press Ctrl+C to stop.")
        print()
        try:
            # Forward server output
            while True:
                line = server_proc.stdout.readline()
                if not line:
                    if server_proc.poll() is not None:
                        break
                    continue
                sys.stderr.write(f"  server> {line}")
        except KeyboardInterrupt:
            print("\n  Shutting down...")
    else:
        print("  [5/5] Launching TUI...", flush=True)
        print()

        # Start a background thread to drain server output
        import threading

        def drain_server():
            try:
                while server_proc.poll() is None:
                    line = server_proc.stdout.readline()
                    if not line:
                        break
            except Exception:
                pass

        drain_thread = threading.Thread(target=drain_server, daemon=True)
        drain_thread.start()

        # Launch the TUI client with isolated config
        client_env = os.environ.copy()
        client_env["AGORA_HOME"] = str(agora_home)
        try:
            subprocess.run(
                [str(client_bin), "--server", server_addr],
                cwd=project_root,
                env=client_env,
            )
        except KeyboardInterrupt:
            pass

    # Cleanup
    server_proc.terminate()
    try:
        server_proc.wait(timeout=5)
    except subprocess.TimeoutExpired:
        server_proc.kill()

    print("  Dev environment stopped.")


if __name__ == "__main__":
    main()
