#!/usr/bin/env python3
"""Measures the layout at phone, tablet and desktop widths.

Checks the things a stylesheet review cannot: real horizontal overflow, real
tap-target sizes, and whether fixed bars cover content at the bottom.
    scripts/audit_mobile.py <base-url>
"""
import sys
import urllib.parse
import urllib.request
from playwright.sync_api import sync_playwright

BASE = sys.argv[1] if len(sys.argv) > 1 else "http://127.0.0.1:8180"

DEVICES = [
    ("iPhone SE", 375, 667, True),
    ("iPhone 13", 390, 844, True),
    ("iPad", 768, 1024, True),
    ("small laptop", 1024, 768, False),
    ("desktop", 1440, 900, False),
]

AUDIT = """
() => {
  const de = document.documentElement;
  const overflow = de.scrollWidth - de.clientWidth;
  // Anything sticking out past the viewport is a horizontal scroll leak.
  const wide = [...document.querySelectorAll('body *')]
    .filter(el => {
      const r = el.getBoundingClientRect();
      return r.width > 0 && r.right > de.clientWidth + 1;
    })
    .slice(0, 6)
    .map(el => el.tagName.toLowerCase() + (el.className ? '.' + String(el.className).split(' ')[0] : ''));
  // Touch targets on the controls a thumb has to hit. A checkbox is measured
  // through its wrapping label, which is the box a thumb actually lands on.
  const TARGETS = 'button, a.btn, .pal, .opt, .tabbar a, input, select, label.chk';
  const small = [...document.querySelectorAll(TARGETS)]
    .map(el => {
      const box = (el.matches('input') && el.closest('label.chk')) ? el.closest('label.chk') : el;
      return { el: box, r: box.getBoundingClientRect() };
    })
    .filter(x => x.r.width > 0 && x.r.height > 0)
    .filter(x => x.r.height < 44 || x.r.width < 44)
    .slice(0, 8)
    .map(x => {
      const cls = x.el.className ? '.' + String(x.el.className).split(' ')[0] : '';
      return `${x.el.tagName.toLowerCase()}${cls} ${Math.round(x.r.width)}x${Math.round(x.r.height)}`;
    });
  // Does the last visible element sit under a fixed bar?
  const fixed = [...document.querySelectorAll('body *')]
    .filter(el => ['fixed', 'sticky'].includes(getComputedStyle(el).position));
  let covered = null;
  if (fixed.length) {
    const lowest = Math.max(...fixed.map(el => el.getBoundingClientRect().bottom));
    if (lowest >= de.clientHeight - 1) {
      const hit = document.elementFromPoint(de.clientWidth / 2, de.clientHeight - 24);
      covered = hit ? hit.tagName.toLowerCase() + (hit.className ? '.' + String(hit.className).split(' ')[0] : '') : 'none';
    }
  }
  return { overflow, wide, small, covered, scrollH: de.scrollHeight };
}
"""


def sign_in(page, base):
    page.goto(base + "/student", wait_until="networkidle")
    page.fill("#name", "Audit")
    page.fill("#email", "audit@example.com")
    page.click("button[type=submit]")
    page.wait_for_load_state("networkidle")


def paper_url(page, base):
    page.goto(base + "/build", wait_until="networkidle")
    page.evaluate("document.querySelectorAll('.chap').forEach(c => { if (!c.checked) c.click(); })")
    page.click("button[type=submit]")
    page.wait_for_load_state("networkidle")
    return page.url


fails = []
with sync_playwright() as pw:
    browser = pw.chromium.launch()
    for name, w, h, touch in DEVICES:
        ctx = browser.new_context(
            viewport={"width": w, "height": h},
            has_touch=touch,
            is_mobile=touch,
            user_agent=("Mozilla/5.0 (iPhone; CPU iPhone OS 17_0 like Mac OS X) "
                        "AppleWebKit/605.1.15 Mobile/15E148") if touch else None,
        )
        page = ctx.new_page()
        sign_in(page, BASE)
        pages = {"build": BASE + "/build"}
        exam = paper_url(page, BASE)
        page.goto(exam, wait_until="networkidle")
        pages["exam"] = exam

        print(f"\n=== {name} ({w}x{h}) ===")
        for label, url in pages.items():
            page.goto(url, wait_until="networkidle")
            r = page.evaluate(AUDIT)
            ok = r["overflow"] <= 0 and not r["wide"]
            print(f"  {label:<7} overflow={r['overflow']}px  {'OK' if ok else 'LEAK'}")
            if r["wide"]:
                print(f"    too wide: {', '.join(r['wide'])}")
                fails.append(f"{name}/{label}: horizontal overflow {r['overflow']}px")
            if r["small"]:
                print(f"    small targets: {', '.join(r['small'])}")
                fails.append(f"{name}/{label}: {len(r['small'])} targets under 44px")
            if r["covered"]:
                print(f"    bottom of screen is covered by: {r['covered']}")
        ctx.close()
    browser.close()

print()
if fails:
    print("FAILURES")
    for f in fails:
        print("  -", f)
    sys.exit(1)
print("all viewports clean: no overflow, every target 44px or larger")