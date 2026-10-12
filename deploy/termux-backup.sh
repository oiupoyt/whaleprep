#!/usr/bin/env bash
# Daily SQLite backup: VACUUM INTO a timestamped copy, keep 7 days.
# Usage: ./deploy/termux-backup.sh  (run from repo root; safe while live)
# Termux has no cron, so run this from Termux:Tasker, a daily alarm via
# Termux:API, or manually. One line, one snapshot.
cd "$(dirname "$0")/.."
OUT="./backups/whaleprep-$(date +%F).db"
mkdir -p ./backups
sqlite3 ./whaleprep.db "VACUUM INTO '$OUT';"
ls -t ./backups/whaleprep-*.db | tail -n +8 | xargs -r rm -f
echo "backup -> $OUT ($(du -h "$OUT" | cut -f1))"
