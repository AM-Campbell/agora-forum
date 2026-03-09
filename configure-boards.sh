#!/usr/bin/env bash
# Configure custom boards for an Agora forum server.
# Run this after the server has started at least once (so the DB exists).
#
# Usage: ./configure-boards.sh [--force]
#   --force  Delete boards even if they contain threads
#
# Edit the BOARDS array below to customize. Format:
#   "slug|Display Name|Description"
# Order in the array determines sort order.

set -euo pipefail

FORCE=false
for arg in "$@"; do
  case "$arg" in
    --force) FORCE=true ;;
    *) echo "Unknown option: $arg"; echo "Usage: $0 [--force]"; exit 1 ;;
  esac
done

AGORA_DB="${AGORA_DB:-/var/lib/agora/forum.db}"

# ── Customize your boards here ──────────────────────────────────────
BOARDS=(
  "announcements|Announcements|Official announcements and updates"
  "history|History|Historical discussion and analysis"
  "rationality|Rationality|Rationality, epistemics, and decision-making"
  "stats-ml|Stats & ML|Statistics, machine learning, and data science"
)
# ────────────────────────────────────────────────────────────────────

if [ ! -f "$AGORA_DB" ]; then
  echo "Error: database not found at $AGORA_DB"
  echo "Set AGORA_DB or run the server once first to create it."
  exit 1
fi

if ! command -v sqlite3 &>/dev/null; then
  echo "Error: sqlite3 is required but not found."
  exit 1
fi

# Collect slugs of boards that can't be removed (have threads, no --force)
skip_slugs=()
existing_boards=$(sqlite3 "$AGORA_DB" "SELECT slug FROM boards;")
for slug in $existing_boards; do
  keep=false
  for entry in "${BOARDS[@]}"; do
    if [ "${entry%%|*}" = "$slug" ]; then
      keep=true
      break
    fi
  done
  if [ "$keep" = false ]; then
    safe_slug="${slug//\'/\'\'}"
    count=$(sqlite3 "$AGORA_DB" "SELECT COUNT(*) FROM threads WHERE board_id = (SELECT id FROM boards WHERE slug = '$safe_slug');")
    if [ "$count" -gt 0 ]; then
      if [ "$FORCE" = true ]; then
        echo "Force-removing board '$slug' ($count thread(s) will be deleted)"
      else
        echo "Skipping board '$slug' — has $count thread(s). Use --force to delete anyway."
        skip_slugs+=("$slug")
      fi
    fi
  fi
done

# Build SQL
sql="BEGIN TRANSACTION;"

# Build the NOT IN list: boards we want to keep + boards we must skip
keep_slugs=""
for entry in "${BOARDS[@]}"; do
  slug="${entry%%|*}"
  keep_slugs="$keep_slugs'${slug//\'/\'\'}',"
done
for slug in "${skip_slugs[@]+"${skip_slugs[@]}"}"; do
  keep_slugs="$keep_slugs'${slug//\'/\'\'}',"
done
keep_slugs="${keep_slugs%,}"

if [ "$FORCE" = true ]; then
  # Delete threads and posts from boards being removed
  sql="$sql DELETE FROM posts WHERE thread_id IN (SELECT id FROM threads WHERE board_id IN (SELECT id FROM boards WHERE slug NOT IN ($keep_slugs)));"
  sql="$sql DELETE FROM threads WHERE board_id IN (SELECT id FROM boards WHERE slug NOT IN ($keep_slugs));"
fi

sql="$sql DELETE FROM boards WHERE slug NOT IN ($keep_slugs);"

# Upsert each board
order=0
for entry in "${BOARDS[@]}"; do
  IFS='|' read -r slug name desc <<< "$entry"
  slug="${slug//\'/\'\'}"
  name="${name//\'/\'\'}"
  desc="${desc//\'/\'\'}"
  sql="$sql INSERT INTO boards (slug, name, description, sort_order) VALUES ('$slug', '$name', '$desc', $order)"
  sql="$sql ON CONFLICT(slug) DO UPDATE SET name='$name', description='$desc', sort_order=$order;"
  order=$((order + 1))
done

sql="$sql COMMIT;"

sqlite3 "$AGORA_DB" "$sql"

echo "Boards configured:"
sqlite3 "$AGORA_DB" "SELECT sort_order, slug, name FROM boards ORDER BY sort_order;" -column -header
