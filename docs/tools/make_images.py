#!/usr/bin/env python3
"""Draws every docs and folder-README image into docs/assets/<topic>/, in black, white and greys only
(the cluaiz.com theme; no colour).

Each figure in figures.py is a small layout: cards with an icon, decision diamonds, frames that group
cards, arrows between them, and a header and footer. This script turns a figure into an HTML page
(cards are HTML so text wraps; arrows are SVG), opens it in a headless Chrome and saves a PNG.

The figures are the editable source; never edit a PNG by hand. Every fact in an image is also written
in the page that shows it.

Run from the repository root:  python docs/tools/make_images.py [name ...]
Needs: pip install playwright pillow; Chrome or Edge installed (or `playwright install chromium`).
Icons: Lucide (ISC licence, docs/tools/icons/LICENSE).
"""

from __future__ import annotations

import html
import io
import re
import sys
from dataclasses import dataclass, field
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
ASSETS = ROOT / "docs" / "assets"
ICONS = Path(__file__).resolve().parent / "icons"

WIDTH = 1200          # CSS pixels of every figure
SCALE = 1.5           # device pixels per CSS pixel in the PNG (1800 px wide)

TONES = {  # black, white and greys only: the cluaiz.com theme (owner, 2026-11-21)
    "white": "#ffffff", "light": "#d6d6d6", "mid": "#a6a6a6", "dim": "#7a7a7a",
    # older names map to greys so a figure never brings a colour in
    "blue": "#d6d6d6", "violet": "#ffffff", "amber": "#e8e8e8", "green": "#bdbdbd",
    "red": "#ffffff", "cyan": "#cfcfcf", "slate": "#a6a6a6", "pink": "#bdbdbd",
}

CSS = """
* { box-sizing: border-box; margin: 0; padding: 0; }
body { background: #070707; font-family: "Segoe UI Variable Text", "Segoe UI", Inter, system-ui, sans-serif; }
#fig { width: %(w)dpx; position: relative; overflow: hidden; color: #ececec;
  background:
    radial-gradient(900px 460px at 50%% -60px, rgba(255,255,255,.075), transparent 70%%),
    radial-gradient(rgba(255,255,255,.055) 1px, transparent 1.4px) 0 0 / 22px 22px,
    #0a0a0a; padding: 44px 52px 34px; }
.eyebrow { font-size: 12.5px; font-weight: 700; letter-spacing: .14em; text-transform: uppercase; }
h1 { font-size: 31px; font-weight: 700; letter-spacing: -.01em; margin-top: 8px; color: #f5f5f5; }
.sub { font-size: 16px; color: #9e9e9e; margin-top: 8px; max-width: 980px; line-height: 1.45; }
#canvas { position: relative; margin-top: 34px; }
svg.edges { position: absolute; left: 0; top: 0; overflow: visible; pointer-events: none; }
.card { position: absolute; border-radius: 14px; padding: 14px 16px; background: linear-gradient(180deg, #171717, #121212);
  border: 1px solid rgba(255,255,255,.085); box-shadow: 0 10px 30px rgba(0,0,0,.35); }
.card.strong { border-color: var(--t); box-shadow: 0 0 0 1px var(--t) inset, 0 12px 34px rgba(0,0,0,.45), 0 0 26px -6px var(--t); }
.card .hd { display: flex; align-items: center; gap: 11px; }
.ic { flex: none; width: 34px; height: 34px; border-radius: 10px; display: grid; place-items: center;
  background: color-mix(in srgb, var(--t) 16%%, transparent); color: var(--t); }
.ic svg { width: 19px; height: 19px; }
.card .tt { font-size: 16.5px; font-weight: 650; color: #f2f2f2; line-height: 1.2; }
.card .tx { font-size: 13.6px; color: #a3a3a3; margin-top: 9px; line-height: 1.42; }
.card .tx b { color: #e6e6e6; font-weight: 600; }
.card .tx code, .mono { font-family: "Cascadia Mono", Consolas, ui-monospace, monospace; font-size: 12.6px; color: #d9d9d9; }
.num { position: absolute; top: -10px; left: -10px; width: 24px; height: 24px; border-radius: 50%%; font-size: 12.5px;
  font-weight: 700; display: grid; place-items: center; background: var(--t); color: #0a0a0a; box-shadow: 0 0 0 3px #0a0a0a; }
.chips { display: flex; flex-wrap: wrap; gap: 6px; margin-top: 10px; }
.chip { font-size: 12px; padding: 3px 8px; border-radius: 999px; color: #dcdcdc;
  background: rgba(255,255,255,.06); border: 1px solid rgba(255,255,255,.09); }
.chip.t { color: var(--t); background: color-mix(in srgb, var(--t) 12%%, transparent); border-color: color-mix(in srgb, var(--t) 35%%, transparent); }
.pill { position: absolute; display: flex; align-items: center; gap: 9px; padding: 0 14px; border-radius: 999px;
  background: color-mix(in srgb, var(--t) 13%%, #121212); border: 1px solid color-mix(in srgb, var(--t) 45%%, transparent);
  font-size: 14.5px; font-weight: 600; color: #efefef; }
.pill svg { width: 17px; height: 17px; color: var(--t); flex: none; }
.pill .tx { font-weight: 400; color: #a3a3a3; font-size: 13px; }
.dia { position: absolute; display: grid; place-items: center; text-align: center; }
.dia .shape { position: absolute; inset: 0; }
.dia .lbl { position: relative; font-size: 14.5px; font-weight: 650; line-height: 1.25; padding: 0 22%%; }
.group { position: absolute; border-radius: 18px; border: 1.5px dashed color-mix(in srgb, var(--t) 45%%, transparent);
  background: color-mix(in srgb, var(--t) 4%%, transparent); }
.group .gl { position: absolute; top: -11px; left: 18px; padding: 2px 10px; border-radius: 999px; background: #0a0a0a;
  font-size: 11.5px; font-weight: 700; letter-spacing: .12em; text-transform: uppercase; color: var(--t);
  border: 1px solid color-mix(in srgb, var(--t) 40%%, transparent); }
.elabel { position: absolute; transform: translate(-50%%, -50%%); white-space: nowrap; font-size: 12.3px; font-weight: 600;
  padding: 3px 9px; border-radius: 999px; background: #0e0e0e; border: 1px solid rgba(255,255,255,.12); color: #cacaca; }
.text { position: absolute; font-size: 13.5px; color: #a3a3a3; line-height: 1.45; }
.foot { margin-top: 30px; padding-top: 18px; border-top: 1px solid rgba(255,255,255,.08); display: flex; flex-direction: column; gap: 9px; }
.foot .n { display: flex; gap: 10px; align-items: flex-start; font-size: 14.2px; color: #b5b5b5; line-height: 1.45; }
.foot .n svg { width: 16px; height: 16px; flex: none; margin-top: 2px; }
.foot .src { font-family: "Cascadia Mono", Consolas, ui-monospace, monospace; font-size: 12.2px; color: #737373; margin-top: 4px; }
"""


