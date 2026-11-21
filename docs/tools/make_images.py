#!/usr/bin/env python3
"""Draws the docs images in the cluaiz.com journal style: black background with a soft grey glow and a
faint dot grid, white headings, grey text, rounded dark cards. Black and white only.

The script is the editable source of every image; the PNGs in docs/assets/ are its output. Change a
drawing here and run it again; never edit a PNG by hand. Text in an image is also written in the page
next to it (the image is a summary, not the only copy).

Run from the repository root:  python docs/tools/make_images.py
Needs Pillow (`pip install pillow`). Fonts: Arial (Windows), Helvetica or DejaVu Sans elsewhere.
"""

from __future__ import annotations

from pathlib import Path

from PIL import Image, ImageDraw, ImageFont

ROOT = Path(__file__).resolve().parents[2]
ASSETS = ROOT / "docs" / "assets"

# The journal palette (sampled from cdn.cluaiz.com/journal/first-model-steps.png)
BG = (10, 10, 10)
GLOW = (38, 38, 38)
DOT = (26, 26, 26)
CARD = (22, 22, 22)
CARD_LINE = (58, 58, 58)
WHITE = (236, 236, 236)
GREY = (160, 160, 160)
DIM = (110, 110, 110)
ACCENT = (255, 255, 255)

WIDTH = 1200


def font(size: int, bold: bool = False) -> ImageFont.FreeTypeFont:
    names = (["arialbd.ttf", "Arial Bold.ttf", "Helvetica-Bold.ttf", "DejaVuSans-Bold.ttf"] if bold
             else ["arial.ttf", "Arial.ttf", "Helvetica.ttf", "DejaVuSans.ttf"])
    for name in names:
        for folder in ["", "C:/Windows/Fonts/", "/Library/Fonts/", "/usr/share/fonts/truetype/dejavu/"]:
            try:
                return ImageFont.truetype(folder + name, size)
            except OSError:
                continue
    return ImageFont.load_default()


def canvas(height: int) -> tuple[Image.Image, ImageDraw.ImageDraw]:
    image = Image.new("RGB", (WIDTH, height), BG)
    # Radial glow: brighter in the middle, black at the edges
    glow = Image.new("L", (WIDTH, height), 0)
    g = ImageDraw.Draw(glow)
    cx, cy = WIDTH // 2, height // 2
    steps = 60
    for i in range(steps, 0, -1):
        r = int(max(WIDTH, height) * 0.7 * i / steps)
        g.ellipse([cx - r, cy - r * 0.75, cx + r, cy + r * 0.75], fill=int(255 * (1 - i / steps)))
    image = Image.composite(Image.new("RGB", image.size, GLOW), image, glow.point(lambda v: v * 0.9))
    draw = ImageDraw.Draw(image)
    for x in range(12, WIDTH, 24):
        for y in range(12, height, 24):
            draw.point((x, y), fill=DOT)
    return image, draw


def title(draw: ImageDraw.ImageDraw, text: str, y: int = 48) -> None:
    draw.text((64, y), text, font=font(36, True), fill=WHITE)


def note(draw: ImageDraw.ImageDraw, text: str, y: int) -> None:
    draw.text((64, y), text, font=font(20), fill=GREY)


def card(draw: ImageDraw.ImageDraw, box: tuple[int, int, int, int], heading: str, lines: list[str], strong: bool = False) -> None:
    draw.rounded_rectangle(box, radius=14, fill=CARD, outline=ACCENT if strong else CARD_LINE, width=2 if strong else 1)
    x, y = box[0] + 20, box[1] + 18
    draw.text((x, y), heading, font=font(22, True), fill=WHITE)
    for i, line in enumerate(lines):
        draw.text((x, y + 36 + i * 26), line, font=font(18), fill=GREY)


def arrow(draw: ImageDraw.ImageDraw, start: tuple[int, int], end: tuple[int, int]) -> None:
    draw.line([start, end], fill=GREY, width=2)
    (x1, y1), (x2, y2) = start, end
    if abs(x2 - x1) >= abs(y2 - y1):
        d = 1 if x2 > x1 else -1
        draw.polygon([(x2, y2), (x2 - 10 * d, y2 - 6), (x2 - 10 * d, y2 + 6)], fill=GREY)
    else:
        d = 1 if y2 > y1 else -1
        draw.polygon([(x2, y2), (x2 - 6, y2 - 10 * d), (x2 + 6, y2 - 10 * d)], fill=GREY)


def save(image: Image.Image, relative: str) -> None:
    path = ASSETS / relative
    path.parent.mkdir(parents=True, exist_ok=True)
    image.save(path, optimize=True)
    print(f"wrote {path.relative_to(ROOT).as_posix()} {image.size[0]}x{image.size[1]}")


