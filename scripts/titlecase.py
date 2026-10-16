import sqlite3, sys, re

# Small words stay lowercase in title case (unless first/last word)
SMALL = {"a","an","the","and","or","nor","but","for","yet","so","as","at",
         "by","for","from","in","into","of","on","onto","per","to","with",
         "vs","via","de","van","der","and/or"}

# Acronyms and special tokens that keep their form
KEEP = {"AC","DC","EMF","pH","GOC","IUPAC","SN1","SN2","E1","E2","3D","2D",
        "AM","GM","HM","AP","GP","HP","LCM","HCF","EM","SHM","RMS","SI",
        "MCQ","JEE","NTA","AIEEE","S1","S2","N","Z","Q","R","C","O","H"}

# Common mis-cased words found in the bank -> correct form
FIX = {"ac": "AC", "am": "AM", "gm": "GM", "hm": "HM", "ap": "AP",
       "gp": "GP", "hp": "HP", "emf": "EMF", "rms": "RMS", "shm": "SHM",
       "goc": "GOC", "iupac": "IUPAC", "em": "EM"}

def title_case(name: str) -> str:
    # Preserve existing mixed-case names that are already fine
    words = name.split()
    out = []
    for i, w in enumerate(words):
        # Keep tokens with internal capitals/digits (e.g. pH, 3D, S1)
        core = re.sub(r'^[^A-Za-z0-9]+|[^A-Za-z0-9]+$', '', w)
        if not core:
            out.append(w)
            continue
        if core in KEEP or (core.isupper() and len(core) <= 4):
            out.append(w)
            continue
        low = core.lower()
        if low in FIX:
            out.append(w.replace(core, FIX[low]))
        elif low in SMALL and i != 0 and i != len(words) - 1:
            # lowercase the core, keep surrounding punctuation
            out.append(w.replace(core, low))
        else:
            out.append(w.replace(core, core[0].upper() + core[1:].lower()))
    return " ".join(out)


def main(db_path: str) -> int:
    con = sqlite3.connect(db_path)
    cur = con.cursor()
    rows = cur.execute("SELECT id, name FROM subtopics").fetchall()
    changed = 0
    for sid, name in rows:
        new = title_case(name)
        if new != name:
            cur.execute("UPDATE subtopics SET name=? WHERE id=?", (new, sid))
            changed += 1
            if changed <= 15:
                print(f"{name}  ->  {new}")
    con.commit()
    print(f"\nchanged {changed} of {len(rows)}")
    return changed


if __name__ == "__main__":
    main(sys.argv[1])
