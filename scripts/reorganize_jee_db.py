#!/usr/bin/env python3
"""Reorganizes and cleanses whaleprep.db to match official JEE Main portions.

1. Fixes mojibake / corrupted UTF-8 sequences in all questions.
2. Reorganizes chapters into the 15 standard JEE Main syllabus units:
   Physics (6):
     - Mechanics
     - Thermal Physics & Thermodynamics
     - Oscillations & Waves
     - Electricity & Magnetism
     - Optics
     - Modern Physics & Electronics
   Chemistry (3):
     - Physical Chemistry
     - Inorganic Chemistry
     - Organic Chemistry
   Maths (6):
     - Algebra
     - Calculus
     - Coordinate Geometry
     - Vectors & 3D Geometry
     - Trigonometry
     - Probability & Statistics
3. Re-assigns subtopics and questions to their respective official chapter.
4. Cleans up empty / duplicate chapters.
"""

import sqlite3
import sys

def get_or_create_chapter(cur, subject, name):
    row = cur.execute("SELECT id FROM chapters WHERE subject=? AND name=?", (subject, name)).fetchone()
    if row:
        return row[0]
    cur.execute("INSERT INTO chapters(subject, name) VALUES(?, ?)", (subject, name))
    return cur.lastrowid

def main(db_path):
    con = sqlite3.connect(db_path)
    cur = con.cursor()

    print(f"Opening database: {db_path}")

    # 1. MOJIBAKE CLEANSE in questions
    print("--- 1. Cleansing Mojibake ---")
    replacements = [
        ("Ã—", "×"),
        ("Â°", "°"),
        ("Â±", "±"),
        ("Âµ", "µ"),
        ("â€“", "–"),
        ("â€”", "—"),
        ("â€™", "’"),
        ("â€˜", "‘"),
        ("â€œ", "“"),
        ("â€", "”"),
        ("â‰ˆ", "≈"),
        ("â‰¤", "≤"),
        ("â‰¥", "≥"),
        ("Â", ""), # stray non-breaking space artifact
    ]
    
    rows = cur.execute("SELECT id, stem, opt_a, opt_b, opt_c, opt_d, solution FROM questions").fetchall()
    fixed_count = 0
    for qid, stem, a, b, c, d, sol in rows:
        orig = (stem, a, b, c, d, sol)
        new = []
        for text in orig:
            if not text:
                new.append(text)
                continue
            t = text
            for bad, good in replacements:
                if bad in t:
                    t = t.replace(bad, good)
            new.append(t)
        if tuple(new) != orig:
            cur.execute(
                "UPDATE questions SET stem=?, opt_a=?, opt_b=?, opt_c=?, opt_d=?, solution=? WHERE id=?",
                (*new, qid)
            )
            fixed_count += 1
    print(f"Fixed mojibake in {fixed_count} questions.")

    # 2. CHAPTER REORGANIZATION
    print("--- 2. Reorganizing JEE Syllabus Portions ---")

    # Rename existing chapters for clarity
    cur.execute("UPDATE chapters SET name='Electricity & Magnetism' WHERE subject='Physics' AND name='Electricity'")
    cur.execute("UPDATE chapters SET name='Modern Physics & Electronics' WHERE subject='Physics' AND name='Modern Physics'")

    # Target chapters
    p_thermal = get_or_create_chapter(cur, "Physics", "Thermal Physics & Thermodynamics")
    p_waves = get_or_create_chapter(cur, "Physics", "Oscillations & Waves")
    p_elec = get_or_create_chapter(cur, "Physics", "Electricity & Magnetism")
    p_optics = get_or_create_chapter(cur, "Physics", "Optics")

    m_vec = get_or_create_chapter(cur, "Maths", "Vectors & 3D Geometry")
    m_prob = get_or_create_chapter(cur, "Maths", "Probability & Statistics")
    m_coord = get_or_create_chapter(cur, "Maths", "Coordinate Geometry")
    m_trig = get_or_create_chapter(cur, "Maths", "Trigonometry")

    # A. Move Thermal Physics subtopics out of Mechanics
    thermal_subs = [
        'Heat Engine Second Law Of Thermodynamics And Carnot Engine',
        'Heat Transfer',
        'Specific Heat Capacity Calorimetry And Change Of State',
        'Kinetic Theory Of Gases And Gas Laws',
        'Thermodynamics Process',
        'Zeroth And First Law Of Thermodynamics',
        'Thermometry And Thermal Expansion'
    ]
    for s in thermal_subs:
        cur.execute("UPDATE subtopics SET chapter_id=? WHERE name=?", (p_thermal, s))

    # B. Move Oscillations & Waves subtopics out of Mechanics
    wave_subs = [
        'Simple Harmonic Motion',
        'Some Systems Of Executing Shm',
        'Forced Damped Oscillations And Resonance',
        'Superposition And Reflection Of Waves',
        'Basic Of Waves And Progressive Waves',
        'Doppler Effect'
    ]
    for s in wave_subs:
        cur.execute("UPDATE subtopics SET chapter_id=? WHERE name=?", (p_waves, s))

    # C. Merge Physics Electromagnetism (seed) into Electricity & Magnetism
    old_em = cur.execute("SELECT id FROM chapters WHERE subject='Physics' AND name='Electromagnetism'").fetchone()
    if old_em:
        cur.execute("UPDATE subtopics SET chapter_id=? WHERE chapter_id=?", (p_elec, old_em[0]))
        cur.execute("DELETE FROM chapters WHERE id=?", (old_em[0],))

    # D. Move Vectors & 3D subtopics out of Algebra
    vec_subs = [
        'Lines And Plane',
        'Plane In Space',
        'Direction Cosines And Direction Ratios Of A Line',
        'Lines In Space',
        'Vectors',
        '3D Geometry'
    ]
    for s in vec_subs:
        cur.execute("UPDATE subtopics SET chapter_id=? WHERE name=?", (m_vec, s))

    # E. Move Probability & Statistics subtopics out of Algebra
    prob_subs = [
        'Classical Defininition Of Probability',
        'Probability Distribution Of A Random Variable',
        'Conditional Probability And Multiplication Theorem',
        'Total Probability Theorem',
        'Calculation Of Standard Deviation Variance And Mean Deviation Of Grouped And Ungrouped Data',
        'Probability'
    ]
    for s in prob_subs:
        cur.execute("UPDATE subtopics SET chapter_id=? WHERE name=?", (m_prob, s))

    # F. Merge Maths Geometry (seed) into Coordinate Geometry
    old_geom = cur.execute("SELECT id FROM chapters WHERE subject='Maths' AND name='Geometry'").fetchone()
    if old_geom:
        cur.execute("UPDATE subtopics SET chapter_id=? WHERE chapter_id=?", (m_coord, old_geom[0]))
        cur.execute("DELETE FROM chapters WHERE id=?", (old_geom[0],))

    # G. Delete duplicate empty Trigonometry chapters
    trig_rows = cur.execute("SELECT id FROM chapters WHERE subject='Maths' AND name='Trigonometry'").fetchall()
    for row in trig_rows:
        cnt = cur.execute("SELECT count(*) FROM subtopics WHERE chapter_id=?", (row[0],)).fetchone()[0]
        qcnt = cur.execute("SELECT count(*) FROM questions q JOIN subtopics s ON s.id=q.subtopic_id WHERE s.chapter_id=?", (row[0],)).fetchone()[0]
        if qcnt == 0:
            cur.execute("DELETE FROM subtopics WHERE chapter_id=?", (row[0],))
            cur.execute("DELETE FROM chapters WHERE id=?", (row[0],))

    # 3. Synchronize question.chapter_id with subtopic.chapter_id
    print("--- 3. Synchronizing Question Chapter IDs ---")
    cur.execute("""
        UPDATE questions 
        SET chapter_id = (SELECT chapter_id FROM subtopics WHERE subtopics.id = questions.subtopic_id)
        WHERE subtopic_id IN (SELECT id FROM subtopics)
    """)

    con.commit()

    print("\n=== FINAL JEE MAIN CHAPTER STRUCTURE ===")
    results = cur.execute("""
        SELECT c.subject, c.name, 
               count(DISTINCT s.id) as sub_cnt, 
               count(q.id) as q_cnt
        FROM chapters c
        LEFT JOIN subtopics s ON s.chapter_id = c.id
        LEFT JOIN questions q ON q.chapter_id = c.id
        GROUP BY c.subject, c.name
        ORDER BY c.subject, c.name
    """).fetchall()

    for subj, cname, sub_cnt, q_cnt in results:
        print(f"  {subj:<12} | {cname:<35} | {sub_cnt:>2} subtopics | {q_cnt:>5} questions")

    con.close()
    print("\nDatabase reorganization complete!")

if __name__ == "__main__":
    db = sys.argv[1] if len(sys.argv) > 1 else "whaleprep.db"
    main(db)
