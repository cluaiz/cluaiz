"""Protect the standalone installer banners from spacing drift and divergence."""
from pathlib import Path
import re
import unittest

ROOT = Path(__file__).resolve().parents[2]


def banners():
    ps = (ROOT / 'install.ps1').read_text(encoding='utf-8-sig')
    sh = (ROOT / 'install.sh').read_text(encoding='utf-8')
    windows = re.findall(r'^Write-Host "([^"]+)" -ForegroundColor White$', ps, re.M)
    unix = re.findall(r'^printf "  \$\{WHITE\}([^"\n]+)\$\{NC\}\\n"$', sh, re.M)
    return [row[2:] for row in windows], unix


class InstallerBannerTests(unittest.TestCase):
    def test_both_installers_show_identical_mascot_and_wordmark(self):
        windows, unix = banners()
        self.assertEqual(len(windows), 24)
        self.assertEqual(windows, unix)

    def test_mascot_has_no_mixed_width_gaps_or_right_edge_jumps(self):
        for rows in banners():
            mascot = rows[:18]
            self.assertTrue(all(len(row) == 61 for row in mascot))
            self.assertTrue(all('\u2800' <= char <= '\u28ff' for row in mascot for char in row))
            contour = [len(row.rstrip('\u2800')) - 1 for row in mascot]
            self.assertEqual(contour[3:12], [44, 44, 45, 46, 46, 47, 48, 49, 50])

    def test_wordmark_reuses_gallery_letters_without_prompt_symbol(self):
        gallery = (ROOT / 'cmd/src/assets/logos/logo_gallery.rs').read_text(encoding='utf-8')
        primary = gallery.split('//  Isometric Primary (The Fixed "Last" Brand)', 1)[1]
        primary = primary.split('r#"\n', 1)[1].split('"#', 1)[0].splitlines()
        expected = [row[17:].rstrip().ljust(44) for row in primary]
        for rows in banners():
            self.assertEqual(rows[18:], [' ' * 8 + row for row in expected])
        for name in ('install.ps1', 'install.sh'):
            self.assertNotIn('>_', (ROOT / name).read_text(encoding='utf-8-sig'))
            self.assertNotIn('[ C L U A I Z ]', (ROOT / name).read_text(encoding='utf-8-sig'))


if __name__ == '__main__':
    unittest.main(verbosity=2)
