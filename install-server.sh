#!/bin/sh
# Agora — Server install script
# Usage: sudo ./install-server.sh           Full interactive setup
#        sudo ./install-server.sh --upgrade  Update binary only, keep config
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

# ── Configuration ────────────────────────────────────────────

AGORA_USER="agora"
AGORA_DIR="/var/lib/agora"
AGORA_DB="${AGORA_DIR}/forum.db"
AGORA_BIN="/usr/local/bin/agora-server"
TOR_HIDDEN_SERVICE_DIR="/var/lib/tor/agora"
SERVICE_FILE="/etc/systemd/system/agora.service"

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

# ── Helper: download binary from GitHub ──────────────────────

install_binary_from_github() {
    ARCH="$(uname -m)"
    case "$ARCH" in
        x86_64|amd64) BINARY_NAME="agora-server-linux-x86_64" ;;
        aarch64|arm64) BINARY_NAME="agora-server-linux-aarch64" ;;
        *)
            echo "  Error: No pre-built server binary for $ARCH."
            echo "  Build from source first: cargo build --release -p agora-server"
            exit 1
            ;;
    esac

    DOWNLOAD_URL="https://github.com/AM-Campbell/agora-forum/releases/latest/download/${BINARY_NAME}"

    if command -v curl >/dev/null 2>&1; then
        if ! curl -fSL "$DOWNLOAD_URL" -o "$AGORA_BIN" 2>&1; then
            echo "  Error: Failed to download server binary from GitHub."
            echo "  URL: $DOWNLOAD_URL"
            echo ""
            echo "  Build from source instead: cargo build --release -p agora-server"
            exit 1
        fi
    elif command -v wget >/dev/null 2>&1; then
        if ! wget -q "$DOWNLOAD_URL" -O "$AGORA_BIN"; then
            echo "  Error: Failed to download server binary from GitHub."
            echo "  URL: $DOWNLOAD_URL"
            echo ""
            echo "  Build from source instead: cargo build --release -p agora-server"
            exit 1
        fi
    else
        echo "  Error: curl or wget required to download the binary."
        exit 1
    fi

    chmod +x "$AGORA_BIN"
    echo "        Downloaded from GitHub"
}

# ── Upgrade mode ─────────────────────────────────────────────

if [ "$1" = "--upgrade" ] || [ "$1" = "upgrade" ]; then
    echo ""
    echo "  ╔═══════════════════════════════════╗"
    echo "  ║    AGORA — Server Upgrade         ║"
    echo "  ╚═══════════════════════════════════╝"
    echo ""

    if [ ! -f "$SERVICE_FILE" ]; then
        echo "  Error: No existing Agora installation found."
        echo "  Run without --upgrade for a fresh install."
        exit 1
    fi

    echo "  [1/3] Downloading binary..."
    install_binary_from_github

    echo "  [2/3] Restarting service..."
    systemctl restart agora

    sleep 1
    if systemctl is-active --quiet agora; then
        echo "        Server is running"
    else
        echo ""
        echo "  Error: Server failed to start after upgrade."
        echo "  Check: journalctl -u agora -e"
        echo "  Rollback: restore the old binary to $AGORA_BIN"
        exit 1
    fi

    CURRENT_VERSION="$("$AGORA_BIN" --version 2>/dev/null || echo "unknown")"
    echo "  [3/3] Upgrade complete ($CURRENT_VERSION)"
    echo ""
    exit 0
fi

# ── Full install ─────────────────────────────────────────────

echo ""
echo "  ╔═══════════════════════════════════╗"
echo "  ║    AGORA — Server Installer       ║"
echo "  ╚═══════════════════════════════════╝"
echo ""

# Detect if this is a re-install
if [ -f "$SERVICE_FILE" ]; then
    echo "  Existing installation detected."
    echo "  Tip: use --upgrade to update the binary without changing config."
    echo ""
    printf "  Continue with full re-install? [y/N]: "
    read -r CONFIRM
    case "$CONFIRM" in
        y|Y|yes|YES) ;;
        *)
            echo "  Cancelled."
            exit 0
            ;;
    esac
    echo ""
fi

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

# ── Prompt for existing Tor keys (same-address migration) ───

EXISTING_TOR_DIR=""
if [ -n "$EXISTING_DB" ]; then
    echo ""
    echo "  To keep the same .onion address, provide the old Tor hidden service directory."
    echo "  (This is the folder containing hs_ed25519_secret_key, usually /var/lib/tor/agora/)"
    printf "  Path to old Tor hidden service dir (leave blank for new address): "
    read -r EXISTING_TOR_DIR
    if [ -n "$EXISTING_TOR_DIR" ]; then
        if [ ! -f "${EXISTING_TOR_DIR}/hs_ed25519_secret_key" ]; then
            echo "  Error: hs_ed25519_secret_key not found in: $EXISTING_TOR_DIR"
            exit 1
        fi
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
        pacman -S --noconfirm tor
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
chmod 700 "$AGORA_DIR"
chown "$AGORA_USER":"$AGORA_USER" "$AGORA_DIR"

