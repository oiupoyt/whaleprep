#!/usr/bin/env bash
# Start WhalePrep on Termux (SQLite, port 8222) in background.
# Usage: ./deploy/termux-start.sh   (run from repo root)
cd "$(dirname "$0")/.."
PID_FILE="./whaleprep.pid"
LOG_FILE="./whaleprep.log"
DB_FILE="./whaleprep.db"
PORT=8222
if [ -f "$PID_FILE" ]; then
    PID=$(cat "$PID_FILE")
    if kill -0 "$PID" 2>/dev/null; then
        echo "WhalePrep is already running (PID: $PID)"
        exit 0
    else
        rm -f "$PID_FILE"
    fi
fi
[ -x ./target/release/whaleprep ] || { echo "missing ./target/release/whaleprep — run: cargo build --release"; exit 1; }
# Optional secrets: GOOGLE_CLIENT_ID / GOOGLE_CLIENT_SECRET, one per line.
# Without them the app runs with the name+email form only.
[ -f ./.env ] && set -a && . ./.env && set +a || true
if [ -z "${GOOGLE_CLIENT_ID:-}" ]; then
  echo "note: GOOGLE_CLIENT_ID unset — Google button stays hidden, email form only"
fi
echo "Starting WhalePrep on :$PORT (db $DB_FILE)..."
PYQ_DB="$DB_FILE" PYQ_ADDR="127.0.0.1:$PORT" RUST_LOG=info nohup ./target/release/whaleprep </dev/null >> "$LOG_FILE" 2>&1 &
NEW_PID=$!
echo $NEW_PID > "$PID_FILE"
disown $NEW_PID 2>/dev/null || true
sleep 2
if kill -0 "$NEW_PID" 2>/dev/null; then
    echo "WhalePrep started (PID: $NEW_PID) -> http://localhost:$PORT"
else
    echo "FAILED — tail $LOG_FILE:"; tail -5 "$LOG_FILE"
fi
