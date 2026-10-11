#!/usr/bin/env python3
"""
Expand the demo question bank beyond the 24 built-in questions.

This is NOT a real scraper — the actual JEE Main questions are published in
image-rendered PDFs and have no machine-readable source. Every "structured JEE
question bank" found online is either paywalled, behind a login, or itself
derived from manual entry.

What this script does: generates a larger set of synthetic questions that are
realistic in form (real syllabus topics, correct +4/-1 scheme, correct NUM
vs MCQ split) so the app can be demonstrated at scale without misrepresenting
sourcing. The stems are deliberately sparse ("placeholder") so no-one mistakes
them for real exam questions.

To get real questions:
  1. MathonGo / Embibe / Allen API — paid, verified, well-tagged.
  2. Manual entry from NTA PDFs using pdftotext + human review.
  3. OCR (tesseract) on NTA PDFs — works but math rendering degrades badly.

Usage:
  python3 scripts/expand_seed.py <database-path> [--count 500]
"""
import sqlite3, argparse, random, sys

TOPICS = [
    # (subject, chapter, subtopic, question_type, difficulty, typical_years)
    ("Physics", "Mechanics", "Kinematics", "MCQ", "Easy", range(2020, 2026)),
    ("Physics", "Mechanics", "Kinematics", "NUM", "Medium", range(2020, 2026)),
    ("Physics", "Mechanics", "Laws of Motion", "MCQ", "Easy", range(2020, 2026)),
    ("Physics", "Mechanics", "Laws of Motion", "NUM", "Hard", range(2022, 2026)),
    ("Physics", "Mechanics", "Work Energy Power", "MCQ", "Medium", range(2020, 2026)),
    ("Physics", "Mechanics", "Work Energy Power", "NUM", "Medium", range(2021, 2026)),
    ("Physics", "Electromagnetism", "Electrostatics", "MCQ", "Medium", range(2020, 2026)),
    ("Physics", "Electromagnetism", "Electrostatics", "NUM", "Hard", range(2021, 2026)),
    ("Physics", "Electromagnetism", "Current Electricity", "MCQ", "Medium", range(2020, 2026)),
    ("Physics", "Optics", "Ray Optics", "MCQ", "Easy", range(2020, 2026)),
    ("Physics", "Optics", "Wave Optics", "MCQ", "Medium", range(2020, 2026)),
    ("Physics", "Optics", "Wave Optics", "NUM", "Hard", range(2022, 2026)),
    ("Physics", "Modern Physics", "Photoelectric Effect", "MCQ", "Easy", range(2020, 2026)),
    ("Physics", "Modern Physics", "Nuclear Physics", "MCQ", "Medium", range(2020, 2026)),
    ("Physics", "Modern Physics", "Atoms and Nuclei", "NUM", "Hard", range(2021, 2026)),
    ("Physics", "Thermodynamics", "Laws of Thermodynamics", "MCQ", "Medium", range(2020, 2026)),
    ("Physics", "Thermodynamics", "Kinetic Theory", "NUM", "Hard", range(2021, 2026)),
    ("Physics", "Waves", "Sound Waves", "MCQ", "Medium", range(2020, 2026)),
    ("Physics", "Waves", "Simple Harmonic Motion", "MCQ", "Easy", range(2020, 2026)),
    ("Physics", "Waves", "Simple Harmonic Motion", "NUM", "Medium", range(2022, 2026)),
    ("Chemistry", "Physical Chemistry", "Mole Concept", "MCQ", "Easy", range(2020, 2026)),
    ("Chemistry", "Physical Chemistry", "Mole Concept", "NUM", "Easy", range(2020, 2026)),
    ("Chemistry", "Physical Chemistry", "Chemical Kinetics", "MCQ", "Medium", range(2020, 2026)),
    ("Chemistry", "Physical Chemistry", "Chemical Equilibrium", "MCQ", "Hard", range(2021, 2026)),
    ("Chemistry", "Physical Chemistry", "Electrochemistry", "MCQ", "Medium", range(2020, 2026)),
    ("Chemistry", "Physical Chemistry", "Electrochemistry", "NUM", "Hard", range(2022, 2026)),
    ("Chemistry", "Physical Chemistry", "Thermodynamics", "MCQ", "Medium", range(2020, 2026)),
    ("Chemistry", "Physical Chemistry", "Solutions", "NUM", "Medium", range(2020, 2026)),
    ("Chemistry", "Organic Chemistry", "GOC", "MCQ", "Medium", range(2020, 2026)),
    ("Chemistry", "Organic Chemistry", "Aldehydes and Ketones", "MCQ", "Medium", range(2020, 2026)),
    ("Chemistry", "Organic Chemistry", "Hydrocarbons", "MCQ", "Easy", range(2020, 2026)),
    ("Chemistry", "Organic Chemistry", "Biomolecules", "MCQ", "Easy", range(2021, 2026)),
    ("Chemistry", "Inorganic Chemistry", "Periodic Table", "MCQ", "Easy", range(2020, 2026)),
    ("Chemistry", "Inorganic Chemistry", "Coordination Compounds", "MCQ", "Medium", range(2020, 2026)),
    ("Chemistry", "Inorganic Chemistry", "d and f Block Elements", "MCQ", "Hard", range(2021, 2026)),
    ("Maths", "Algebra", "Quadratic Equations", "MCQ", "Easy", range(2020, 2026)),
    ("Maths", "Algebra", "Sequences and Series", "MCQ", "Medium", range(2020, 2026)),
    ("Maths", "Algebra", "Sequences and Series", "NUM", "Hard", range(2021, 2026)),
    ("Maths", "Algebra", "Matrices and Determinants", "MCQ", "Medium", range(2020, 2026)),
    ("Maths", "Algebra", "Matrices and Determinants", "NUM", "Hard", range(2022, 2026)),
    ("Maths", "Algebra", "Permutations and Combinations", "MCQ", "Medium", range(2020, 2026)),
    ("Maths", "Algebra", "Binomial Theorem", "MCQ", "Medium", range(2020, 2026)),
    ("Maths", "Calculus", "Limits and Continuity", "MCQ", "Easy", range(2020, 2026)),
    ("Maths", "Calculus", "Differentiation", "MCQ", "Medium", range(2020, 2026)),
    ("Maths", "Calculus", "Definite Integration", "MCQ", "Hard", range(2020, 2026)),
    ("Maths", "Calculus", "Definite Integration", "NUM", "Hard", range(2021, 2026)),
    ("Maths", "Calculus", "Differential Equations", "MCQ", "Hard", range(2021, 2026)),
    ("Maths", "Geometry", "Straight Lines", "MCQ", "Easy", range(2020, 2026)),
    ("Maths", "Geometry", "Circles", "MCQ", "Medium", range(2020, 2026)),
    ("Maths", "Geometry", "Parabola", "MCQ", "Hard", range(2020, 2026)),
    ("Maths", "Geometry", "Ellipse and Hyperbola", "MCQ", "Hard", range(2021, 2026)),
    ("Maths", "Trigonometry", "Trigonometric Functions", "MCQ", "Easy", range(2020, 2026)),
    ("Maths", "Trigonometry", "Inverse Trigonometry", "MCQ", "Medium", range(2020, 2026)),
    ("Maths", "Vectors and 3D", "Vectors", "MCQ", "Medium", range(2020, 2026)),
    ("Maths", "Vectors and 3D", "3D Geometry", "MCQ", "Hard", range(2021, 2026)),
    ("Maths", "Statistics", "Probability", "MCQ", "Medium", range(2020, 2026)),
    ("Maths", "Statistics", "Probability", "NUM", "Hard", range(2022, 2026)),
]

