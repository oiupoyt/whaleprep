import unittest
import urllib.request
import subprocess

class TestWhaleprepStyling(unittest.TestCase):
    def test_local_style_css_served(self):
        """Whaleprep on VPS must serve /static/style.css with HTTP 200"""
        cmd = [
            "ssh", "-i", "/home/oiupoyt/.ssh/id_ed25519", "-o", "BatchMode=yes",
            "oiupoyt@130.61.117.228", "curl -s -o /dev/null -w '%{http_code}' http://127.0.0.1:8222/static/style.css"
        ]
        r = subprocess.run(cmd, capture_output=True, text=True)
        self.assertEqual(r.stdout.strip(), "200", f"Expected HTTP 200 from local 8222, got {r.stdout.strip()}")

    def test_public_style_css_served(self):
        """https://whaleprep.oiupoyt.space/static/style.css must return HTTP 200 and text/css"""
        req = urllib.request.Request(
            "https://whaleprep.oiupoyt.space/static/style.css?v=5",
            headers={"User-Agent": "Mozilla/5.0"}
        )
        with urllib.request.urlopen(req, timeout=10) as resp:
            self.assertEqual(resp.getcode(), 200)
            self.assertIn("text/css", resp.headers.get("Content-Type", "").lower())

if __name__ == "__main__":
    unittest.main()
