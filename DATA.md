# Data sourcing — JEE Main question bank

## Status

**`scripts/import_examgoal.py` imports 9,813 real JEE Main PYQs** (2002–2025,
all three subjects) from the MIT-licensed
[jee_mains_pyqs_data_base](https://github.com/HostServer001/jee_mains_pyqs_data_base)
release asset (`1762787474-DataBaseChapters-v007.pkl`, ~24 MB, 14,973 questions
total). The keep rule is: type=mcq, fully text (no `<img>` anywhere in
stem/options/explanation), exactly one correct option in A–D. Skipped: 1,909
image-questions, 2,875 answer-less, 19 multi-correct, ~357 with stray images
in options/explanations.

```
python3 scripts/import_examgoal.py path/to/app.db [--pkl db.pkl]
```

Safe to re-run: INSERTs skip stems already present, so the 24 hand-written
seed questions are never duplicated. Existing deployments pick up the new
`CREATE INDEX` statements on boot (`init_db` runs them idempotently).

The demo seed (`src/db.rs`, 24 hand-written questions) remains as the offline
fallback when the pickle cannot be fetched. `scripts/expand_seed.py` is kept
for synthetic load testing only.

## Attribution

Question content: [HostServer001/jee_mains_pyqs_data_base](https://github.com/HostServer001/jee_mains_pyqs_data_base)
(MIT), itself reverse-engineered from a subscription site's API and cached.
Upstream model credit: examgoal database schema. No NTA PDFs were scraped —
NTA publishes image-rendered PDFs with no machine-readable question text
(`pdftotext` yields only question IDs).

## Schema mapping

The bank is organized into the official NTA JEE Main syllabus units
(2026 Paper 1 B.E./B.Tech):

- Physics: 20 units (Units and Measurements through Electronic Devices)
- Chemistry: 21 units (Some Basic Concepts through Practical Chemistry)
- Maths: 14 units (Sets, Relations and Functions through Trigonometry)

`scripts/reorg_nta_units.py` maps upstream subtopics into these units by
keyword matching; stragglers are placed manually (documented in the script).

The 24 hand-written demo questions that previously shipped inside the seed
have been removed from the live bank: they were trivial one-liners that
diluted difficulty. A fresh install still seeds them so the app is not
empty before import (see src/db.rs), but they are labeled in the seed code
as demo content.

## Sanitizer contract (both sides match)

Write side (`import_examgoal.py::sanitize`) and read side
(`views.rs::rich`) share one allowlist: `br p sup sub b strong i em u ol ul
li table tbody thead tr th td span h3`, attributes always stripped,
`<picture>/<source>/<style>/<col>` blocks dropped whole. KaTeX renders
`$$..$$` display math and `\(..\)` inline at page load with `trust: false`.

## Adding more questions later

Re-run the importer against a newer pickle release. For NUM questions the
upstream data carries no answers, so they are excluded until a verified
answer source exists. The `answer` column for MCQ is a single letter.

## Why NUM answers are not extracted from explanations

Audited 2026-10-12: integer-question explanations do end with the numeric
result (`= 80`, `Total 11`, `is 8`), but the phrasing varies per question
(`Hence`, `Clearly`, `no. of integers in S is`, bare `= X` mid-derivation)
and several tails contain multiple numbers. A regex would mis-key a
meaningful fraction, and a wrong answer key teaches the wrong result —
worse than no question. NUM import stays out until NTA final answer keys
(authoritative per-shift source) are transcribed. The grader already
supports NUM with 0.1% tolerance for hand-added questions.
