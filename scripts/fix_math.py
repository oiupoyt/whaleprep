#!/usr/bin/env python3
"""Normalize math delimiters in stored questions so KaTeX renders inline.

The source data wraps EVERY math fragment in $$...$$ — even a mid-sentence
minus sign ($$-$$) or a single symbol ($$\\theta $$ inside a word). KaTeX
auto-render treats $$ as display math: a centered block that shreds sentence
flow (a fraction alone on three lines, ", is :" stranded below).

Fix: convert all balanced $$...$$ spans to \\(...\\) inline math. True
display equations don't exist in this corpus — every $$ pair is an inline
fragment, verified by audit (0 stems with odd $$; the only unbalanced rows
are 1 option + 3 solutions with an orphan closer, which we drop).

Also rewrites legacy \\matrix{...\\cr...} (Plain-TeX-ism, undefined in KaTeX)
to \\begin{matrix}...\\\\...\\end{matrix}.

Usage: python3 scripts/fix_math.py path/to/app.db [--dry-run]
Idempotent: rows without $$ or \\matrix are untouched; re-running is a no-op.
"""
import argparse
import re
import sqlite3
import sys

def fix_matrix(text: str) -> str:
    """\\matrix{ a & b \\cr c & d } -> \\begin{matrix} a & b \\\\ c & d \\end{matrix}."""

    # Handle nesting by matching \matrix{ then balancing braces manually.
    out = []
    i = 0
    while True:
        j = text.find("\\matrix", i)
        if j == -1:
            out.append(text[i:])
            break
        out.append(text[i:j])
        k = text.find("{", j)
        if k == -1:
            out.append(text[j:])
            break
        depth = 0
        p = k
        while p < len(text):
            if text[p] == "{":
                depth += 1
            elif text[p] == "}":
                depth -= 1
                if depth == 0:
                    break
            p += 1
        if depth != 0:  # unbalanced — leave as-is, KaTeX will show source
            out.append(text[j:])
            break
        inner = text[k + 1 : p].replace("\\cr", "\\\\")
        out.append("\\begin{matrix} " + inner + " \\end{matrix}")
        i = p + 1
    return "".join(out)


# Dollar runs pair sequentially left-to-right. One preprocessing step first:
# a $$ run is a closer AND an opener fused together ($A$$B$) only when the
# span it would open contains no tags before its closer — i.e. what follows
# is genuine math, not text like sin<sup>2</sup>. Punctuation or tags after
# $$ ($$a$$, b) is a plain closer and must not split.
ABUT_RE = re.compile(r"([^\s])\$\$(?=[A-Za-z0-9\\({][^<>]*?\$)")
RUN_RE = re.compile(r"\$+")
TAG_RE = re.compile(r"<[a-zA-Z/!]")
MAX_MATH = 500


def fix_delims(text: str) -> str:
    """$$...$$ and $...$ -> inline \\(...\\) for fragments, $$ display for
    block derivations. Orphan $$ dropped, lone $ kept."""
    if not text or "$" not in text:
        return text
    # A $$$$ run is an empty pair fused to a boundary: split it so the empty
    # span renders as nothing instead of corrupting pairing.
    text = re.sub(r"\$\$\$\$", "$$ $$", text)
    # A $$ run with non-space on BOTH sides ($A$$B$) is a closer AND an
    # opener fused together. Splitting it lets sequential pairing work.
    text = ABUT_RE.sub(r"\1$ $", text)
    parts = RUN_RE.split(text)
    runs = RUN_RE.findall(text)
    # parts[0] TEXT, then contents alternate MATH/TEXT/MATH.../TEXT.
    out = [parts[0]]
    pending: "str | None" = None  # MATH content waiting for its closer
    i = 0
    while i < len(runs):
        content = parts[i + 1]
        if pending is None:
            if content.strip() == "" and i < len(runs) - 1:
                # Whitespace-only content means mis-pairing (e.g. a stray
                # "$ " before a real pair): emit literally, stay in text.
                out.append(runs[i] + content)
            elif i == len(runs) - 1:
                # Orphan opener: a $$ run is a stray closer — drop it. A
                # lone $ stays literal (harmless; auto-render ignores it).
                if len(runs[i]) >= 2:
                    out.append(content)
                else:
                    out.append(runs[i] + content)
            elif TAG_RE.search(content) or len(content) > MAX_MATH:
                out.append(content)  # abandon rest to literal
                for j in range(i + 1, len(runs)):
                    out.append(runs[j] + parts[j + 1])
                break
            else:
                pending = content
        else:
            # Closer run consumed: wrap pending with the right delimiters,
            # then append the trailing text.
            if "\n" in pending or "\\begin{" in pending:
                out.append("$$" + pending + "$$" + content)
            else:
                out.append("\\(" + pending + "\\)" + content)
            pending = None
        i += 1
    if pending is not None:
        # Unclosed opener (odd runs): emit content literally.
        out.append(pending)
    return "".join(out)




