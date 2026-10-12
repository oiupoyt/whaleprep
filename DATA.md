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

| pickle field | our column |
|---|---|
| `chapterGroup` (11 groups) | `chapters.name` |
| `chapter` (91 slugs) | `subtopics.name` |
| `topic` (472 slugs) | folded into subtopic name when distinct |
| `correct_options: ["X"]` | `answer` |
| `question.en.content` (HTML+`$$`) | `stem` (sanitized) |
| `options[].content` | `opt_a..opt_d` (sanitized) |
| `explanation` | `solution` (sanitized) |
| `paperTitle` Morning/Evening | `shift` S1/S2 (examDate hour as fallback) |
| `difficulty` easy/medium/hard | `Easy`/`Medium`/`Hard` |

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
