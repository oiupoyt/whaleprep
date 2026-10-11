# Data sourcing — JEE Main question bank

## Status

The demo seed contains **24 hand-written questions** spread across 9 chapters and 19 subtopics. `scripts/expand_seed.py` inflates this to 500+ synthetic questions so the app can be demonstrated at scale; those questions have real syllabus metadata but sparse placeholder stems.

## Why there is no real scraper

NTA publishes every JEE Main question paper at `jeemain.nta.nic.in/question-papers/`. The files are multi-MB PDFs. The problem:

**Every NTA question paper PDF renders questions as images, not text.** Running `pdftotext` extracts only question IDs and section metadata — no stems, no options. This is intentional (anti-copy protection). Every aggregator site (Embibe, Allen, Aakash, MathonGo) that has structured data built it through manual data-entry pipelines over years.

```
$ pdftotext "B Tech 2nd Apr 2026 Shift 1.pdf" - | head -30
Question Number : 1 Question Id : 6911211 Question Type : MCQ ...
Options :
6911211.       ← this is just the option ID, not the text
6911212.
```

## Realistic paths to a real bank

| Method | Effort | Quality |
|---|---|---|
| MathonGo / Embibe API | Paid (₹ ~5–20k/yr) | Verified, tagged, 10k+ questions |
| Allen/Aakash raw exports | Partnership required | Best tagging, solutions included |
| Manual entry from NTA PDFs | ~2 min/question → 350 person-hours for 10k | Accurate, controllable |
| OCR (Tesseract) on NTA PDFs | Medium effort, high error rate on math | Needs human review pass |
| Scrape a coaching site | Legally grey, brittle to layout changes | Varies |

## What `scripts/expand_seed.py` does

Generates synthetic questions using the real JEE Main syllabus taxonomy (15 chapters, 45 subtopics, correct MCQ/NUM split, correct difficulty distribution per topic). The stems are deliberately sparse (`[Phy/Kinematic/2024S1] Practice question 42: Kinematics concept application.`) so no-one mistakes them for NTA content.

Run with `--count N` to produce N additional questions:
```
python3 scripts/expand_seed.py path/to/app.db --count 1000
```

## Adding real questions

The schema expects:
```sql
INSERT INTO questions(subject, chapter_id, subtopic_id, qtype,
  stem, opt_a, opt_b, opt_c, opt_d, answer,
  difficulty, year, shift, solution)
VALUES(...)
```

`chapter_id` and `subtopic_id` must exist in `chapters` and `subtopics` first. The expand script shows how to upsert those. For NUM questions set `opt_a..opt_d` to empty strings and `answer` to the numeric string (e.g. `"20.00"`).