def icon(name: str | None) -> str:
    if not name:
        return ""
    text = (ICONS / f"{name}.svg").read_text(encoding="utf-8")
    text = re.sub(r"<!--.*?-->", "", text, flags=re.S).strip()
    return re.sub(r'class="[^"]*"', "", text)


def esc(text: str) -> str:
    """Escapes text, then allows `code` and **bold** marks."""
    out = html.escape(text, quote=False)
    out = re.sub(r"`([^`]+)`", r"<code>\1</code>", out)
    return re.sub(r"\*\*([^*]+)\*\*", r"<b>\1</b>", out)


@dataclass
class Box:
    x: float
    y: float
    w: float
    h: float

    def anchor(self, side: str, off: float = 0.5) -> tuple[tuple[float, float], tuple[int, int]]:
        if side == "r":
            return (self.x + self.w, self.y + self.h * off), (1, 0)
        if side == "l":
            return (self.x, self.y + self.h * off), (-1, 0)
        if side == "t":
            return (self.x + self.w * off, self.y), (0, -1)
        return (self.x + self.w * off, self.y + self.h), (0, 1)


@dataclass
class Fig:
    name: str
    eyebrow: str
    title: str
    sub: str = ""
    tone: str = "blue"
    height: int = 300
    parts: list[str] = field(default_factory=list)
    boxes: dict[str, Box] = field(default_factory=dict)
    edges: list[str] = field(default_factory=list)
    labels: list[str] = field(default_factory=list)
    notes: list[tuple[str, str, str]] = field(default_factory=list)
    sources: list[str] = field(default_factory=list)

    # ---- elements -------------------------------------------------------------------------------
    def card(self, id: str, x, y, w, h, title, text="", icon_name=None, tone="slate", num=None, strong=False,
             chips=(), chip_tone=False, mono=False):
        self.boxes[id] = Box(x, y, w, h)
        t = TONES[tone]
        ic = f'<div class="ic">{icon(icon_name)}</div>' if icon_name else ""
        tt = f'<div class="tt{" mono" if mono else ""}">{esc(title)}</div>'
        tx = f'<div class="tx">{esc(text)}</div>' if text else ""
        ch = "".join(f'<span class="chip{" t" if chip_tone else ""}">{esc(c)}</span>' for c in chips)
        ch = f'<div class="chips">{ch}</div>' if ch else ""
        nm = f'<div class="num">{num}</div>' if num is not None else ""
        self.parts.append(f'<div class="card{" strong" if strong else ""}" style="--t:{t};left:{x}px;top:{y}px;'
                          f'width:{w}px;height:{h}px">{nm}<div class="hd">{ic}{tt}</div>{tx}{ch}</div>')

    def pill(self, id: str, x, y, w, h, title, text="", icon_name=None, tone="slate"):
        self.boxes[id] = Box(x, y, w, h)
        tx = f'<span class="tx">{esc(text)}</span>' if text else ""
        self.parts.append(f'<div class="pill" style="--t:{TONES[tone]};left:{x}px;top:{y}px;width:{w}px;height:{h}px">'
                          f'{icon(icon_name)}<span>{esc(title)}</span>{tx}</div>')

    def diamond(self, id: str, x, y, w, h, title, tone="amber"):
        self.boxes[id] = Box(x, y, w, h)
        t = TONES[tone]
        shape = (f'<svg class="shape" viewBox="0 0 {w} {h}" width="{w}" height="{h}"><polygon points="{w/2},1 {w-1},{h/2} '
                 f'{w/2},{h-1} 1,{h/2}" fill="{t}" fill-opacity=".12" stroke="{t}" stroke-width="1.5"/></svg>')
        self.parts.append(f'<div class="dia" style="left:{x}px;top:{y}px;width:{w}px;height:{h}px">{shape}'
                          f'<div class="lbl">{esc(title)}</div></div>')

    def group(self, x, y, w, h, label, tone="slate", id=None):
        if id:
            self.boxes[id] = Box(x, y, w, h)
        self.parts.insert(0, f'<div class="group" style="--t:{TONES[tone]};left:{x}px;top:{y}px;width:{w}px;'
                             f'height:{h}px"><div class="gl">{esc(label)}</div></div>')

    def text(self, x, y, w, body, size=13.5, color="#a3a3a3", align="left", weight=400):
        self.parts.append(f'<div class="text" style="left:{x}px;top:{y}px;width:{w}px;font-size:{size}px;color:{color};'
                          f'text-align:{align};font-weight:{weight}">{esc(body)}</div>')

    def raw(self, markup: str):
        self.parts.append(markup)

    def note(self, text, icon_name="info", tone="slate"):
        self.notes.append((text, icon_name, tone))

    def source(self, text):
        self.sources.append(text)

    # ---- arrows ---------------------------------------------------------------------------------
    def edge(self, a, b, sa="r", sb="l", label=None, tone=None, dashed=False, via=(), oa=0.5, ob=0.5, at=0.5, head=True):
        (p0, d0) = self.boxes[a].anchor(sa, oa)
        (p1, d1) = self.boxes[b].anchor(sb, ob)
        gap = 18
        q0 = (p0[0] + d0[0] * gap, p0[1] + d0[1] * gap)
        q1 = (p1[0] + d1[0] * gap, p1[1] + d1[1] * gap)
        pts = [p0, q0]
        if via:
            pts += list(via)
        else:
            if d0[0] and d1[0]:
                mx = (q0[0] + q1[0]) / 2
                pts += [(mx, q0[1]), (mx, q1[1])]
            elif d0[1] and d1[1]:
                my = (q0[1] + q1[1]) / 2
                pts += [(q0[0], my), (q1[0], my)]
            elif d0[0]:
                pts += [(q1[0], q0[1])]
            else:
                pts += [(q0[0], q1[1])]
        pts += [q1, p1]
        pts = simplify(pts)
        color = TONES[tone] if tone else "#6b6b6b"
        mid = tone or "base"
        dash = ' stroke-dasharray="6 6"' if dashed else ""
        marker = f' marker-end="url(#m-{mid})"' if head else ""
        self.edges.append(f'<path d="{rounded(pts)}" fill="none" stroke="{color}" stroke-width="1.7"{dash}{marker}/>')
        if label:
            lx, ly = point_along(pts, at)
            style = f"color:{color};border-color:{color}55" if tone else ""
            self.labels.append(f'<div class="elabel" style="left:{lx}px;top:{ly}px;{style}">{esc(label)}</div>')

    # ---- page -----------------------------------------------------------------------------------
    def page(self) -> str:
        markers = "".join(
            f'<marker id="m-{k}" viewBox="0 0 10 10" refX="9" refY="5" markerWidth="7" markerHeight="7" orient="auto-start-reverse">'
            f'<path d="M0,0 L10,5 L0,10 z" fill="{v}"/></marker>'
            for k, v in list(TONES.items()) + [("base", "#6b6b6b")])
        svg = (f'<svg class="edges" width="{WIDTH - 104}" height="{self.height}"><defs>{markers}</defs>'
               f'{"".join(self.edges)}</svg>')
        notes = "".join(f'<div class="n" style="color:#b5b5b5"><span style="color:{TONES[t]}">{icon(i)}</span>'
                        f'<span>{esc(n)}</span></div>' for n, i, t in self.notes)
        src = "".join(f'<div class="src">{esc(s)}</div>' for s in self.sources)
        foot = f'<div class="foot">{notes}{src}</div>' if notes or src else ""
        sub = f'<div class="sub">{esc(self.sub)}</div>' if self.sub else ""
        return (f'<!doctype html><html><head><meta charset="utf-8"><style>{CSS % {"w": WIDTH}}</style></head><body>'
                f'<div id="fig"><div class="eyebrow" style="color:{TONES[self.tone]}">{esc(self.eyebrow)}</div>'
                f'<h1>{esc(self.title)}</h1>{sub}<div id="canvas" style="height:{self.height}px">{svg}'
                f'{"".join(self.parts)}{"".join(self.labels)}</div>{foot}</div></body></html>')


