#!/usr/bin/env python3
"""Estimates disk and index cost at the real bank's scale.

Builds a throwaway database holding 10,905 questions plus simulated student
activity, then reports the size on disk and the query timings that decide
whether SQLite is still the right store.
    scripts/bench_scale.py [db-path]
"""
import os
import random
import sqlite3
import sys
import time

DB = sys.argv[1] if len(sys.argv) > 1 else "/tmp/scale.db"
QUESTIONS = 10905
SUBTOPICS = 349
STUDENTS = 2000
PAPERS_PER_STUDENT = 6
QUESTIONS_PER_PAPER = 75

for suffix in ("", "-wal", "-shm"):
    try:
        os.remove(DB + suffix)
    except FileNotFoundError:
        pass

con = sqlite3.connect(DB)
con.executescript(
    """
    PRAGMA journal_mode=WAL;
    CREATE TABLE users(id TEXT PRIMARY KEY, name TEXT, email TEXT UNIQUE, created_at TEXT);
    CREATE TABLE chapters(id INTEGER PRIMARY KEY, subject TEXT, name TEXT);
    CREATE TABLE subtopics(id INTEGER PRIMARY KEY, chapter_id INTEGER, name TEXT);
    CREATE TABLE questions(id INTEGER PRIMARY KEY, subject TEXT, chapter_id INTEGER,
        subtopic_id INTEGER, qtype TEXT, stem TEXT, opt_a TEXT, opt_b TEXT, opt_c TEXT,
        opt_d TEXT, answer TEXT, difficulty TEXT, year INTEGER, shift TEXT, solution TEXT);
    CREATE TABLE papers(id TEXT PRIMARY KEY, user_id TEXT, title TEXT,
        duration_min INTEGER, created_at TEXT, share_token TEXT);
    CREATE TABLE paper_questions(paper_id TEXT, question_id INTEGER, ord INTEGER,
        PRIMARY KEY(paper_id, question_id));
    CREATE TABLE attempts(id TEXT PRIMARY KEY, paper_id TEXT, user_id TEXT,
        started_at TEXT, submitted_at TEXT, score INTEGER, max_score INTEGER, used_secs INTEGER);
    CREATE TABLE answers(attempt_id TEXT, question_id INTEGER, selected TEXT,
        marked_review INTEGER, time_secs INTEGER, PRIMARY KEY(attempt_id, question_id));
    CREATE TABLE sessions(token TEXT PRIMARY KEY, user_id TEXT, created_at TEXT);
    CREATE TABLE seen_questions(user_id TEXT, question_id INTEGER,
        PRIMARY KEY(user_id, question_id));
    CREATE TABLE mistake_tags(attempt_id TEXT, question_id INTEGER, tag TEXT,
        PRIMARY KEY(attempt_id, question_id, tag));
    """
)

rng = random.Random(7)
stem = ("A particle of mass 2 kg moves along the x axis under a restoring force "
        "F = -kx. If k = 200 N/m the time period of small oscillations is:")
opt = "A 0.1 s" * 4

t0 = time.perf_counter()
con.executemany("INSERT INTO chapters VALUES(?,?,?)",
                [(i, ["Physics", "Chemistry", "Maths"][i % 3], f"Chapter {i}") for i in range(1, 28)])
con.executemany("INSERT INTO subtopics VALUES(?,?,?)",
                [(i, i % 27 + 1, f"Subtopic {i}") for i in range(1, SUBTOPICS + 1)])
con.executemany(
    "INSERT INTO questions VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)",
    [(i, ["Physics", "Chemistry", "Maths"][i % 3], i % 27 + 1, i % SUBTOPICS + 1,
      "MCQ" if i % 4 else "NUM", stem, opt, opt, opt, opt, "ABCD"[i % 4],
      ["Easy", "Medium", "Hard"][i % 3], 2020 + i % 7, f"S{i % 4 + 1}", stem)
     for i in range(1, QUESTIONS + 1)])
con.commit()
print(f"seeded {QUESTIONS:,} questions in {time.perf_counter()-t0:.1f}s")

# Indexes the app would need for subtopic filtering and per-user lookups.
t0 = time.perf_counter()
con.executescript(
    """
    CREATE INDEX q_chapter ON questions(chapter_id);
    CREATE INDEX q_subtopic ON questions(subtopic_id);
    CREATE INDEX pq_question ON paper_questions(question_id);
    CREATE INDEX a_paper ON attempts(paper_id);
    CREATE INDEX a_user ON attempts(user_id, submitted_at);
    CREATE INDEX ans_question ON answers(question_id);
    CREATE INDEX seen_q ON seen_questions(question_id);
    """
)
con.commit()
print(f"indexes built in {time.perf_counter()-t0:.1f}s")

