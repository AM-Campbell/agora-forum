# AGORA

A private, invite-only forum that runs in your terminal over the Tor network.

Someone you know runs a forum. They give you two things: a server address (a long `.onion` URL) and an invite code. You install Agora, type those in, pick a username, and you're in.

## Getting Started

### 1. Install

Open your terminal and paste this:

```
curl -sSL https://raw.githubusercontent.com/am-campbell/agora-forum/main/install.sh | sh
```

This does three things:
- Installs Tor if you don't have it (it will ask for your password)
- Downloads the Agora program
- Sets up your PATH so you can run `agora` from anywhere

If you're on macOS and don't have Homebrew, install it first: https://brew.sh

### 2. Join a Forum

```
agora setup
```

You'll be asked for:
- **Server address** — the `.onion` URL your friend gave you
- **Invite code** — the code your friend gave you
- **Username** — pick whatever you like (letters, numbers, underscores, 3-20 characters)

That's it. Your account is created.

### 3. Open the Forum

```
agora
```

This opens the interactive forum browser. You'll see a list of boards (topics). Use the arrow keys to move around, Enter to open things, and Esc to go back. Press `?` at any time to see all the keyboard shortcuts.

To write a post, press `n`. This opens your text editor — write your message, save, and quit the editor. Your post is submitted automatically.

## Keyboard Shortcuts

| Key | What it does |
|---|---|
| Arrow keys or `j`/`k` | Move up and down |
| `Enter` | Open the selected item |
| `Esc` or `q` | Go back (or quit) |
| `r` | Refresh |
| `n` | New thread or new reply |
| `e` | Edit your post |
| `b` | Bookmark a thread |
| `i` | Invites |
| `/` | Search |
| `?` | Help |

## Inviting Others

Once you're a member, you can invite others:

```
agora invite
```

This prints an invite code. Give it to someone along with the server address.

## Joining Multiple Servers

Agora works like Discord — you can join multiple independent servers, each with a separate identity:

```
agora setup                              # Join another server
agora servers                            # See all your servers
agora servers set-default http://...     # Switch your default
```

## Command Line

Everything you can do in the interactive browser, you can also do from the command line:

```
agora boards                             # List boards
agora threads general                    # List threads in a board
agora read 42                            # Read a thread
agora post general "My Thread Title"     # Start a new thread
agora reply 42                           # Reply to a thread
agora search "bayesian" --by alice       # Search, optionally filter by user
agora bookmark 42                        # Bookmark a thread
agora status                             # Check your connection
```

For a complete list, run `agora --help`.

## Troubleshooting

**"Tor doesn't seem to be running"**
Start Tor and try again:
```
# Linux
sudo systemctl start tor

# macOS
brew services start tor
```

**The forum says "offline"**
The server might be down, or Tor might have disconnected. Press `r` to retry. You can still read anything you've previously loaded — it's cached on your computer.

**A post failed to send**
Your text is saved automatically in `~/.agora/drafts/`. When the connection is back, resubmit it:
```
agora reply 42 -f ~/.agora/drafts/reply_42_....txt
```

**Your text editor doesn't open**
Set the `EDITOR` environment variable. Add this line to your `~/.bashrc` or `~/.zshrc`:
```
export EDITOR=nano
```
Then restart your terminal.

## More Information

- [USER-GUIDE.md](USER-GUIDE.md) — Complete user manual
- [SERVER-GUIDE.md](SERVER-GUIDE.md) — Running your own Agora server
- [ARCHITECTURE.md](ARCHITECTURE.md) — Technical internals for contributors

---

## For Developers

### Building from Source

```
git clone https://github.com/am-campbell/agora-forum && cd agora-forum
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
python3 tests/test_e2e.py       # 93 integration tests
```
