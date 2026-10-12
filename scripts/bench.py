#!/usr/bin/env python3
"""Load check for the PYQ server.

    scripts/bench.py <base-url> [requests] [concurrency]

Reports latency percentiles per route and sustained throughput under load.
"""
import http.cookiejar
import statistics
import sys
import time
import urllib.request
from concurrent.futures import ThreadPoolExecutor

BASE = sys.argv[1] if len(sys.argv) > 1 else "http://127.0.0.1:8171"
N = int(sys.argv[2]) if len(sys.argv) > 2 else 300
CONC = int(sys.argv[3]) if len(sys.argv) > 3 else 20

jar = http.cookiejar.CookieJar()
opener = urllib.request.build_opener(urllib.request.HTTPCookieProcessor(jar))
opener.addheaders = [("User-Agent", "Mozilla/5.0 (X11; Linux x86_64) WhalePrep-bench/1.0")]


def sign_in():
    body = b"name=Bench&email=bench@example.com&next=/build"
    req = urllib.request.Request(f"{BASE}/student", data=body, method="POST")
    opener.open(req).read()


def timed(path):
    t0 = time.perf_counter()
    with opener.open(BASE + path) as r:
        r.read()
    return (time.perf_counter() - t0) * 1000


def percentile(values, p):
    s = sorted(values)
    return s[min(int(len(s) * p), len(s) - 1)]


def report(path, samples):
    print(
        f"  {path:<22} n={len(samples):<5} "
        f"p50={statistics.median(samples):6.2f}ms  "
        f"p95={percentile(samples, .95):6.2f}ms  "
        f"max={max(samples):6.2f}ms"
    )


def load(path, concurrency):
    t0 = time.perf_counter()
    with ThreadPoolExecutor(max_workers=concurrency) as pool:
        list(pool.map(lambda _: timed(path), range(N)))
    return N / (time.perf_counter() - t0)


sign_in()
print(f"=== {BASE} | {N} requests | {CONC} parallel ===")
print("sequential latency")
for p in ["/", "/static/style.css", "/build", "/dashboard", "/history"]:
    report(p, [timed(p) for _ in range(min(N, 50))])

print(f"throughput at {CONC} concurrent")
for p in ["/", "/build"]:
    warm = [timed(p) for _ in range(CONC * 2)]
    rps = load(p, CONC)
    report(p, warm)
    print(f"  -> {p:<20} {rps:,.0f} req/s sustained")