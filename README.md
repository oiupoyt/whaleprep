# PYQ clone

A working full-stack copy of the ZenithPrep PYQ practice app: build a paper from
JEE Main style questions, take it on a timed CBT screen with +4 / −1 marking,
then read the analysis and progress dashboard.

Rust + Axum + SQLite. Server-rendered HTML, vanilla JS, no frontend framework.

## Run

    cargo run

Then open http://127.0.0.1:8080 and sign in with any name and email.

Environment:

| var        | default            | meaning                      |
|------------|--------------------|------------------------------|
| `PYQ_ADDR` | `127.0.0.1:8080`  | listen address               |
| `PYQ_DB`   | `pyq.db`           | SQLite file, created if absent |
| `PYQ_BASE` | empty              | URL prefix when mounted in a subpath |
| `GOOGLE_CLIENT_ID` | empty    | enables Google sign-in; unset hides the button |
| `GOOGLE_CLIENT_SECRET` | empty | needed only by the full-page fallback |

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
| `GET /build`, `POST /build`   | chapter picker and paper generation        |
| `POST /attempt/:paper`    | starts or resumes an attempt                 |
| `GET /attempt/:paper`     | the CBT screen                                |
| `POST /attempt/:id/answer` | JSON autosave of one answer                  |
| `POST /attempt/:id/submit` | grades and closes the paper                  |
| `GET /result/:attempt`    | score, accuracy and per-question analysis     |
| `GET /history`            | every submitted paper                         |
| `GET /dashboard`          | cross-paper trend, weak subtopics, coach notes |
| `GET /s/:token`           | public paper view, never a score              |
| `GET /api/attempt/:id/state` | autosaved answers as JSON                 |

## Marking

+4 for a correct answer, −1 for a wrong one, 0 for a blank. Numerical answers
compare against a 0.1 percent tolerance so `20.01` is accepted for `20`.

## Question selection

A paper is drawn from the chosen chapters, shuffled with a seed derived from the
request, and trimmed to the requested length. With "unseen questions only" the
query excludes anything already answered by that user, so the same question does
not come back by accident.

## Tests

    cargo test

Covers the marking rules, numerical tolerance, and the urlencoded form decoding
that shapes the build endpoint.
