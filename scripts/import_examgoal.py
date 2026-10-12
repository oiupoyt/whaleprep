#!/usr/bin/env python3
"""Import real JEE Main PYQs from the HostServer001 examgoal-derived database.

Source: https://github.com/HostServer001/jee_mains_pyqs_data_base (MIT)
Release asset: 1762787474-DataBaseChapters-v007.pkl (~24 MB, 14,973 questions).

What this does:
  1. Downloads the pickle (or uses --pkl if you already have it).
  2. Keeps only: type=mcq, fully text (no <img anywhere in stem/options/
     explanation), exactly one correct option in A-D.
  3. Sanitizes HTML to a safe allowlist, unwraps $$..$$ math for KaTeX.
  4. Maps chapterGroup -> our chapter, chapter -> our subtopic, creating
     rows as needed. Keeps the existing 24 seed questions untouched
     (dedupes on exact stem match).
  5. Shifts: Morning Slot/Shift -> S1, Evening -> S2, Offline/AIEEE -> S1.

Usage:
  python3 scripts/import_examgoal.py path/to/app.db [--pkl db.pkl]

Safe to re-run: INSERTs skip stems that already exist.
"""
import argparse, html, pickle, re, sqlite3, sys, urllib.request
from pathlib import Path

PKL_URL = ("https://github.com/HostServer001/jee_mains_pyqs_data_base"
           "/releases/download/v007/1762787474-DataBaseChapters-v007.pkl")

ALLOWED = {"br", "p", "sup", "sub", "b", "strong", "i", "em", "u",
           "ol", "ul", "li", "table", "tbody", "thead", "tr", "th", "td",
           "span", "h3"}

SUBJECT_FIX = {"mathematics": "Maths", "physics": "Physics", "chemistry": "Chemistry"}


def title_case(slug: str) -> str:
    return " ".join(w.capitalize() for w in slug.replace("-", " ").split())


def sanitize(raw: str) -> str:
    """Strip to safe HTML. Unescape entities first so &lt; renders as <. KaTeX
    handles $$..$$ at render time; nothing else needs dollar-escaping."""
    if not raw:
        return ""
    text = html.unescape(str(raw))

    # Drop whole <picture>/<source>/<style> blocks (responsive image variants).
    text = re.sub(r"<picture.*?</picture>", "", text, flags=re.S | re.I)
    text = re.sub(r"<source[^>]*>", "", text, flags=re.I)
    text = re.sub(r"<style.*?</style>", "", text, flags=re.S | re.I)
    text = re.sub(r"<colgroup.*?</colgroup>", "", text, flags=re.S | re.I)
    text = re.sub(r"</?(?:col|colgroup)[^>]*>", "", text, flags=re.I)

    # Keep only allowlisted tags, drop all attributes.
    def tag(m):
        closing, name = m.group(1), m.group(2).lower()
        if name in ALLOWED:
            return f"<{closing}{name}>"
        return ""

    text = re.sub(r"<(/?)([a-zA-Z][a-zA-Z0-9]*)(\s[^>]*)?>", tag, text)
    # Collapse whitespace runs but keep single spaces inside math.
    text = re.sub(r"[ \t\xa0]+", " ", text)
    text = re.sub(r"\n\s*\n+", "\n", text)
    return text.strip()


def shift_of(paper_title: str, exam_date) -> str:
    t = (paper_title or "").lower()
    if "evening" in t:
        return "S2"
    if "morning" in t:
        return "S1"
    if exam_date:
        try:
            hour = int(str(exam_date).split()[1].split(":")[0])
            return "S2" if hour >= 12 else "S1"
        except (IndexError, ValueError):
            pass
    return "S1"


