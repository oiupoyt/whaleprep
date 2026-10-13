#!/usr/bin/env python3
"""Write-path check: many concurrent students autosaving and submitting at once.

SQLite serialises writers, so this is where the ceiling actually lives.
    scripts/bench_write.py <base-url> <students> <questions>
"""
import http.cookiejar
import sqlite3
import statistics
import sys
import time
import urllib.parse
import urllib.request
from concurrent.futures import ThreadPoolExecutor

BASE = sys.argv[1] if len(sys.argv) > 1 else "http://127.0.0.1:8171"
STUDENTS = int(sys.argv[2]) if len(sys.argv) > 2 else 20
QUESTIONS = int(sys.argv[3]) if len(sys.argv) > 3 else 12
DB = sys.argv[4] if len(sys.argv) > 4 else "/tmp/bench.db"


def student_session(i):
    jar = http.cookiejar.CookieJar()
    opener = urllib.request.build_opener(urllib.request.HTTPCookieProcessor(jar))
    opener.addheaders = [("User-Agent", "Mozilla/5.0 (X11; Linux x86_64) WhalePrep-bench/1.0")]
    body = urllib.parse.urlencode(
        {"name": f"Load{i}", "email": f"load{i}@example.com", "next": "/build"}
    ).encode()
    opener.open(urllib.request.Request(f"{BASE}/student", data=body, method="POST")).read()
    return opener


def run(i):
    opener = student_session(i)
    form = urllib.parse.urlencode(
        {"chapters": "1,2,3,4,5,6,7,8,9", "count": QUESTIONS, "duration": 24, "unseen_only": "on"}
    ).encode()
    r = opener.open(urllib.request.Request(f"{BASE}/build", data=form, method="POST"))
    paper = r.geturl().rsplit("/", 1)[-1].split("?")[0]

    r = opener.open(urllib.request.Request(f"{BASE}/attempt/{paper}", data=b"", method="POST"))
    attempt = r.geturl().split("run=")[-1]

    import json as _json
    # Question ids via the state API so this works against any deployment
    # (no local DB access needed).
    state_raw = opener.open(
        urllib.request.Request(f"{BASE}/api/attempt/{attempt}/state")
    ).read()
    ids = [a["question_id"] for a in _json.loads(state_raw)["answers"]]

    # Autosave each answer, which is what the exam screen does while a student works.
    saves = []
    for q in ids:
        payload = b'{"answers":[{"question_id":%d,"selected":"A","marked_review":false,"time_secs":30}]}' % q
        t0 = time.perf_counter()
        opener.open(
            urllib.request.Request(
                f"{BASE}/attempt/{attempt}/answer", data=payload,
                headers={"content-type": "application/json"}, method="POST",
            )
        ).read()
        saves.append((time.perf_counter() - t0) * 1000)

    t0 = time.perf_counter()
    opener.open(urllib.request.Request(f"{BASE}/attempt/{attempt}/submit", data=b"", method="POST")).read()
    submit_ms = (time.perf_counter() - t0) * 1000
    return saves, submit_ms


t0 = time.perf_counter()
with ThreadPoolExecutor(max_workers=STUDENTS) as pool:
    results = list(pool.map(run, range(STUDENTS)))
elapsed = time.perf_counter() - t0

saves = [ms for r, _ in results for ms in r]
submits = [ms for _, ms in results]
saves.sort()
total_saves = len(saves)

print(f"=== {STUDENTS} concurrent students x {QUESTIONS} questions ===")
print(f"  wall clock        {elapsed:.2f}s")
print(f"  autosaves         {total_saves} in {elapsed:.2f}s = {total_saves/elapsed:,.0f} writes/s")
print(f"  autosave latency  p50={statistics.median(saves):.1f}ms  "
      f"p95={saves[int(len(saves)*.95)]:.1f}ms  max={saves[-1]:.1f}ms")
print(f"  submit latency    p50={statistics.median(submits):.1f}ms  max={max(submits):.1f}ms")