#!/usr/bin/env python3
"""Convert static/qimg/*.png to resized WebP and update DB references.

Question figures only need ~900px width for the exam card; WebP q80
cuts the folder to about a fifth of its PNG size.

Usage: python3 scripts/shrink_qimg.py DB_PATH
"""
import re
import sqlite3
import sys
from pathlib import Path
from PIL import Image

ROOT = Path(__file__).resolve().parent.parent
QIMG = ROOT / "static" / "qimg"

def main():
    db = sqlite3.connect(sys.argv[1])
    cur = db.cursor()
    converted = 0
    for png in sorted(QIMG.glob("*.png")):
        webp = png.with_suffix(".webp")
        if webp.exists():
            continue
        try:
            im = Image.open(png)
            if im.mode in ("P", "RGBA", "LA"):
                im = im.convert("RGB")
            w, h = im.size
            if w > 900:
                im = im.resize((900, int(h * 900 / w)))
            im.save(webp, "WEBP", quality=80)
            png.unlink()
            converted += 1
        except Exception as e:
            print(f"skip {png.name}: {e}")
            continue
    print(f"converted {converted} pngs to webp")
    # rewrite DB references
    n = 0
    for (qid, stem, a, b, c, d, sol) in cur.execute(
            "SELECT id, stem, opt_a, opt_b, opt_c, opt_d, solution FROM questions"):
        vals = [stem, a, b, c, d, sol]
        changed = False
        for i, v in enumerate(vals):
            if v and "/static/qimg/" in v and ".png" in v:
                nv = re.sub(r"(/static/qimg/[A-Za-z0-9_-]+)\.png", r"\1.webp", v)
                if nv != v:
                    vals[i] = nv
                    changed = True
        if changed:
            cur.execute("UPDATE questions SET stem=?, opt_a=?, opt_b=?, opt_c=?, opt_d=?, solution=? WHERE id=?",
                        (vals[0], vals[1], vals[2], vals[3], vals[4], vals[5], qid))
            n += 1
    db.commit()
    print(f"updated {n} question rows")
    db.close()

if __name__ == "__main__":
    main()