def main() -> None:
    ap = argparse.ArgumentParser()
    ap.add_argument("db")
    ap.add_argument("--pkl", default=None)
    args = ap.parse_args()

    pkl_path = Path(args.pkl) if args.pkl else Path("/tmp/pyqeval/db.pkl")
    if not pkl_path.exists():
        print(f"downloading {PKL_URL} ...", flush=True)
        pkl_path.parent.mkdir(parents=True, exist_ok=True)
        urllib.request.urlretrieve(PKL_URL, pkl_path)
    print(f"loading {pkl_path} ({pkl_path.stat().st_size / 1e6:.1f} MB) ...", flush=True)

    sys.path.insert(0, "/tmp/pyqeval")
    from pkg.core.question import Question
    from pkg.core.chapter import Chapter

    class FixUnpickler(pickle.Unpickler):
        def find_class(self, module, name):
            if name == "Question":
                return Question
            if name == "Chapter":
                return Chapter
            return super().find_class(module, name)

    with open(pkl_path, "rb") as f:
        data = FixUnpickler(f).load()

    con = sqlite3.connect(args.db)
    existing = {r[0] for r in con.execute("SELECT stem FROM questions")}
    print(f"existing stems in db: {len(existing)}")

    chap_cache: dict = {}
    sub_cache: dict = {}

    def chapter_id(subject: str, chapter: str) -> int:
        key = (subject, chapter)
        if key not in chap_cache:
            row = con.execute(
                "SELECT id FROM chapters WHERE subject=? AND name=?",
                (subject, chapter)).fetchone()
            chap_cache[key] = (row[0] if row
                               else con.execute(
                                   "INSERT INTO chapters(subject,name) VALUES(?,?)",
                                   (subject, chapter)).lastrowid)
        return chap_cache[key]

    def subtopic_id(ch_id: int, sub: str) -> int:
        key = (ch_id, sub)
        if key not in sub_cache:
            row = con.execute(
                "SELECT id FROM subtopics WHERE chapter_id=? AND name=?",
                (ch_id, sub)).fetchone()
            sub_cache[key] = (row[0] if row
                              else con.execute(
                                  "INSERT INTO subtopics(chapter_id,name) VALUES(?,?)",
                                  (ch_id, sub)).lastrowid)
        return sub_cache[key]

    kept = skipped = 0
    diff_map = {"easy": "Easy", "medium": "Medium", "hard": "Hard"}
    for _key, chap in data.items():
        for q in chap.question_dict.values():
            if getattr(q, "type", None) != "mcq":
                skipped += 1
                continue
            if getattr(q, "isImgQuestion", False):
                skipped += 1
                continue
            co = getattr(q, "correct_options", None) or []
            if len(co) != 1 or co[0] not in ("A", "B", "C", "D"):
                skipped += 1
                continue
            stem = sanitize(getattr(q, "question", ""))
            if not stem or "<img" in stem:
                skipped += 1
                continue
            opt_contents = []
            bad = False
            for opt in (getattr(q, "options", None) or []):
                c = sanitize(opt.get("content", "") if isinstance(opt, dict) else "")
                if not c or "<img" in c:
                    bad = True
                    break
                opt_contents.append(c)
            if bad or len(opt_contents) != 4:
                skipped += 1
                continue
            solution = sanitize(getattr(q, "explanation", ""))
            if "<img" in solution:
                skipped += 1
                continue
            if stem in existing:
                skipped += 1
                continue

            subject = SUBJECT_FIX.get(
                str(getattr(chap, "parent_subject", "")).lower(),
                str(getattr(chap, "parent_subject", "")).title())
            chapter = title_case(str(getattr(q, "chpaterGroup", None)
                                     or getattr(q, "chapter", "General")))
            subtopic = title_case(str(getattr(q, "topic", None)
                                      or getattr(q, "chapter", "General")))
            ch_id = chapter_id(subject, chapter)
            st_id = subtopic_id(ch_id, subtopic)
            diff = diff_map.get(str(getattr(q, "difficulty", "medium")).lower(),
                                "Medium")
            try:
                year = int(getattr(q, "year", 0) or 0)
            except (TypeError, ValueError):
                year = 0
            shift = shift_of(getattr(q, "paperTitle", ""), getattr(q, "examDate", None))

            con.execute(
                "INSERT INTO questions(subject,chapter_id,subtopic_id,qtype,"
                "stem,opt_a,opt_b,opt_c,opt_d,answer,difficulty,year,shift,solution)"
                " VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?)",
                (subject, ch_id, st_id, "MCQ", stem,
                 opt_contents[0], opt_contents[1],
                 opt_contents[2], opt_contents[3],
                 co[0], diff, year, shift, solution))
            existing.add(stem)
            kept += 1

    con.commit()
    n_q = con.execute("SELECT COUNT(*) FROM questions").fetchone()[0]
    n_c = con.execute("SELECT COUNT(*) FROM chapters").fetchone()[0]
    n_s = con.execute("SELECT COUNT(*) FROM subtopics").fetchone()[0]
    print(f"imported {kept}, skipped {skipped}")
    print(f"db now: {n_q} questions / {n_c} chapters / {n_s} subtopics")
    con.close()


if __name__ == "__main__":
    main()
