#!/bin/sh
# Agora — Server install script
# Usage: sudo ./install-server.sh
#
# Automates server deployment:
#   1. Creates dedicated agora user and directories
#   2. Installs and configures Tor hidden service
#   3. Installs the server binary
#   4. Creates and enables a systemd service
#   5. Starts the server and prints the bootstrap invite code + .onion address
#
# Prerequisites: Linux with systemd, Tor installable via package manager.
# Run as root or with sudo.

set -e

# ── Check prerequisites ──────────────────────────────────────

if [ "$(id -u)" -ne 0 ]; then
    echo "Error: This script must be run as root (or with sudo)."
    exit 1
fi

if [ "$(uname -s)" != "Linux" ]; then
    echo "Error: This script only supports Linux."
    exit 1
fi

if ! command -v systemctl >/dev/null 2>&1; then
    echo "Error: systemd is required."
    exit 1
fi

echo ""
echo "  ╔═══════════════════════════════════╗"
echo "  ║    AGORA — Server Installer       ║"
echo "  ╚═══════════════════════════════════╝"
echo ""

# ── Configuration ────────────────────────────────────────────

AGORA_USER="agora"
AGORA_DIR="/var/lib/agora"
AGORA_DB="${AGORA_DIR}/forum.db"
AGORA_BIND="127.0.0.1:8080"
AGORA_BIN="/usr/local/bin/agora-server"
TOR_HIDDEN_SERVICE_DIR="/var/lib/tor/agora"
SERVICE_FILE="/etc/systemd/system/agora.service"

# ── Prompt for forum name ────────────────────────────────────

printf "  Forum name (shown to users, e.g. \"Book Club\"): "
read -r AGORA_NAME
if [ -z "$AGORA_NAME" ]; then
    AGORA_NAME="Agora"
fi

# ── Prompt for bind port ─────────────────────────────────────

printf "  Bind port [8080]: "
read -r BIND_PORT
if [ -z "$BIND_PORT" ]; then
    BIND_PORT="8080"
fi
AGORA_BIND="127.0.0.1:${BIND_PORT}"

# ── Prompt for existing database ─────────────────────────────

printf "  Path to existing database (leave blank for new forum): "
read -r EXISTING_DB
if [ -n "$EXISTING_DB" ]; then
    if [ ! -f "$EXISTING_DB" ]; then
        echo "  Error: File not found: $EXISTING_DB"
        exit 1
    fi
fi

# ── Step 1: Install Tor ─────────────────────────────────────

echo ""
echo "  [1/6] Installing Tor..."

if command -v tor >/dev/null 2>&1; then
    echo "        Tor is already installed"
else
    if command -v apt >/dev/null 2>&1; then
        apt update -qq && apt install -y -qq tor
    elif command -v pacman >/dev/null 2>&1; then
        pacman -Sy --noconfirm tor
    elif command -v dnf >/dev/null 2>&1; then
        dnf install -y -q tor
    else
        echo "  Error: Could not install Tor. Install it manually and re-run."
        exit 1
    fi
    echo "        Tor installed"
fi

# ── Step 2: Create user and directories ──────────────────────

echo "  [2/6] Creating agora user and directories..."

if id "$AGORA_USER" >/dev/null 2>&1; then
    echo "        User '$AGORA_USER' already exists"
else
    useradd -r -s /usr/sbin/nologin "$AGORA_USER" 2>/dev/null || \
    useradd -r -s /bin/false "$AGORA_USER"
    echo "        Created user '$AGORA_USER'"
fi

mkdir -p "$AGORA_DIR"
chown "$AGORA_USER":"$AGORA_USER" "$AGORA_DIR"

