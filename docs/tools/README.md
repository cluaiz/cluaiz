# Docs tooling (`docs/tools/`)

Checks for the Engine docs. Not shown on the website (only pages listed in `docs/registry.json` are).

| File | What it does |
|---|---|
| [`make_images.py`](make_images.py) | Turns each figure into an HTML page (cards are HTML so text wraps, arrows are SVG), renders it in headless Chrome or Edge through Playwright and saves a PNG into `docs/assets/<topic>/`. Warns when a card's text does not fit (`TEXT OVERFLOWS`). Needs `pip install playwright pillow` and Chrome, Edge or `playwright install chromium` |
| [`figures.py`](figures.py) | Every figure as a layout: cards with an icon, decision diamonds, frames, arrows, header and footer, in CSS pixels on a 1096 px canvas. A new figure is one function added to `FIGURES` |
| [`icons/`](icons/) | The Lucide icons the figures use (ISC licence, `icons/LICENSE`) |
| [`check_docs.py`](check_docs.py) | Every page in `docs/registry.json` exists and slugs are unique; every relative link and `#heading` anchor in `docs/**/*.md`, `README.md` and every folder README (outside `target`, `.agent`, `llama.cpp`) resolves; no page links a local machine path. No network, no dependencies |

Images: `python docs/tools/make_images.py` redraws all of them; `python docs/tools/make_images.py kv-reuse` redraws
the ones whose file name contains that text. Rules: black, white and greys only (the cluaiz.com theme, owner
2026-11-21): the element a figure is about has a white border, side or idle states are darker; never a colour.
One image only where a picture explains faster than text (a flow, a decision, an order in time, a comparison),
never decoration; every fact in it also in the text next to it, with its source; alt text that says what the
image shows. After a change, open the PNG and look at it: arrows must not cross cards or labels. Each image
is linked with a relative path (`../assets/...`), which GitHub shows and the website serves from
`raw.githubusercontent.com` (`resolveDocSrc` in the website renderer).

Run from the repository root: `python docs/tools/check_docs.py` (exit 1 with one line per problem).
CI: `.github/workflows/docs-check.yml` runs it when `docs/` or `README.md` changes.

Decisions:
- Figures are HTML rendered by a browser, not drawn with Pillow: text wraps and fonts render properly,
  and a figure is a short layout instead of pixel arithmetic. The cost is a browser at build time; the PNGs
  are committed, so readers and the website need nothing.
- Relative `.md` links are the one form that works on both GitHub and cluaiz.com (the website's renderer
  turns them into its routes, `cluaiz.com/src/components/organisms/docs/markdown-renderer.tsx`
  `resolveDocHref`). Website routes such as `/docs/get-started/quickstart` work only on the website.
- Anchors follow GitHub's slugs: a heading's emoji is dropped but leaves a leading dash, so removing an
  emoji from a heading breaks its old `#-name` links. The check catches that.
- External links are not fetched: they break for reasons outside the repo and need a scheduled run with a
  cache (lychee is the usual tool), not a gate on every change.

First run (2026-11-21): 24 problems in 200 pages: 8 anchors still written for old emoji headings, 16 links in
`get-started/welcome.md` and `reference/index.md` that only worked on the website. All fixed.

## Pending

| Item | Why it is open |
|---|---|
| External link check (scheduled, cached) | Needs network and a cache; separate from the per-change gate |
| Prose lint (style, banned hype words) | Rules live in `.agents/skills/human-writing`; not yet machine-checked |
| Running the code examples in the docs | Examples need a server and a model; done by hand per release today |