t0 = time.perf_counter()
papers = STUDENTS * PAPERS_PER_STUDENT
for s in range(STUDENTS):
    uid = f"user-{s}"
    con.execute("INSERT INTO users VALUES(?,?,?,?)", (uid, f"Student {s}", f"s{s}@x.com", "2026-01-01"))
    for _ in range(PAPERS_PER_STUDENT):
        pid, aid = f"paper-{s}-{_}", f"att-{s}-{_}"
        con.execute("INSERT INTO papers VALUES(?,?,?,?,?,?)",
                    (pid, uid, "Paper", 180, "2026-01-01", f"tok{pid}"))
        picks = rng.sample(range(1, QUESTIONS + 1), QUESTIONS_PER_PAPER)
        con.executemany("INSERT INTO paper_questions VALUES(?,?,?)",
                        [(pid, q, n) for n, q in enumerate(picks)])
        con.execute("INSERT INTO attempts VALUES(?,?,?,?,?,?,?,?)",
                    (aid, pid, uid, "2026-01-01", "2026-01-01", 180, 300, 5400))
        con.executemany("INSERT INTO answers VALUES(?,?,?,?,?)",
                        [(aid, q, "ABCD"[q % 4], q % 5 == 0, 40 + q % 60) for q in picks])
        # A question already answered on an earlier paper is not "new" for this student.
        con.executemany("INSERT OR IGNORE INTO seen_questions VALUES(?,?)",
                        [(uid, q) for q in picks])
con.commit()
elapsed = time.perf_counter() - t0
print(f"seeded {STUDENTS:,} students / {papers:,} papers in {elapsed:.1f}s")
con.execute("VACUUM")
con.commit()
con.close()


def mb(path):
    total = 0
    for suffix in ("", "-wal", "-shm"):
        try:
            total += os.path.getsize(path + suffix)
        except FileNotFoundError:
            pass
    return total / 1024 / 1024


db_mb = mb(DB)
print(f"\n=== {DB} ===")
print(f"  questions        {QUESTIONS:>9,}")
print(f"  students         {STUDENTS:>9,}")
print(f"  papers           {papers:>9,}")
print(f"  answer rows      {papers * QUESTIONS_PER_PAPER:>9,}")
print(f"  on disk          {db_mb:>9.1f} MB")
print(f"  per paper        {db_mb * 1024 / papers:.1f} KB")
print(f"  per student      {db_mb * 1024 / STUDENTS:.1f} KB")

con = sqlite3.connect(f"file:{DB}?mode=ro", uri=True)
print("\nquery timings at this size")
queries = {
    "paper builder (subtopic filter)":
        "SELECT q.id FROM questions q WHERE q.subtopic_id IN (?, ?) AND q.chapter_id IN (?, ?) "
        "AND q.id NOT IN (SELECT question_id FROM seen_questions WHERE user_id=?) LIMIT 75",
    "seen-question exclusion (450 rows)":
        "SELECT q.id FROM questions q WHERE q.id NOT IN "
        "(SELECT question_id FROM seen_questions WHERE user_id=?)",
    "dashboard topic rollup":
        "SELECT c.name, s.name, COUNT(*) FROM answers a JOIN attempts at ON at.id=a.attempt_id "
        "JOIN questions q ON q.id=a.question_id JOIN chapters c ON c.id=q.chapter_id "
        "JOIN subtopics s ON s.id=q.subtopic_id WHERE at.user_id=? GROUP BY c.name, s.name",
    "history list":
        "SELECT a.id, a.score FROM attempts a WHERE a.user_id=? ORDER BY a.started_at DESC",
    "mistake-tag rollup":
        "SELECT tag, COUNT(*) FROM mistake_tags WHERE attempt_id IN "
        "(SELECT id FROM attempts WHERE user_id=?) GROUP BY tag",
}
uid = "user-3"
topics = ",".join(str(i) for i in range(1, 31))
chapters = ",".join(str(i) for i in range(1, 10))
for label, q in queries.items():
    # Bind exactly what each statement declares, in order.
    n_list = q.count("?") - 1
    params = [int(i) for i in topics.split(",")][:n_list] + [uid]
    t0 = time.perf_counter()
    rows = con.execute(q, tuple(params)).fetchall()
    print(f"  {label:<34} {(time.perf_counter()-t0)*1000:7.1f} ms  ({len(rows)} rows)")
