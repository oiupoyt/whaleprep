#!/usr/bin/env bash
# Daily SQLite backup: VACUUM INTO a timestamped copy, keep 7 days.
# Usage: ./deploy/termux-backup.sh  (run from repo root; safe while live)
# Termux has no cron, so run this from Termux:Tasker, a daily alarm via
# Termux:API, or manually. One line, one snapshot.
# Resolve symlinks first: this script may be invoked as ./backup.sh (a
# symlink at repo root), in which case plain dirname would escape the repo
# and sqlite would create an empty DB in the wrong place.
HERE="$(dirname "$(readlink -f "$0")")"
if [ "$(basename "$HERE")" = "deploy" ]; then
  cd "$HERE/.."
else
  cd "$HERE"
fi
[ -f ./whaleprep.db ] || { echo "no ./whaleprep.db here ($(pwd)) — aborting"; exit 1; }
OUT="./backups/whaleprep-$(date +%F).db"
mkdir -p ./backups
# VACUUM INTO refuses to overwrite, so remove today's snapshot first —
# otherwise re-runs silently keep stale data.
rm -f "$OUT"
sqlite3 ./whaleprep.db "VACUUM INTO '$OUT';"
ls -t ./backups/whaleprep-*.db | tail -n +8 | xargs -r rm -f
echo "backup -> $OUT ($(du -h "$OUT" | cut -f1))"