def simplify(pts):
    out = []
    for p in pts:
        if out and abs(out[-1][0] - p[0]) < 0.5 and abs(out[-1][1] - p[1]) < 0.5:
            continue
        out.append(p)
    i = 1
    while i < len(out) - 1:
        (ax, ay), (bx, by), (cx, cy) = out[i - 1], out[i], out[i + 1]
        if (abs(ax - bx) < 0.5 and abs(bx - cx) < 0.5) or (abs(ay - by) < 0.5 and abs(by - cy) < 0.5):
            out.pop(i)
        else:
            i += 1
    return out


def rounded(pts, r=12):
    d = f"M{pts[0][0]:.1f},{pts[0][1]:.1f}"
    for i in range(1, len(pts) - 1):
        (ax, ay), (bx, by), (cx, cy) = pts[i - 1], pts[i], pts[i + 1]
        l1 = max(abs(bx - ax), abs(by - ay))
        l2 = max(abs(cx - bx), abs(cy - by))
        rr = min(r, l1 / 2, l2 / 2)
        ux, uy = (bx - ax) / (l1 or 1), (by - ay) / (l1 or 1)
        vx, vy = (cx - bx) / (l2 or 1), (cy - by) / (l2 or 1)
        d += f" L{bx - ux * rr:.1f},{by - uy * rr:.1f} Q{bx:.1f},{by:.1f} {bx + vx * rr:.1f},{by + vy * rr:.1f}"
    d += f" L{pts[-1][0]:.1f},{pts[-1][1]:.1f}"
    return d