# Copy existing database if provided
if [ -n "$EXISTING_DB" ]; then
    cp "$EXISTING_DB" "$AGORA_DB"
    # Also copy WAL/SHM files if they exist
    [ -f "${EXISTING_DB}-wal" ] && cp "${EXISTING_DB}-wal" "${AGORA_DB}-wal"
    [ -f "${EXISTING_DB}-shm" ] && cp "${EXISTING_DB}-shm" "${AGORA_DB}-shm"
    chown "$AGORA_USER":"$AGORA_USER" "$AGORA_DIR"/*
    chmod 600 "$AGORA_DB"
    [ -f "${AGORA_DB}-wal" ] && chmod 600 "${AGORA_DB}-wal"
    [ -f "${AGORA_DB}-shm" ] && chmod 600 "${AGORA_DB}-shm"
    echo "        Copied existing database to $AGORA_DB"
fi

# ── Step 3: Install binary ──────────────────────────────────

echo "  [3/6] Installing server binary..."
install_binary_from_github

# ── Step 4: Configure Tor hidden service ─────────────────────

echo "  [4/6] Configuring Tor hidden service..."

TORRC="/etc/tor/torrc"

if grep -q "HiddenServiceDir.*agora" "$TORRC" 2>/dev/null; then
    # Update existing config in case port changed
    if grep -q "HiddenServicePort 80 ${AGORA_BIND}" "$TORRC" 2>/dev/null; then
        echo "        Tor hidden service already configured"
    else
        # Port changed — update the HiddenServicePort line after the agora HiddenServiceDir
        sed -i "/^HiddenServiceDir.*agora/{n;s|HiddenServicePort.*|HiddenServicePort 80 ${AGORA_BIND}|;}" "$TORRC"
        echo "        Updated Tor hidden service port to ${AGORA_BIND}"
    fi
else
    cat >> "$TORRC" <<TOREOF

# Agora forum hidden service
HiddenServiceDir ${TOR_HIDDEN_SERVICE_DIR}/
HiddenServicePort 80 ${AGORA_BIND}
TOREOF
    echo "        Added hidden service to $TORRC"
fi

# Copy existing Tor keys if provided (same-address migration)
if [ -n "$EXISTING_TOR_DIR" ]; then
    mkdir -p "$TOR_HIDDEN_SERVICE_DIR"
    cp "${EXISTING_TOR_DIR}/hs_ed25519_secret_key" "$TOR_HIDDEN_SERVICE_DIR/"
    cp "${EXISTING_TOR_DIR}/hs_ed25519_public_key" "$TOR_HIDDEN_SERVICE_DIR/"
    cp "${EXISTING_TOR_DIR}/hostname" "$TOR_HIDDEN_SERVICE_DIR/"
    chown -R debian-tor:debian-tor "$TOR_HIDDEN_SERVICE_DIR" 2>/dev/null || \
    chown -R tor:tor "$TOR_HIDDEN_SERVICE_DIR" 2>/dev/null || true
    chmod 700 "$TOR_HIDDEN_SERVICE_DIR"
    chmod 600 "$TOR_HIDDEN_SERVICE_DIR"/hs_ed25519_*
    echo "        Restored Tor keys from $EXISTING_TOR_DIR"
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

# Quotes around Environment= values are required for names with spaces
cat > "$SERVICE_FILE" <<SVCEOF
[Unit]
Description=Agora Forum Server
After=network.target tor.service

[Service]
Type=simple
User=${AGORA_USER}
Group=${AGORA_USER}
WorkingDirectory=${AGORA_DIR}
Environment="AGORA_NAME=${AGORA_NAME}"
Environment="AGORA_URL=http://${ONION_ADDR}"
Environment="AGORA_DB=${AGORA_DB}"
Environment="AGORA_BIND=${AGORA_BIND}"
ExecStart=${AGORA_BIN}
Restart=on-failure
RestartSec=5

# Hardening
NoNewPrivileges=true
ProtectSystem=strict
ProtectHome=true
ReadWritePaths=${AGORA_DIR}
PrivateTmp=true
PrivateDevices=true
ProtectKernelTunables=true
ProtectKernelModules=true
ProtectControlGroups=true
RestrictNamespaces=true
RestrictRealtime=true
RestrictSUIDSGID=true
MemoryDenyWriteExecute=true
LockPersonality=true
SystemCallFilter=@system-service
SystemCallArchitectures=native
UMask=0077

[Install]
WantedBy=multi-user.target
SVCEOF

systemctl daemon-reload
echo "        Created $SERVICE_FILE"

# ── Step 6: Start and capture bootstrap code ─────────────────

echo "  [6/6] Starting Agora server..."

systemctl enable agora 2>/dev/null
systemctl restart agora

# Give the server a moment to start and print the bootstrap code
sleep 2

BOOTSTRAP_CODE=""
if [ -z "$EXISTING_DB" ]; then
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
    CODE_LEN=${#BOOTSTRAP_CODE}
    BOX_WIDTH=$((CODE_LEN + 28))
    BORDER=$(printf '─%.0s' $(seq 1 $BOX_WIDTH))
    echo "  ┌${BORDER}┐"
    echo "  │  BOOTSTRAP INVITE CODE: ${BOOTSTRAP_CODE}  │"
    echo "  └${BORDER}┘"
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
    if [ "$ONION_ADDR" != "<pending>" ]; then
        echo "  Users should update their server address if it changed."
    fi
else
    echo ""
    echo "  To retrieve the bootstrap code:"
    echo "      journalctl -u agora | grep 'BOOTSTRAP INVITE CODE'"
fi

echo ""
echo "  Manage the server:"
echo "      sudo systemctl status agora        Check status"
echo "      sudo journalctl -u agora -f        View logs"
echo "      sudo systemctl restart agora       Restart"
echo "      sudo ./install-server.sh --upgrade Update binary"
echo ""
