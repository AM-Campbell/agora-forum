#!/bin/sh
# Run the server in development mode with isolated DB and port.
# Usage: ./dev-server.sh
#
# This ensures dev never touches the production database or port.

export AGORA_DB="${AGORA_DB:-dev.db}"
export AGORA_BIND="${AGORA_BIND:-127.0.0.1:9090}"
export AGORA_NAME="${AGORA_NAME:-Agora Dev}"
export RUST_LOG="${RUST_LOG:-agora_server=debug,tower_http=debug}"

echo "  Dev server: bind=$AGORA_BIND  db=$AGORA_DB"
echo ""

cargo run --package agora-server "$@"
