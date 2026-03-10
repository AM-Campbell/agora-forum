# AGORA

A private, invite-only forum that runs in your terminal over the Tor network.

Someone you know runs a forum. They give you two things: a server address (a long `.onion` URL) and an invite code. You install Agora, type those in, pick a username, and you're in.

> **Recommended terminals:** [Ghostty](https://ghostty.org), [Kitty](https://sw.kovidgoyal.net/kitty/), or [WezTerm](https://wezfurlong.org/wezterm/) for the best experience (inline image rendering). Agora works in any terminal.

## Install

**If you're on macOS and don't have Homebrew, install it first: https://brew.sh**

Open your terminal and paste this:

```
curl -sSL https://raw.githubusercontent.com/AM-Campbell/agora-forum/refs/heads/master/install.sh | sh
```

This installs Tor (if needed), downloads the Agora binary, and sets up your PATH. 

Then join a forum:

```
agora setup
```

Enter the server address, invite code, and pick a username. Then run `agora` to open the forum.

## Running a Server

To host your own forum on a Linux machine (no Rust toolchain needed):

```
curl -sSL https://raw.githubusercontent.com/AM-Campbell/agora-forum/refs/heads/master/install-server.sh -o install-server.sh
sudo sh install-server.sh
```

This downloads the server binary, installs Tor, configures a hidden service, and starts everything. See **[SERVER-GUIDE.md](SERVER-GUIDE.md)** for details.

## Documentation

- **[USER-GUIDE.md](USER-GUIDE.md)** — How to use Agora: browsing, posting, DMs, search, attachments, and all features
- **[SERVER-GUIDE.md](SERVER-GUIDE.md)** — Running your own Agora server
- **[ARCHITECTURE.md](ARCHITECTURE.md)** — Technical internals for contributors

The client also has built-in documentation: run `agora guide` for in-terminal help on all commands and features.

## Quick Reference

| Key | What it does |
|---|---|
| Arrow keys or `j`/`k` | Move up and down |
| `Enter` | Open the selected item |
| `Esc` or `q` | Go back (or quit) |
| `n` | New thread or reply |
| `Tab` | Post selection mode (reply-to, react) |
| `@` | View your @mentions |
| `m` | Direct messages |
| `b` | Bookmarks |
| `/` | Search |
| `?` | Help |

## Features

- **Boards, threads, posts** — classic forum structure
- **Markdown** — bold, italic, code, links, blockquotes
- **@mentions** — mention users, see who mentioned you
- **Reactions** — thumbs up, check, heart, thinking, laugh
- **Direct messages** — end-to-end encrypted (XSalsa20)
- **File attachments** — up to 5 MB, images display inline in supported terminals
- **Search** — full-text search, filter by user
- **Bookmarks** — save threads for later
- **Multiple servers** — join multiple forums with separate identities
- **Offline mode** — everything is cached locally, works without connection
- **Moderation** — pin/lock threads, delete posts, ban users, role-based permissions
- **Profile backup** — export/import identities across devices

---

## For Developers

### Building from Source

```
git clone https://github.com/AM-Campbell/agora-forum && cd agora-forum
cargo build --release
```

Binaries: `target/release/agora-server` and `target/release/agora`

### Local Development (no Tor needed)

```
cargo run --bin agora-server    # Prints a bootstrap invite code
cargo run --bin agora -- setup  # Use http://127.0.0.1:8080 as server address
```

### Tests

```
python3 tests/test_e2e.py       # Integration tests
```
