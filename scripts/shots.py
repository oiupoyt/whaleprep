#!/usr/bin/env python3
"""Screenshots each page at phone and desktop width.
    scripts/shots.py <base-url> <out-dir>
"""
import os
import sys
import urllib.request
from playwright.sync_api import sync_playwright

BASE = sys.argv[1] if len(sys.argv) > 1 else "http://127.0.0.1:8180"
OUT = sys.argv[2] if len(sys.argv) > 2 else "/tmp/shots"
os.makedirs(OUT, exist_ok=True)

SIZES = [("phone", 390, 844, True), ("desktop", 1440, 900, False)]


def sign_in(page):
    page.goto(BASE + "/student", wait_until="networkidle")
    page.fill("#name", "Shot")
    page.fill("#email", "shot@example.com")
    page.click("button[type=submit]")
    page.wait_for_load_state("networkidle")


with sync_playwright() as pw:
    browser = pw.chromium.launch()
    for label, w, h, touch in SIZES:
        ctx = browser.new_context(viewport={"width": w, "height": h},
                                  has_touch=touch, is_mobile=touch, device_scale_factor=2)
        page = ctx.new_page()
        sign_in(page)

        page.goto(BASE + "/", wait_until="networkidle")
        page.screenshot(path=f"{OUT}/{label}-landing.png")

        page.goto(BASE + "/build", wait_until="networkidle")
        page.evaluate("document.querySelectorAll('.chap').forEach(c => { if (!c.checked) c.click(); })")
        page.screenshot(path=f"{OUT}/{label}-build.png", full_page=label == "desktop")

        page.click("button[type=submit]")
        page.wait_for_load_state("networkidle")
        exam = page.url
        page.goto(exam, wait_until="networkidle")
        page.screenshot(path=f"{OUT}/{label}-exam.png")

        # Answer one so the palette shows the answered state too.
        # On a phone the palette is a drawer, so it has to be opened before use.
        if touch:
            page.click("#pal-toggle")
            page.wait_for_timeout(350)
            page.screenshot(path=f"{OUT}/{label}-palette-open.png")
            page.click("[data-close-palette]")
            page.wait_for_timeout(350)

        # Step forward with the on-screen control until a multiple-choice
        # question shows, then pick an option.
        for _ in range(12):
            opt = page.locator(".qpane:not([hidden]) .opt").first
            if opt.count() > 0:
                opt.click()
                break
            page.click(".qpane:not([hidden]) [data-act=save]")
            page.wait_for_timeout(150)
        page.wait_for_timeout(400)
        page.screenshot(path=f"{OUT}/{label}-exam-answered.png")

        ctx.close()
    browser.close()

print("\n".join(sorted(os.listdir(OUT))))