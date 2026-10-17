#!/usr/bin/env python3
"""Import the image-bearing MCQs that scripts/import_examgoal.py skipped.

Downloads each question figure from the examgoal CDN into static/qimg/
and rewrites <img src> to the local path, so the bank stays self-contained.

Usage: python3 scripts/import_with_images.py DB_PATH [--pkl /tmp/db.pkl]
Safe to re-run: dedupes on exact stem match, skips already-downloaded files.
"""
import argparse, hashlib, html, pickle, re, sqlite3, sys, urllib.request
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(ROOT / "scripts"))
sys.path.insert(0, str(ROOT / "scripts" / "examgoal_model"))
from question import Question
from chapter import Chapter
from fix_math import fix_field

ALLOWED = {"br", "p", "sup", "sub", "b", "strong", "i", "em", "u",
           "ol", "ul", "li", "table", "tbody", "thead", "tr", "th", "td",
           "span", "h3"}
SUBJECT_FIX = {"mathematics": "Maths", "physics": "Physics", "chemistry": "Chemistry"}
QIMG = ROOT / "static" / "qimg"

class FixUnpickler(pickle.Unpickler):
    def find_class(self, module, name):
        if name == "Question":
            return Question
        if name == "Chapter":
            return Chapter
        return super().find_class(module, name)

def title_case(slug: str) -> str:
    return " ".join(w.capitalize() for w in slug.replace("-", " ").split())

def shift_of(paper_title, exam_date) -> str:
    t = str(paper_title or "")
    low = t.lower()
    if "evening" in low: return "S2"
    if "morning" in low: return "S1"
    return "S1"

def url_hash(u: str) -> str:
    return hashlib.sha1(u.encode()).hexdigest()[:16]

def fetch(url: str, dest: Path, tries=3) -> bool:
    if dest.exists() and dest.stat().st_size > 0:
        return True
    for _ in range(tries):
        try:
            req = urllib.request.Request(url, headers={"User-Agent": "Mozilla/5.0"})
            with urllib.request.urlopen(req, timeout=20) as r:
                b = r.read()
            if b:
                dest.write_bytes(b)
                return True
        except Exception:
            pass
    return False

def rewrite_images(qid: str, raw: str):
    """Download each <img src>, rewrite to /static/qimg/<name>.png."""
    ok_all = True
    def repl(m):
        nonlocal ok_all
        url = html.unescape(m.group(1))
        name = f"{qid[:24]}-{url_hash(url)}.png"
        if fetch(url, QIMG / name):
            return f'<img src="/static/qimg/{name}">'
        ok_all = False
        return ""
    out = re.sub(r'<img[^>]*src="([^"]+)"[^>]*>', repl, raw)
    return out, ok_all

def sanitize_img(qid: str, raw: str):
    """sanitize() + surviving img tags with local src only."""
    if not raw:
        return "", True
    text = html.unescape(str(raw))
    # unwrap <picture>: keep the <img> fallback inside, drop <source> variants
    def unwrap_pic(m):
        inner = m.group(0)
        imgs = re.findall(r"<img[^>]*>", inner)
        return imgs[0] if imgs else ""
    text = re.sub(r"<picture.*?</picture>", unwrap_pic, text, flags=re.S | re.I)
    text = re.sub(r"<style.*?</style>", "", text, flags=re.S | re.I)
    text = re.sub(r"<colgroup.*?</colgroup>", "", text, flags=re.S | re.I)
    text = re.sub(r"</?(?:col|colgroup)[^>]*>", "", text, flags=re.I)
    text, ok = rewrite_images(qid, text)
    def tag(m):
        whole, closing, name = m.group(0), m.group(1), m.group(2).lower()
        if name == "img":
            # keep ONLY if src is already a local rewritten path
            if 'src="/static/qimg/' in whole:
                return whole
            return ""
        if name in ALLOWED:
            return f"<{closing}{name}>"
        return ""
    text = re.sub(r"<(/?)([a-zA-Z][a-zA-Z0-9]*)((?:\s[^>]*)?)>", tag, text)
    text = re.sub(r"[ \t\xa0]+", " ", text)
    text = re.sub(r"\n\s*\n+", "\n", text)
    text = text.strip()
    return fix_field(text), ok

