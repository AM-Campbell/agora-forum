#!/bin/sh
# Agora — install script
# Usage: curl -sSL https://raw.githubusercontent.com/am-campbell/agora-forum/main/install.sh | sh
set -e

REPO="am-campbell/agora-forum"
INSTALL_DIR="$HOME/.local/bin"

echo ""
echo "  ╔═══════════════════════════════════╗"
echo "  ║         AGORA — Installer         ║"
echo "  ╚═══════════════════════════════════╝"
echo ""

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

echo "  [1/4] Detected $OS_NAME ($ARCH_NAME)"

# ── Step 2: Install Tor if missing ──────────────────────────────

if command -v tor >/dev/null 2>&1; then
    echo "  [2/4] Tor is already installed"
else
    echo "  [2/4] Installing Tor..."
    echo ""
    echo "  Tor is free privacy software that Agora uses to connect securely."
    echo "  This step may ask for your password."
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
    # Try standard Tor port
    if (echo >/dev/tcp/127.0.0.1/9050) 2>/dev/null; then
        SOCKS_PORT=9050
        break
    fi
    # Try Tor Browser port
    if (echo >/dev/tcp/127.0.0.1/9150) 2>/dev/null; then
        SOCKS_PORT=9150
        break
    fi
    # Fallback: try with nc if /dev/tcp not available (common on macOS)
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

# Remove old binary if present (data in ~/.agora/ is preserved)
if [ -f "$INSTALL_DIR/agora" ]; then
    echo "        Replacing existing installation"
    rm -f "$INSTALL_DIR/agora"
fi

if command -v curl >/dev/null 2>&1; then
    curl -fSL "$DOWNLOAD_URL" -o "$INSTALL_DIR/agora" 2>/dev/null
elif command -v wget >/dev/null 2>&1; then
    wget -q "$DOWNLOAD_URL" -O "$INSTALL_DIR/agora"
else
    echo "  Error: curl or wget is required to download the binary."
    exit 1
fi

chmod +x "$INSTALL_DIR/agora"

# ── Ensure ~/.local/bin is in PATH ──────────────────────────────

add_to_path() {
    rcfile="$1"
    if [ -f "$rcfile" ]; then
        if ! grep -q '\.local/bin' "$rcfile" 2>/dev/null; then
            echo '' >> "$rcfile"
            echo '# Added by Agora installer' >> "$rcfile"
            echo 'export PATH="$HOME/.local/bin:$PATH"' >> "$rcfile"
            return 0
        fi
    fi
    return 1
}

PATH_UPDATED=0
case ":$PATH:" in
    *":$INSTALL_DIR:"*|*":$HOME/.local/bin:"*)
        # Already in PATH
        ;;
    *)
        UPDATED=0
        if [ -f "$HOME/.zshrc" ]; then
            add_to_path "$HOME/.zshrc" && UPDATED=1
        fi
        if [ -f "$HOME/.bashrc" ]; then
            add_to_path "$HOME/.bashrc" && UPDATED=1
        fi
        if [ $UPDATED -eq 0 ]; then
            echo 'export PATH="$HOME/.local/bin:$PATH"' >> "$HOME/.bashrc"
        fi
        PATH_UPDATED=1
        # Add to current session so 'agora setup' works immediately
        export PATH="$HOME/.local/bin:$PATH"
        ;;
esac

# ── Done ─────────────────────────────────────────────────────────

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
echo "  Restoring an existing profile? Run:"
echo ""
echo "      agora profile import <your-backup-file>"
echo ""