def reader_paths() -> None:
    """get-started/welcome.md: the three ways in"""
    image, draw = canvas(620)
    title(draw, "Three ways to start with Cluaiz")
    columns = [
        ("Use it", ["1  Install the Engine", "2  Download a model", "3  Chat in the terminal", "4  Give the agent a task"]),
        ("Build on it", ["1  Start cluaiz serve", "2  Send a first API request", "3  Stream and cancel", "4  Add tools, memory, speech"]),
        ("Create tools", ["1  Pick skill, WASM or MCP", "2  Run it locally", "3  Check its permissions", "4  Publish to the registry"]),
    ]
    for i, (heading, steps) in enumerate(columns):
        x = 64 + i * 366
        card(draw, (x, 130, x + 340, 470), heading, [])
        for j, step in enumerate(steps):
            draw.text((x + 20, 190 + j * 64), step, font=font(19), fill=GREY if j else WHITE)
            if j < len(steps) - 1:
                arrow(draw, (x + 34, 216 + j * 64), (x + 34, 246 + j * 64))
    note(draw, "Every path ends at the same reference pages: the API, the CLI and the settings.", 530)
    save(image, "get-started/reader-paths.png")


def two_walls() -> None:
    """engine/permission.md, troubleshooting: approvals, then the OS sandbox"""
    image, draw = canvas(560)
    title(draw, "Two walls around every agent command")
    card(draw, (64, 150, 324, 300), "The agent", ["wants to run", "npm test"])
    card(draw, (384, 150, 704, 300), "1  Approval policy", ["allow, ask or deny", "agent_security_mode"], strong=True)
    card(draw, (764, 150, 1136, 300), "2  OS sandbox", ["bubblewrap (Linux)", "sandbox-exec (macOS)", "AppContainer (Windows)"], strong=True)
    arrow(draw, (324, 225), (384, 225))
    arrow(draw, (704, 225), (764, 225))
    card(draw, (384, 360, 1136, 460), "The command sees", ["the project folder; not ~/.ssh, ~/.aws, your home or Cluaiz's files"])
    arrow(draw, (950, 300), (950, 360))
    note(draw, "Setting os_sandbox: auto (default), required or off.  cluaiz permission os-sandbox <mode>", 492)
    save(image, "engine/two-walls.png")


def request_queue() -> None:
    """api/inference/embeddings.md, troubleshooting: one model, requests wait in line"""
    image, draw = canvas(560)
    title(draw, "One model serves one request; the others wait in line")
    left, right, scale = 250, 1120, 870 / 30  # 0-30 seconds
    # (name, arrives, starts, ends): each request runs about 8 s in this example
    rows = [("Request A", 0, 0, 8), ("Request B", 2, 8, 16), ("Request C", 3, 16, 24)]
    for i, (name, arrives, starts, ends) in enumerate(rows):
        y = 150 + i * 80
        draw.text((64, y + 8), name, font=font(20, True), fill=WHITE)
        if starts > arrives:
            draw.rounded_rectangle([left + arrives * scale, y, left + starts * scale, y + 40], radius=8, outline=GREY, width=2)
            draw.text((left + arrives * scale + 12, y + 9), "waits", font=font(18), fill=GREY)
        draw.rounded_rectangle([left + starts * scale, y, left + ends * scale, y + 40], radius=8, fill=ACCENT)
        draw.text((left + starts * scale + 12, y + 9), "runs", font=font(18, True), fill=BG)
    for second in (0, 5, 10, 15, 20, 25, 30):
        x = left + second * scale
        draw.line([(x, 395), (x, 405)], fill=DIM, width=2)
        draw.text((x - 10, 410), f"{second} s", font=font(16), fill=DIM)
    draw.line([(left, 400), (right, 400)], fill=DIM, width=1)
    note(draw, "Arrival order, one at a time: embeddings, speech and chat attachments.", 455)
    note(draw, "A request still waiting after 120 s gets HTTP 429 (embedding_busy, speech_busy, media_busy).", 489)
    save(image, "api/request-queue.png")


def thinking_budget() -> None:
    """api/inference/chat_completions.md: reasoning_effort as a share of the room"""
    image, draw = canvas(600)
    title(draw, "How much a model may think: reasoning_effort")
    rows = [("low", 0.25, "25% of the room, at most 2,048 tokens"),
            ("medium", 0.50, "50% of the room, at most 8,192 tokens"),
            ("high / auto", 0.75, "75% of the room, at most 32,768 tokens"),
            ("max", 1.00, "the whole room")]
    left, width = 250, 560
    for i, (name, share, text) in enumerate(rows):
        y = 140 + i * 80
        draw.text((64, y + 10), name, font=font(22, True), fill=WHITE)
        draw.rounded_rectangle([left, y, left + width, y + 44], radius=8, outline=CARD_LINE, width=1, fill=CARD)
        draw.rounded_rectangle([left, y, left + int(width * share), y + 44], radius=8, fill=ACCENT)
        draw.text((left + width + 20, y + 12), text, font=font(17), fill=GREY)
    note(draw, "Room = context left after the prompt, minus max_tokens kept for the answer.", 470)
    note(draw, "A number (e.g. 256) is that many tokens; 0 answers without thinking.", 504)
    save(image, "api/thinking-budget.png")


if __name__ == "__main__":
    reader_paths()
    two_walls()
    request_queue()
    thinking_budget()
