#!/bin/bash
# Starts the clone detached so it survives shell teardown. Usage: run_server.sh <port> <db>
set -e
DIR="/home/oiupoyt/my stuff yea/projects/zenith-pyq-clone"
PORT="${1:-8123}"
DB="${2:-/tmp/pyq_run.db}"
cd "$DIR"
rm -f "$DB" "$DB-wal" "$DB-shm"
PYQ_DB="$DB" PYQ_ADDR="127.0.0.1:$PORT" RUST_LOG=info \
  setsid nohup ./target/debug/zenith-pyq-clone > "/tmp/pyq_server_$PORT.log" 2>&1 < /dev/null &
echo "started pid=$! port=$PORT db=$DB"
sleep 3
curl -s -o /dev/null -w "health=%{http_code}\n" "http://127.0.0.1:$PORT/"