# Copy existing database if provided
if [ -n "$EXISTING_DB" ]; then
    cp "$EXISTING_DB" "$AGORA_DB"
    # Also copy WAL/SHM files if they exist
    [ -f "${EXISTING_DB}-wal" ] && cp "${EXISTING_DB}-wal" "${AGORA_DB}-wal"
    [ -f "${EXISTING_DB}-shm" ] && cp "${EXISTING_DB}-shm" "${AGORA_DB}-shm"
    chown "$AGORA_USER":"$AGORA_USER" "$AGORA_DIR"/*
    echo "        Copied existing database to $AGORA_DB"
fi

# ── Step 3: Install binary ──────────────────────────────────

echo "  [3/6] Installing server binary..."

# Check if there's a local build first
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
LOCAL_BIN="${SCRIPT_DIR}/target/release/agora-server"

if [ -f "$LOCAL_BIN" ]; then
    cp "$LOCAL_BIN" "$AGORA_BIN"
    echo "        Installed from local build"
else
    # Try to download from GitHub
    ARCH="$(uname -m)"
    case "$ARCH" in
        x86_64|amd64) BINARY_NAME="agora-server-linux-x86_64" ;;
        *)
            echo "  Error: No pre-built server binary for $ARCH."
            echo "  Build from source first: cargo build --release"
            exit 1
            ;;
    esac

    DOWNLOAD_URL="https://github.com/AM-Campbell/agora-forum/releases/latest/download/${BINARY_NAME}"

    if command -v curl >/dev/null 2>&1; then
        curl -fSL "$DOWNLOAD_URL" -o "$AGORA_BIN" 2>/dev/null
    elif command -v wget >/dev/null 2>&1; then
        wget -q "$DOWNLOAD_URL" -O "$AGORA_BIN"
    else
        echo "  Error: curl or wget required to download the binary."
        exit 1
    fi
    echo "        Downloaded from GitHub"
fi

chmod +x "$AGORA_BIN"

# ── Step 4: Configure Tor hidden service ─────────────────────

echo "  [4/6] Configuring Tor hidden service..."

TORRC="/etc/tor/torrc"

if grep -q "HiddenServiceDir.*agora" "$TORRC" 2>/dev/null; then
    echo "        Tor hidden service already configured"
else
    cat >> "$TORRC" <<TOREOF

# Agora forum hidden service
HiddenServiceDir ${TOR_HIDDEN_SERVICE_DIR}/
HiddenServicePort 80 ${AGORA_BIND}
TOREOF
    echo "        Added hidden service to $TORRC"
fi

systemctl enable tor 2>/dev/null || true
systemctl restart tor

# Wait for .onion address to be generated
ATTEMPTS=0
while [ $ATTEMPTS -lt 30 ]; do
    if [ -f "${TOR_HIDDEN_SERVICE_DIR}/hostname" ]; then
        break
    fi
    ATTEMPTS=$((ATTEMPTS + 1))
    sleep 1
done

if [ -f "${TOR_HIDDEN_SERVICE_DIR}/hostname" ]; then
    ONION_ADDR="$(cat "${TOR_HIDDEN_SERVICE_DIR}/hostname")"
    echo "        .onion address: $ONION_ADDR"
else
    echo "        Warning: .onion address not yet available. Check: sudo cat ${TOR_HIDDEN_SERVICE_DIR}/hostname"
    ONION_ADDR="<pending>"
fi

# ── Step 5: Create systemd service ──────────────────────────

echo "  [5/6] Creating systemd service..."

cat > "$SERVICE_FILE" <<SVCEOF
[Unit]
Description=Agora Forum Server
After=network.target tor.service

[Service]
Type=simple
User=${AGORA_USER}
Group=${AGORA_USER}
WorkingDirectory=${AGORA_DIR}
Environment=AGORA_NAME=${AGORA_NAME}
Environment=AGORA_DB=${AGORA_DB}
Environment=AGORA_BIND=${AGORA_BIND}
ExecStart=${AGORA_BIN}
Restart=on-failure
RestartSec=5

# Hardening
NoNewPrivileges=true
ProtectSystem=strict
ProtectHome=true
ReadWritePaths=${AGORA_DIR}
PrivateTmp=true

[Install]
WantedBy=multi-user.target
SVCEOF

systemctl daemon-reload
echo "        Created $SERVICE_FILE"

# ── Step 6: Start and capture bootstrap code ─────────────────

echo "  [6/6] Starting Agora server..."

systemctl enable agora 2>/dev/null
systemctl start agora

# Give the server a moment to start and print the bootstrap code
sleep 2

BOOTSTRAP_CODE=""
if [ ! -f "$AGORA_DB" ] || [ -z "$EXISTING_DB" ]; then
    # Try to capture bootstrap code from journal
    BOOTSTRAP_CODE="$(journalctl -u agora --no-pager -n 20 2>/dev/null | grep 'BOOTSTRAP INVITE CODE' | tail -1 | sed 's/.*BOOTSTRAP INVITE CODE: //')"
fi

# Verify it's running
if systemctl is-active --quiet agora; then
    echo "        Server is running"
else
    echo ""
    echo "  Error: Server failed to start. Check: journalctl -u agora -e"
    exit 1
fi

# ── Done ─────────────────────────────────────────────────────

echo ""
echo "  ╔═══════════════════════════════════╗"
echo "  ║       Setup complete!             ║"
echo "  ╚═══════════════════════════════════╝"
echo ""
echo "  Forum name:     $AGORA_NAME"
echo "  Database:       $AGORA_DB"
echo "  Listening on:   $AGORA_BIND"

if [ "$ONION_ADDR" != "<pending>" ]; then
    echo "  .onion address: http://$ONION_ADDR"
fi

if [ -n "$BOOTSTRAP_CODE" ]; then
    echo ""
    echo "  ┌─────────────────────────────────────────────────┐"
    echo "  │  BOOTSTRAP INVITE CODE: $BOOTSTRAP_CODE  │"
    echo "  └─────────────────────────────────────────────────┘"
    echo ""
    echo "  The first user to register with this code becomes admin."
    echo "  To register, run on a client machine:"
    echo ""
    echo "      agora setup"
    echo ""
    echo "  Enter the .onion address and this invite code when prompted."
elif [ -n "$EXISTING_DB" ]; then
    echo ""
    echo "  Existing database loaded. No new invite code needed."
    echo "  Users should update their server address:"
    echo ""
    echo "      agora servers update-address <old-address> http://$ONION_ADDR"
else
    echo ""
    echo "  To retrieve the bootstrap code:"
    echo "      journalctl -u agora | grep 'BOOTSTRAP INVITE CODE'"
fi

echo ""
echo "  Manage the server:"
echo "      sudo systemctl status agora      Check status"
echo "      sudo journalctl -u agora -f      View logs"
echo "      sudo systemctl restart agora     Restart"
echo ""