def point_along(pts, t):
    lengths = [abs(b[0] - a[0]) + abs(b[1] - a[1]) for a, b in zip(pts, pts[1:])]
    goal = sum(lengths) * t
    for (a, b), length in zip(zip(pts, pts[1:]), lengths):
        if goal <= length and length:
            k = goal / length
            return a[0] + (b[0] - a[0]) * k, a[1] + (b[1] - a[1]) * k
        goal -= length
    return pts[-1]


def render(figs: list[Fig]) -> None:
    from PIL import Image
    from playwright.sync_api import sync_playwright

    with sync_playwright() as p:
        browser = None
        for channel in ("chrome", "msedge", None):
            try:
                browser = p.chromium.launch(channel=channel) if channel else p.chromium.launch()
                break
            except Exception:
                continue
        if browser is None:
            sys.exit("No Chrome, Edge or Playwright Chromium found")
        page = browser.new_page(viewport={"width": WIDTH, "height": 800}, device_scale_factor=SCALE)
        for fig in figs:
            page.set_content(fig.page(), wait_until="load")
            over = page.evaluate("""() => [...document.querySelectorAll('.card,.pill')]
                .filter(e => { const b = e.getBoundingClientRect();
                    return [...e.querySelectorAll('*')].some(c => { const r = c.getBoundingClientRect();
                        return r.bottom > b.bottom - 8 || r.right > b.right - 6; }); })
                .map(e => e.innerText.split(String.fromCharCode(10))[0])""")
            if over:
                print(f"  TEXT OVERFLOWS in {fig.name}: {over}")
            png = page.locator("#fig").screenshot()
            image = Image.open(io.BytesIO(png)).convert("RGB")
            image = image.quantize(colors=256, method=Image.Quantize.MEDIANCUT, dither=Image.Dither.NONE)
            path = ASSETS / fig.name
            path.parent.mkdir(parents=True, exist_ok=True)
            image.save(path, optimize=True)
            print(f"wrote {path.relative_to(ROOT).as_posix()} {image.size[0]}x{image.size[1]} "
                  f"{path.stat().st_size // 1024} KB")
        browser.close()


if __name__ == "__main__":
    sys.path.insert(0, str(Path(__file__).resolve().parent))
    from figures import FIGURES

    wanted = set(sys.argv[1:])
    chosen = [f() for f in FIGURES]
    if wanted:
        chosen = [f for f in chosen if any(w in f.name for w in wanted)]
    render(chosen)
