#!/bin/sh
# Agora — install script
# Usage: curl -sSL https://raw.githubusercontent.com/AM-Campbell/agora-forum/refs/heads/master/install.sh | sh
set -e

REPO="AM-Campbell/agora-forum"
INSTALL_DIR="$HOME/.local/bin"

# Detect if this is an upgrade (binary exists AND Tor is installed = previous install was complete)
IS_UPGRADE=0
if [ -f "$INSTALL_DIR/agora" ] && command -v tor >/dev/null 2>&1; then
    IS_UPGRADE=1
fi

if [ $IS_UPGRADE -eq 1 ]; then
    echo ""
    echo "  ╔═══════════════════════════════════╗"
    echo "  ║       AGORA — Upgrading...        ║"
    echo "  ╚═══════════════════════════════════╝"
    echo ""
    echo "  Existing installation detected. Your profile and config are safe."
    echo ""
else
    echo ""
    echo "  ╔═══════════════════════════════════╗"
    echo "  ║         AGORA — Installer         ║"
    echo "  ╚═══════════════════════════════════╝"
    echo ""
fi

# ── Step 1: Detect OS and architecture ───────────────────────────

OS="$(uname -s)"
ARCH="$(uname -m)"

case "$OS" in
    Linux)  OS_NAME="linux" ;;
    Darwin) OS_NAME="macos" ;;
    *)
        echo "  Sorry, Agora only runs on Linux and macOS."
        echo "  Your system ($OS) is not supported."
        exit 1
        ;;
esac

case "$ARCH" in
    x86_64|amd64)   ARCH_NAME="x86_64" ;;
    aarch64|arm64)   ARCH_NAME="aarch64" ;;
    *)
        echo "  Sorry, your processor type ($ARCH) is not supported."
        echo "  Agora supports x86_64 (most computers) and ARM64 (Apple Silicon, Raspberry Pi)."
        exit 1
        ;;
esac

if [ "$OS_NAME" = "macos" ] && [ "$ARCH_NAME" = "x86_64" ]; then
    echo "  Sorry, Intel Macs are not supported. Agora requires Apple Silicon (M1/M2/M3/M4)."
    exit 1
fi

# ── macOS: require Homebrew early ─────────────────────────────
# Homebrew is needed to install Tor (and potentially other deps).
# Check this BEFORE downloading anything so a partial install doesn't
# trick the next run into thinking it's an upgrade.

if [ "$OS_NAME" = "macos" ] && ! command -v brew >/dev/null 2>&1; then
    echo "  Agora requires Homebrew on macOS (to install Tor and other tools)."
    echo ""
    echo "  To install Homebrew, paste this into your terminal:"
    echo ""
    echo "      /bin/bash -c \"\$(curl -fsSL https://raw.githubusercontent.com/Homebrew/install/HEAD/install.sh)\""
    echo ""
    echo "  After installing Homebrew, close and reopen your terminal,"
    echo "  then re-run this Agora installer."
    exit 1
fi

echo "  [1/4] Detected $OS_NAME ($ARCH_NAME)"

# ── Step 2: Install Tor if missing ──────────────────────────────

if command -v tor >/dev/null 2>&1; then
    echo "  [2/4] Tor is already installed"
    # Make sure the service is running (user may have installed but not started it)
    if [ "$OS_NAME" = "macos" ] && command -v brew >/dev/null 2>&1; then
        brew services start tor 2>/dev/null || true
    elif command -v systemctl >/dev/null 2>&1; then
        sudo systemctl start tor 2>/dev/null || true
    fi
