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

## Routes

| route                     | what it does                                  |
|---------------------------|-----------------------------------------------|
| `GET /`                   | landing page with live bank counts             |
| `GET /student`, `POST /student` | local sign-in, sets an HttpOnly session cookie |
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