SHIFTS = ["S1", "S2"]
ANSWERS_MCQ = ["A", "B", "C", "D"]
ANSWERS_NUM = ["0", "1", "2", "4", "6", "8", "10", "12", "16", "18", "20",
               "24", "25", "36", "48", "50", "64", "72", "81", "100"]


def ensure_chapter_subtopic(con, subject, chapter, subtopic):
    row = con.execute("SELECT id FROM chapters WHERE subject=? AND name=?", (subject, chapter)).fetchone()
    if row:
        ch_id = row[0]
    else:
        cur = con.execute("INSERT INTO chapters(subject, name) VALUES(?, ?)", (subject, chapter))
        ch_id = cur.lastrowid
    row = con.execute("SELECT id FROM subtopics WHERE chapter_id=? AND name=?", (ch_id, subtopic)).fetchone()
    if row:
        st_id = row[0]
    else:
        cur = con.execute("INSERT INTO subtopics(chapter_id, name) VALUES(?, ?)", (ch_id, subtopic))
        st_id = cur.lastrowid
    return ch_id, st_id


def make_question(rng, subject, chapter, subtopic, qtype, difficulty, year, shift, n):
    """Return (stem, opts, answer, solution) for a synthetic question."""
    tag = f"{subject[:3]}/{subtopic[:8]}/{year}{shift}"
    if qtype == "MCQ":
        stem = f"[{tag}] Practice question {n}: {subtopic} concept application."
        opts = ("Option A", "Option B", "Option C", "Option D")
        answer = rng.choice(ANSWERS_MCQ)
        solution = f"Applying {subtopic} principles. Correct answer is {answer}."
    else:
        stem = f"[{tag}] Numerical problem {n}: Find the value related to {subtopic}."
        opts = ("", "", "", "")
        answer = rng.choice(ANSWERS_NUM)
        solution = f"Solving numerically for {subtopic}: result = {answer}."
    return stem, opts, answer, solution


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("db", help="path to the SQLite database")
    ap.add_argument("--count", type=int, default=500,
                    help="target number of questions to add (default 500)")
    args = ap.parse_args()

    con = sqlite3.connect(args.db)
    current = con.execute("SELECT COUNT(*) FROM questions").fetchone()[0]
    print(f"current: {current} questions")

    rng = random.Random(42)
    added = 0
    n = current + 1

    # Round-robin over topics until we reach target.
    while added < args.count:
        for row in TOPICS:
            if added >= args.count:
                break
            subject, chapter, subtopic, qtype, difficulty, years = row
            year = rng.choice(list(years))
            shift = rng.choice(SHIFTS)
            ch_id, st_id = ensure_chapter_subtopic(con, subject, chapter, subtopic)
            stem, opts, answer, solution = make_question(
                rng, subject, chapter, subtopic, qtype, difficulty, year, shift, n)
            con.execute(
                "INSERT INTO questions(subject,chapter_id,subtopic_id,qtype,stem,"
                "opt_a,opt_b,opt_c,opt_d,answer,difficulty,year,shift,solution) "
                "VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?)",
                (subject, ch_id, st_id, qtype, stem,
                 opts[0], opts[1], opts[2], opts[3],
                 answer, difficulty, year, shift, solution))
            added += 1
            n += 1

    con.commit()
    final = con.execute("SELECT COUNT(*) FROM questions").fetchone()[0]
    chapters = con.execute("SELECT COUNT(*) FROM chapters").fetchone()[0]
    subtopics = con.execute("SELECT COUNT(*) FROM subtopics").fetchone()[0]
    print(f"added {added} questions → total {final}")
    print(f"chapters: {chapters}  subtopics: {subtopics}")
    con.close()


if __name__ == "__main__":
    main()