else
    echo "  [2/4] Installing Tor..."
    echo ""
    echo "  Tor is free privacy software that Agora uses to connect securely."
    echo "  Installing it requires administrator access, so your computer will"
    echo "  ask for your password (the one you use to log in to your computer)."
    echo "  The password won't appear as you type — that's normal. Just type it"
    echo "  and press Enter."
    echo ""

    if command -v apt >/dev/null 2>&1; then
        sudo apt update -qq && sudo apt install -y -qq tor
        sudo systemctl enable tor 2>/dev/null || true
        sudo systemctl start tor 2>/dev/null || true
    elif command -v pacman >/dev/null 2>&1; then
        sudo pacman -S --noconfirm tor
        sudo systemctl enable tor 2>/dev/null || true
        sudo systemctl start tor 2>/dev/null || true
    elif command -v dnf >/dev/null 2>&1; then
        sudo dnf install -y -q tor
        sudo systemctl enable tor 2>/dev/null || true
        sudo systemctl start tor 2>/dev/null || true
    elif command -v brew >/dev/null 2>&1; then
        brew install tor
        brew services start tor
    else
        echo ""
        echo "  Could not install Tor automatically."
        echo ""
        echo "  Please install Tor manually from: https://www.torproject.org/download/"
        echo "  After installing, start the Tor service and re-run this installer."
        exit 1
    fi

    echo ""
    echo "  Tor installed successfully."
fi

# ── Step 3: Wait for Tor to be ready ─────────────────────────────

echo "  [3/4] Checking Tor connection..."

SOCKS_PORT=""
ATTEMPTS=0
MAX_ATTEMPTS=15

while [ $ATTEMPTS -lt $MAX_ATTEMPTS ]; do
    # Try nc first (works on macOS and most Linux; /dev/tcp is bash-only)
    if command -v nc >/dev/null 2>&1; then
        if nc -z 127.0.0.1 9050 2>/dev/null; then
            SOCKS_PORT=9050
            break
        fi
        if nc -z 127.0.0.1 9150 2>/dev/null; then
            SOCKS_PORT=9150
            break
        fi
    fi
    # Fallback: /dev/tcp (bash-only, won't work under plain sh on macOS)
    if (echo >/dev/tcp/127.0.0.1/9050) 2>/dev/null; then
        SOCKS_PORT=9050
        break
    fi
    if (echo >/dev/tcp/127.0.0.1/9150) 2>/dev/null; then
        SOCKS_PORT=9150
        break
    fi
    ATTEMPTS=$((ATTEMPTS + 1))
    if [ $ATTEMPTS -eq 1 ]; then
        printf "        Waiting for Tor to start"
    fi
    printf "."
    sleep 1
done

if [ $ATTEMPTS -gt 0 ]; then
    echo ""
fi

if [ -z "$SOCKS_PORT" ]; then
    echo ""
    echo "  Tor is installed but doesn't seem to be running yet."
    echo "  That's OK — Agora will detect it when you run 'agora setup'."
    echo ""
    echo "  If Tor still isn't running later, try:"
    if [ "$OS_NAME" = "macos" ]; then
        echo "    brew services start tor"
    else
        echo "    sudo systemctl start tor"
    fi
    echo ""
else
    echo "        Tor is running (port $SOCKS_PORT)"
fi

# ── Step 4: Download Agora binary ───────────────────────────────

echo "  [4/4] Downloading Agora..."

BINARY_NAME="agora-${OS_NAME}-${ARCH_NAME}"
DOWNLOAD_URL="https://github.com/${REPO}/releases/latest/download/${BINARY_NAME}"

mkdir -p "$INSTALL_DIR"

if command -v curl >/dev/null 2>&1; then
    if ! curl -fSL "$DOWNLOAD_URL" -o "$INSTALL_DIR/agora" 2>&1; then
        echo "  Error: Failed to download Agora binary."
        echo "  URL: $DOWNLOAD_URL"
        exit 1
    fi
elif command -v wget >/dev/null 2>&1; then
    if ! wget -q "$DOWNLOAD_URL" -O "$INSTALL_DIR/agora"; then
        echo "  Error: Failed to download Agora binary."
        echo "  URL: $DOWNLOAD_URL"
        exit 1
    fi
else
    echo "  Error: curl or wget is required to download the binary."
    exit 1
fi

chmod +x "$INSTALL_DIR/agora"

# ── Ensure ~/.local/bin is in PATH ──────────────────────────────

