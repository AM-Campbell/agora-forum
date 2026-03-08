# Agora User Guide

## What is Agora?

Agora is a private discussion forum that runs in your terminal. There's no website — you use a small program on your computer to connect to the forum. The connection goes through the Tor network, which keeps everything private: nobody can see what you're reading or writing, and the server's location is hidden.

The forum is organized like an old-school bulletin board: there are **boards** (topics like "general", "books", "off-topic"), each board has **threads** (conversations), and each thread has a sequence of **posts**. Everything is plain text.

---

## Installing Agora

Open your terminal and paste:

```
curl -sSL https://raw.githubusercontent.com/am-campbell/agora-forum/main/install.sh | sh
```

This installs Tor (if you don't have it) and downloads the Agora program. It may ask for your password during the Tor installation step.

If you're on macOS and don't have Homebrew, install it first from https://brew.sh — the installer will tell you if it can't proceed.

---

## Joining a Forum

Someone gives you two things: a **server address** (a long URL ending in `.onion`) and an **invite code**. Then run:

```
agora setup
```

You'll see:

```
  AGORA — Join a Forum

  Checking for Tor... found (SOCKS5 proxy at 127.0.0.1:9050)

  Server address: http://xxxxx.onion
  Invite code: abc123def456ghij
  Choose a username: your_name

  Generating keypair... done
  Registering with server...

  Welcome to AGORA, your_name!
  Run 'agora' to open the forum.
```

Type the server address, paste the invite code, and pick a username. Your username must be 3-20 characters using only letters, numbers, and underscores. You can't change it later, so choose something you like.

That's it — you're registered.

### If Tor isn't found

If you see "Checking for Tor... not found", it means Tor isn't running. The message will tell you how to start it:

```
  Ubuntu/Debian:  sudo apt install tor && sudo systemctl start tor
  Arch:           sudo pacman -S tor && sudo systemctl start tor
  macOS:          brew install tor && brew services start tor
```

Start Tor, then run `agora setup` again.

---

## Browsing the Forum

Run `agora` with no arguments to open the forum:

```
agora
```

### The Home Screen

You'll see a list of boards:

```
+-- AGORA ─────────────────────────── your_name ── online ─+
|                                                            |
|  #  Board               Threads  Unread  Latest           |
|  ---------------------------------------------------      |
|  1  general                  12    (3)   2h ago           |
|  2  books                     8         5h ago           |
|  3  meta                      4         1d ago           |
|> 4  off-topic                31    (7)   3m ago           |
|                                                            |
+------------------------------------------------------------+
| [Enter] open  [r]efresh  [i]nvites  [?]help  [q]uit       |
+------------------------------------------------------------+
```

- The `>` shows which board is selected
- Numbers in parentheses under "Unread" mean you have unread posts
- "online" in the top-right means you're connected to the server

### Moving Around

| Key | What it does |
|---|---|
| Arrow keys (or `j`/`k`) | Move up and down |
| `Enter` | Open the selected item |
| `Esc` (or `q`) | Go back to the previous screen (or quit) |
| `?` | Show all keyboard shortcuts |

Press **Enter** on a board to see its threads. Press **Enter** on a thread to read it.

### Reading a Thread

```
+-- AGORA ── general ── Is Bayesian reasoning overrated? ──+
|                                                            |
|  [#1] epistemic_rat                          2h ago        |
|  ─────────────────────────────────────────────────         |
|  I've been thinking about whether the emphasis on          |
|  Bayesian reasoning in rationalist circles has become      |
|  more of a tribal marker than a useful tool.               |
|                                                            |
|  [#2] bayes_fan                              1h ago        |
|  ─────────────────────────────────────────────────         |
|  I think you're conflating two things. The framework       |
|  itself is mathematically sound.                           |
|                                                            |
+------------------------------------------------------------+
| [n]ew reply  [j/k] scroll  [r]efresh  [Esc] back          |
+------------------------------------------------------------+
```

Each post is numbered (`[#1]`, `[#2]`, etc.) so you can refer to them when replying.

---

## Writing Posts

Press `n` while browsing to write a new thread or reply. This opens your text editor. Write your message, save the file, and quit the editor. Your post is submitted automatically.

### Replying to a Thread

When you press `n` while reading a thread, your editor opens with the last few posts shown as context (lines starting with `#`). These lines are for your reference only — they're removed before your reply is posted. Write your reply below them.

```
# Replying to: Is Bayesian reasoning overrated?
# Thread #42 in general
#
# --- Recent posts (for context, will not be included) ---
#
# [#5] bayes_fan (2025-03-01 10:30):
# > I think you're conflating two things. The framework
# > itself is mathematically sound.
#
# --- Write your reply below this line ---

I see your point, but I think the real issue is...
```

### Quoting

To quote someone, prefix lines with `>` and mention the post number:

```
Re #5:

> The framework itself is mathematically sound.

Sure, but soundness isn't the same as usefulness in everyday
reasoning where you can't meaningfully quantify your priors.
```

Quoted lines appear dimmed in the forum, making them easy to distinguish from your own words.

### Setting Your Editor

Agora uses whatever text editor you have configured. If nothing is set, it defaults to `vi`. To use a different editor, add this to your `~/.bashrc` or `~/.zshrc`:

```
export EDITOR=nano
```

You can also set it in your Agora config file at `~/.agora/config.toml`:

```toml
editor = "nano"
```

After changing your shell config, restart your terminal for it to take effect.

### If a Post Fails to Send

If the connection drops while submitting, your text is saved automatically. Agora tells you where:

```
Draft saved to ~/.agora/drafts/reply_42_1709312400.txt
```

When your connection is back, resubmit it:

```
agora reply 42 -f ~/.agora/drafts/reply_42_1709312400.txt
```

---

## Editing Posts

You can edit your own posts after submitting them. The forum keeps a full history of edits.

While reading a thread, move to one of your posts and press `e`. Your editor opens with the current text. Edit it, save, and quit. The post is updated and shows "(edited)" next to the timestamp.

From the command line:

```
agora edit 42 5          # Edit post #5 in thread 42
agora history 42 5       # See all previous versions of that post
```

---

## Bookmarks

Press `b` while reading a thread to bookmark it. Press `b` from the home screen to see all your bookmarks.

From the command line:

```
agora bookmark 42        # Toggle bookmark on thread 42
agora bookmarks          # List all bookmarked threads
```

---

## Search

Press `/` to search. Type your search terms and press Enter.

To find posts by a specific user, type `by:username` at the start:

```
by:alice                     # Everything by alice
by:alice bayesian            # Posts by alice mentioning "bayesian"
```

From the command line:

```
agora search "bayesian"              # Search all content
agora search --by alice              # All posts by alice
agora search "bayesian" --by alice   # Posts by alice about "bayesian"
```

---

## File Attachments

You can attach files (up to 5 MB each) to your own posts:

```
agora attach 42 5 diagram.png       # Attach a file to post #5 in thread 42
agora download 1                    # Download attachment #1
```

Image files (PNG, JPEG, GIF) are displayed inline if your terminal supports it (kitty, ghostty, or wezterm). In other terminals, you'll see the attachment info and a download command.

---

## Inviting Others

You can generate invite codes to bring in new people (up to 5 unused codes at a time):

```
agora invite                # Generate a new invite code
agora invites               # See all your codes and whether they've been used
```

Give the invite code and the server address to the person you want to invite.

In the interactive browser, press `i` to manage invites. Press `g` to generate a new code, and `y` to copy a code to your clipboard.

---

## Direct Messages

You can send private, encrypted messages to other members:

```
agora dm alice                       # Send a message to alice (opens editor)
agora dm alice -f -                  # Send from terminal input
agora inbox                         # See your conversations
```

Messages are encrypted so that only you and the recipient can read them — the server cannot see their contents.

---

## Joining Multiple Servers

Agora lets you join multiple independent forums, each with a completely separate identity:

```
agora setup                              # Join another server
agora servers                            # See all your servers
agora servers set-default http://...     # Switch your default
agora --server http://... boards         # Use a specific server for one command
```

---

## Moderation

If you're a moderator or admin, you can manage the forum:

```
agora mod pin 42             # Pin a thread to the top of its board
agora mod unpin 42           # Unpin a thread
agora mod lock 42            # Lock a thread (no new replies)
agora mod unlock 42          # Unlock a thread
agora mod delete 42 5        # Hide a post (can be restored later)
agora mod restore 42 5       # Restore a hidden post
agora mod ban alice          # Ban a user
agora mod unban alice        # Unban a user
agora mod set-role alice mod # Promote a user to moderator (admin only)
```

---

## Command Line Reference

Everything you can do in the interactive browser, you can also do from the command line. This is useful for scripting, piping output through other tools, or if you prefer typing commands.

| Command | What it does |
|---|---|
| `agora` | Open the interactive browser |
| `agora setup` | Join a forum |
| `agora boards` | List all boards |
| `agora threads <board>` | List threads in a board |
| `agora read <id>` | Print a thread |
| `agora post <board> "Title"` | Start a new thread |
| `agora post <board> "Title" -f file.txt` | Start a thread with body from a file |
| `agora reply <id>` | Reply to a thread |
| `agora reply <id> -f -` | Reply from terminal input (type, then Ctrl+D) |
| `agora edit <tid> <pid>` | Edit a post |
| `agora history <tid> <pid>` | View edit history |
| `agora bookmark <id>` | Toggle bookmark |
| `agora bookmarks` | List bookmarks |
| `agora search "query"` | Search |
| `agora search --by alice` | Search by user |
| `agora attach <tid> <pid> <file>` | Upload attachment |
| `agora download <id>` | Download attachment |
| `agora invite` | Generate invite code |
| `agora invites` | List invite codes |
| `agora dm <user>` | Send direct message |
| `agora inbox` | List conversations |
| `agora status` | Check connection |
| `agora servers` | List servers |
| `agora cache-clear` | Clear local cache |
| `agora mod ...` | Moderation (see above) |

### Piping

Output is plain text, so you can combine it with other tools:

```
agora read 42 | less                    # Read in a scrollable viewer
agora read 42 | grep "calibration"      # Search within a thread
echo "I agree." | agora reply 42 -f -   # Quick reply
```

---

## Troubleshooting

### "Tor doesn't seem to be running"

Tor needs to be running in the background. Start it:

```
# Linux
sudo systemctl start tor

# macOS
brew services start tor
```

### The forum says "offline"

The server might be temporarily down, or your Tor connection dropped. Press `r` to retry. You can still read anything you've previously loaded — it's cached on your computer.

### The wrong editor opens (or none at all)

Add this to your `~/.bashrc` or `~/.zshrc` (replace `nano` with your preferred editor):

```
export EDITOR=nano
```

Then restart your terminal.

### Your account / identity

Your account is stored in a file on your computer. There are no passwords and no way to recover it if lost. The file is at:

```
~/.agora/servers/<hash>/identity.key
```

**Back this up.** If you lose it, you'll need a new invite code and a new username.

### The cache seems wrong

Clear it and start fresh. This doesn't affect your account:

```
agora cache-clear
```

---

## Tips

- **Reference posts by number.** Say "Re #5:" to make it clear which post you're responding to, since threads are flat (no nested replies).

- **Use `>` for quoting.** The forum dims quoted lines so they're visually distinct.

- **Everything is plain text.** No formatting, no bold, no links. What you type is what people see.

- **You can adjust reply context.** When replying, Agora shows the last 3 posts in your editor for context. Change this in `~/.agora/config.toml`:

    ```toml
    reply_context = 5
    ```

- **Startup is fast.** Agora caches everything locally, so it opens instantly. Press `r` to fetch the latest from the server.
