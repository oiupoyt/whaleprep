#!/usr/bin/env bash
# Stop WhalePrep. Usage: ./deploy/termux-stop.sh   (run from repo root)
cd "$(dirname "$0")/.."
[ -f ./whaleprep.pid ] && PID=$(cat ./whaleprep.pid) && kill "$PID" 2>/dev/null && rm -f ./whaleprep.pid && echo "stopped $PID" || echo "not running"