add_to_path() {
    rcfile="$1"
    # Create the file if it doesn't exist (new Mac users often have no .zshrc)
    if [ ! -f "$rcfile" ]; then
        touch "$rcfile"
    fi
    if ! grep -q '\.local/bin' "$rcfile" 2>/dev/null; then
        echo '' >> "$rcfile"
        echo '# Added by Agora installer' >> "$rcfile"
        echo 'export PATH="$HOME/.local/bin:$PATH"' >> "$rcfile"
        return 0
    fi
    return 1
}

# Figure out which rc file to update based on the user's actual shell
detect_shell_rc() {
    CURRENT_SHELL="$(basename "$SHELL" 2>/dev/null || echo "")"
    case "$CURRENT_SHELL" in
        zsh)  echo "$HOME/.zshrc" ;;
        fish) echo "" ;; # fish uses a different PATH mechanism
        bash) echo "$HOME/.bashrc" ;;
        *)
            # Unknown shell — use .profile as a safe fallback (read by most login shells)
            echo "$HOME/.profile"
            ;;
    esac
}

SHELL_RC="$(detect_shell_rc)"

PATH_UPDATED=0
case ":$PATH:" in
    *":$INSTALL_DIR:"*|*":$HOME/.local/bin:"*)
        # Already in PATH
        ;;
    *)
        if [ -n "$SHELL_RC" ]; then
            add_to_path "$SHELL_RC"
        fi
        PATH_UPDATED=1
        # Add to current session so 'agora setup' works immediately
        export PATH="$HOME/.local/bin:$PATH"
        ;;
esac

# ── Set default editor if not configured ──────────────────────
# Non-technical users (especially on fresh Macs) may have no EDITOR set,
# which means they'd get dropped into vim. Default to nano instead.

if [ -z "$EDITOR" ] && [ -z "$VISUAL" ] && command -v nano >/dev/null 2>&1; then
    if [ -n "$SHELL_RC" ]; then
        if ! grep -q 'export EDITOR=' "$SHELL_RC" 2>/dev/null; then
            echo '' >> "$SHELL_RC"
            echo '# Default editor (added by Agora installer)' >> "$SHELL_RC"
            echo 'export EDITOR=nano' >> "$SHELL_RC"
        fi
    fi
fi

# ── Done ─────────────────────────────────────────────────────────

if [ $IS_UPGRADE -eq 1 ]; then
    echo ""
    echo "  ╔═══════════════════════════════════╗"
    echo "  ║        Upgrade complete!          ║"
    echo "  ╚═══════════════════════════════════╝"
    echo ""
    echo "  Your profile and settings are unchanged."
    echo "  Run 'agora' to start."
    echo ""
else
    echo ""
    echo "  ╔═══════════════════════════════════╗"
    echo "  ║       Installation complete!      ║"
    echo "  ╚═══════════════════════════════════╝"
    echo ""

    if [ $PATH_UPDATED -eq 1 ]; then
        echo "  Note: After this setup finishes, restart your terminal"
        echo "  (or run: export PATH=\"\$HOME/.local/bin:\$PATH\")"
        echo "  so that 'agora' is available everywhere."
        echo ""
    fi

    echo "  To join a forum, run:"
    echo ""
    echo "      agora setup"
    echo ""
    echo "  You'll need two things from the person who invited you:"
    echo "    1. A server address (looks like http://xxxx.onion)"
    echo "    2. An invite code (a short string of letters and numbers)"
    echo ""
    echo "  Optional: enable tab-completion for agora commands:"
    echo ""
    COMP_SHELL="$(basename "$SHELL" 2>/dev/null || echo "bash")"
    case "$COMP_SHELL" in
        zsh)
            echo "      agora completions zsh >> ~/.zshrc && source ~/.zshrc"
            ;;
        fish)
            echo "      agora completions fish > ~/.config/fish/completions/agora.fish"
            ;;
        *)
            echo "      agora completions bash >> ~/.bashrc && source ~/.bashrc"
            ;;
    esac
    echo ""
fi
