# WhalePrep

JEE Main PYQ practice: build a paper from 9,813 real previous-year questions
(2002–2025, Physics/Chemistry/Maths), take it on a timed CBT screen with +4 /
−1 marking and per-subject clocks, tag why you lost each question, then read
the analysis and progress dashboard.

Rust + Axum + SQLite. Server-rendered HTML, KaTeX math, vanilla JS, no
frontend framework.

## Run

    cargo run --release

Then open http://127.0.0.1:8080 and sign in with any name and email.

Environment:

| var        | default            | meaning                      |
|------------|--------------------|------------------------------|
| `PYQ_ADDR` | `127.0.0.1:8080`  | listen address               |
| `PYQ_DB`   | `pyq.db`           | SQLite file, created if absent |
| `PYQ_BASE` | empty              | URL prefix when mounted in a subpath |
| `GOOGLE_CLIENT_ID` | empty    | enables Google sign-in; unset hides the button |
| `GOOGLE_CLIENT_SECRET` | empty | needed only by the full-page fallback |

## Question bank

A fresh database seeds 24 hand-written questions so the app works offline.
For the real bank:

    python3 scripts/import_examgoal.py pyq.db

This pulls the MIT-licensed 14,973-question pickle and imports the 9,813
fully-text MCQs with verified single answers. Idempotent — safe to re-run.
See [DATA.md](DATA.md) for the source, the keep rule, and the schema mapping.

## Google sign-in

Set a client id and the button appears on `/student`. Without it the page says
Google is off and offers the email form instead.

    GOOGLE_CLIENT_ID=1234-ab.apps.googleusercontent.com \
    GOOGLE_CLIENT_SECRET=... \
    cargo run

In the [Google Cloud console](https://console.cloud.google.com/apis/credentials)
add an **OAuth 2.0 Client ID / Web application**, then list every origin this runs
on, for example `http://127.0.0.1:8080` and `http://localhost:8080`. The callback
URL is derived from the incoming request, so it follows the port you are on and
follows `x-forwarded-proto` behind a proxy. Register each of those origins rather
than trying to predict one URL.

Two paths lead to the same session:

- **Popup.** Google Identity Services returns an ID token to the page, which posts
  it to `POST /student/google`.
- **Full page.** Where a popup is refused, the link beside the button starts an
  authorization-code flow with PKCE. The code is redeemed server side at
  `GET /student/google/callback`, so the client secret never reaches the browser.

Either way the ID token is verified here, not in the browser. The signature is
checked against the keys Google publishes at
`https://www.googleapis.com/oauth2/v3/certs`, and the audience must be this client,
the issuer must be a Google issuer, the expiry must hold, and the email must be
verified. A token failing any of that is rejected and no session is created. The
`state` parameter is single use and expires after ten minutes.

To confirm the rejection path without a real Google account:

    cargo run --example forge_google_token -- YOUR_CLIENT_ID attacker@example.com

That prints a correctly signed token from a key the server has never seen.
Posting it to `/student/google` returns 401 and creates no user.

## Routes

| route                     | what it does                                  |
|---------------------------|-----------------------------------------------|
| `GET /`                   | landing page with live bank counts             |
| `GET /student`, `POST /student` | email sign-in, sets an HttpOnly session cookie |
| `POST /student/google` | verifies a Google ID token, signs in |
| `GET /student/google/callback` | redeems the authorization code for the full-page flow |
| `GET /logout`             | clears the session                            |
| `GET /build`, `POST /build`   | chapter/subtopic picker and paper generation |
| `POST /attempt/:paper`    | starts or resumes an attempt                 |
| `GET /attempt/:paper`     | the CBT screen                                |
| `POST /attempt/:id/answer` | JSON autosave of one answer                  |
| `POST /attempt/:id/submit` | grades and closes the paper                  |
| `GET /result/:attempt`    | score, accuracy and per-question analysis     |
| `POST /result/:attempt/tag/:qid` | toggles a why-lost tag on one question |
| `GET /paper/:id`          | owner shortcut to the attempt screen          |
| `GET /history`            | every submitted paper                         |
| `GET /dashboard`          | trend, weak subtopics, mistake rollup, coach notes |
| `GET /s/:token`           | public paper view with take-this-paper CTA     |
| `GET /api/attempt/:id/state` | autosaved answers as JSON                 |

## Marking

+4 for a correct answer, −1 for a wrong one, 0 for a blank. Numerical answers
compare against a 0.1 percent tolerance so `20.01` is accepted for `20`.

## Question selection

A paper is drawn from the chosen chapters or subtopics, shuffled with a seed
derived from the request, and trimmed to the requested length. With "unseen
questions only" the query excludes anything already answered by that user; once
a subtopic is exhausted, the longest-ago-seen questions recycle first instead
of erroring. JEE mode sets 60 minutes per subject with independent countdown
strips that auto-advance.

## Measured performance (9,837-question bank, local loopback)

| metric | value |
|---|---|
| release binary | 17 MB |
| populated DB | 12.8 MB |
| resident memory | ~24 MB |
| `/build` latency | p50 3 ms, p95 8 ms |
| `/build` throughput | ~940 req/s at 20 concurrent |
| autosave writes | ~470/s, p95 23 ms |
| import time | ~10 s first run, ~3 s re-run |

Recommended floor: **1 vCPU / 512 MB RAM / 5 GB disk**. Comfortable headroom:
**1 vCPU / 1 GB RAM / 10 GB SSD**. No Postgres, no Redis, no object storage —
one static binary plus one SQLite file.

## Deploy (Termux + Cloudflare Tunnel)

The live site runs on an ARM tablet via Termux behind a dedicated tunnel:

    # on the host
    pkg install rust sqlite python git
    git clone https://git.oiupoyt.space/oiupoyt/whaleprep.git
    cd whaleprep && cargo build --release
    ./deploy/termux-start.sh
    python3 scripts/import_examgoal.py whaleprep.db   # 9,813 real PYQs

    # tunnel (once): cloudflared tunnel create whaleprep, point the CNAME at it
    cloudflared tunnel --config ~/.cloudflared/whaleprep.yml \
      run ffc0df8c-0978-4b54-93a1-57c64fa1464c

See [deploy/](deploy/) for the service scripts and tunnel config.
Live at: https://whaleprep.oiupoyt.space

## Tests

    cargo test

Covers the marking rules, numerical tolerance, and the urlencoded form decoding
that shapes the build endpoint.