def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("db")
    ap.add_argument("--pkl", default="/tmp/db.pkl")
    args = ap.parse_args()

    print(f"loading {args.pkl} ...", flush=True)
    with open(args.pkl, "rb") as f:
        data = FixUnpickler(f).load()
    QIMG.mkdir(parents=True, exist_ok=True)

    con = sqlite3.connect(args.db)
    cur = con.cursor()
    existing = {r[0] for r in cur.execute("SELECT stem FROM questions")}
    print(f"existing stems: {len(existing)}", flush=True)

    def ensure_ids(subject, chapter, subtopic):
        cur.execute("SELECT id FROM chapters WHERE subject=? AND name=?", (subject, chapter))
        row = cur.fetchone()
        if row:
            ch_id = row[0]
        else:
            cur.execute("INSERT INTO chapters(subject,name) VALUES(?,?)", (subject, chapter))
            ch_id = cur.lastrowid
        cur.execute("SELECT id FROM subtopics WHERE chapter_id=? AND name=?", (ch_id, subtopic))
        row = cur.fetchone()
        if row:
            return ch_id, row[0]
        cur.execute("INSERT INTO subtopics(chapter_id,name) VALUES(?,?)", (ch_id, subtopic))
        return ch_id, cur.lastrowid

    imported = skipped = dl_fail = opt_img = 0
    diff_map = {"easy": "Easy", "medium": "Medium", "hard": "Hard"}
    for _key, chap in data.items():
        for q in chap.question_dict.values():
            d = q.__dict__
            if d.get("type") != "mcq":
                continue
            co = d.get("correct_options") or []
            if len(co) != 1 or co[0] not in ("A", "B", "C", "D"):
                continue
            stem_raw = d.get("question") or ""
            opts_raw = [o for o in (d.get("options") or []) if isinstance(o, dict)]
            opt_txt = " ".join((o.get("content") or "") for o in opts_raw)
            if "<img" not in stem_raw and "<img" not in opt_txt:
                continue
            if len([o for o in opts_raw if (o.get("content") or "").strip()]) != 4:
                skipped += 1
                continue
            qid = str(d.get("question_id") or f"q{d.get('year','x')}")
            stem, ok = sanitize_img(qid, stem_raw)
            stem_has_img = "<img" in stem_raw
            if not ok or (stem_has_img and "<img" not in stem):
                dl_fail += 1
                continue
            # Options may carry images (organic structures, graphs): download
            # and rewrite them exactly like stem figures.
            opts = []
            bad = False
            for o in opts_raw[:4]:
                t, ook = sanitize_img(qid, o.get("content") or "")
                if not t or (("<img" in (o.get("content") or "")) and not ook):
                    bad = True
                    break
                opts.append(t)
            if bad or len(opts) != 4:
                skipped += 1
                continue
            solution, _ok = sanitize_img(qid, d.get("explanation") or "")
            if stem in existing:
                skipped += 1
                continue
            subject = SUBJECT_FIX.get(str(getattr(chap, "parent_subject", "")).lower(),
                                      str(getattr(chap, "parent_subject", "")).title())
            chapter = title_case(str(d.get("chpaterGroup") or d.get("chapter") or "General"))
            subtopic = title_case(str(d.get("topic") or d.get("chapter") or "General"))
            ch_id, st_id = ensure_ids(subject, chapter, subtopic)
            diff = diff_map.get(str(d.get("difficulty", "medium")).lower(), "Medium")
            try:
                year = int(d.get("year") or 0)
            except (TypeError, ValueError):
                year = 0
            shift = shift_of(d.get("paperTitle", ""), d.get("examDate"))
            cur.execute(
                "INSERT INTO questions(subject,chapter_id,subtopic_id,qtype,"
                "stem,opt_a,opt_b,opt_c,opt_d,answer,difficulty,year,shift,solution)"
                " VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?)",
                (subject, ch_id, st_id, "MCQ", stem, opts[0], opts[1], opts[2], opts[3],
                 co[0], diff, year, shift, solution))
            existing.add(stem)
            imported += 1
            if imported % 200 == 0:
                print(f"  ... {imported} imported", flush=True)
    con.commit()
    n_img = len(list(QIMG.glob("*.png")))
    print(f"imported {imported} image MCQs (skipped {skipped}, dl-fail {dl_fail}, "
          f"opt-image {opt_img}); static/qimg holds {n_img} files", flush=True)
    con.close()

if __name__ == "__main__":
    main()