def fix_single_dollar(text: str) -> str:
    """Strip stray $ left after delimiter pairing: a lone $ before \\( or
    before $$, and any orphan single $ in text segments between display
    pairs. Everything inside $$..$$ pairs is preserved."""
    if not text or "$" not in text:
        return text
    # "$ $$" -> "$$" ; "$ \\(" -> "\\(" (junk dollar stuck to real math)
    text = re.sub(r"\$(\s*\$\$)", r"\1", text)
    text = re.sub(r"\$(\s*\\\()", r"\1", text)
    # Drop any remaining single $ in text segments; keep display pairs.
    parts = text.split("$$")
    out = []
    for i, part in enumerate(parts):
        if i % 2 == 1:
            out.append("$$" + part + "$$")
        else:
            out.append(part.replace("$", ""))
    return "".join(out)


def fix_tex_isms(text: str) -> str:
    """Rewrite source TeX idioms KaTeX rejects (verified by batch render):
    _\\limits / ^\\limits -> \\limits placed before _/^, \\raise/\\lower
    boxes dropped (vertical tweaks), \\cdotp -> \\cdot."""
    if not text or "\\" not in text:
        return text
    text = re.sub(r"_\\limits\s*", r"\\limits_", text)
    text = re.sub(r"\^\\limits\s*", r"\\limits^", text)
    # \raise<dim>\hbox{...} / \lower<dim>\hbox{...}: drop the whole box
    # including nested braces (extensible-arrow hacks like
    # \mathbin{\lower.3ex\hbox{...}} become \rightarrow). re.sub can't match
    # balanced braces, so splice manually.
    hbox_pat = re.compile(r"\\(?:raise|lower)[0-9.]*ex\\hbox\{")

    def drop_all_hbox(s: str) -> str:
        while True:
            m = hbox_pat.search(s)
            if not m:
                return s
            depth = 1
            p = m.end()
            while p < len(s) and depth > 0:
                if s[p] == "{":
                    depth += 1
                elif s[p] == "}":
                    depth -= 1
                p += 1
            if depth != 0:
                return s  # unbalanced: leave the rest alone
            inner = s[m.end() : p - 1]
            repl = (
                "\\rightarrow "
                if ("rightarrow" in inner or "leftarrow" in inner or "over" in inner)
                else ""
            )
            s = s[: m.start()] + repl + s[p:]

    text = drop_all_hbox(text)
    text = text.replace("\\cdotp", "\\cdot")
    # \root 4 \of X -> \sqrt[4]{X} (KaTeX has no \root primitive).
    text = re.sub(r"\\root\s+(\S+)\s+\\of\b", r"\\sqrt[\1]", text)
    # \tag{..} is an equation number — meaningless (and illegal) inline.
    text = re.sub(r"\\tag\{[^{}]*\}", "", text)
    # \, thin-space trips KaTeX in spots (prime-adjacent etc.); plain space.
    text = text.replace("\\,", " ")
    return text


def fix_field(text: str) -> str:
    if not text:
        return text
    # TeX-ism rewrites FIRST: \hbox{...} blocks contain paren sequences that
    # would corrupt delimiter pairing if split across \( boundaries.
    text = fix_tex_isms(text)
    if "\\matrix" in text:
        text = fix_matrix(text)
    if "$" in text:
        text = fix_delims(text)
    if "$" in text:
        text = fix_single_dollar(text)
    return text


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("db")
    ap.add_argument("--dry-run", action="store_true")
    args = ap.parse_args()

    con = sqlite3.connect(args.db)
    rows = con.execute(
        "SELECT id, stem, opt_a, opt_b, opt_c, opt_d, solution FROM questions"
    ).fetchall()
    changed = 0
    for qid, stem, a, b, c, d, sol in rows:
        new = [fix_field(x) for x in (stem, a, b, c, d, sol)]
        if list(new) != [stem, a, b, c, d, sol]:
            changed += 1
            if not args.dry_run:
                con.execute(
                    "UPDATE questions SET stem=?, opt_a=?, opt_b=?, opt_c=?,"
                    " opt_d=?, solution=? WHERE id=?",
                    (*new, qid),
                )
    if not args.dry_run:
        con.commit()
    # Report math health after the pass.
    left = 0
    for (stem, a, b, c, d, sol) in con.execute(
        "SELECT stem, opt_a, opt_b, opt_c, opt_d, solution FROM questions"
    ):
        blob = "".join(x or "" for x in (stem, a, b, c, d, sol))
        if "$$" in blob or "\\matrix" in blob:
            left += 1
    print(f"rows: {len(rows)}, changed: {changed}, rows still with $$/\\matrix: {left}")
    con.close()
    return 0


if __name__ == "__main__":
    sys.exit(main())
