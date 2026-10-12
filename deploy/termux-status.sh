#!/usr/bin/env bash
# WhalePrep status + health ping. Usage: ./deploy/termux-status.sh
cd "$(dirname "$0")/.."
if [ -f ./whaleprep.pid ] && kill -0 "$(cat ./whaleprep.pid)" 2>/dev/null; then
    PID=$(cat ./whaleprep.pid)
    echo "RUNNING (PID $PID, RSS $(( $(ps -o rss= -p $PID 2>/dev/null || echo 0) / 1024 )) MB)"
    curl -s -o /dev/null -w "health=%{http_code}\n" http://127.0.0.1:8222/ 2>&1 || echo "port not responding"
else
    echo "NOT running"
fi
