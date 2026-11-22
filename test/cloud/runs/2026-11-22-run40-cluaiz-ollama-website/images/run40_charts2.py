"""Docker, 4 GB vs 8 GB, and a scorecard of every test. Numbers from run 26 analysis.md / manifest and run 40 SUMMARY."""
import os, runpy
g = runpy.run_path(os.path.join(os.path.dirname(__file__), "run40_charts_lib.py"))
fig, style, bars, callout, OUT, BG, TXT, DIM, GRID, PANEL, plt = (g[k] for k in
    ("fig", "style", "bars", "callout", "OUT", "BG", "TXT", "DIM", "GRID", "PANEL", "plt"))
E3 = ["Cluaiz", "llama.cpp", "Ollama"]
M3 = ["Qwen3 0.6B", "Qwen3 1.7B", "Llama 3.2 3B"]

# 5. Docker
f = fig("In Docker: Cluaiz used the least memory, Ollama's image is 8.52 GB",
        "1 vCPU, 3.9 GB VPS, web app + PostgreSQL beside it. One request at a time. All three engines on the same CPU.")
ax = f.add_axes([0.06, 0.14, 0.42, 0.62])
bars(ax, M3, E3, {"Cluaiz": [1075, 1520, 2140], "llama.cpp": [1169, 1555, 2135], "Ollama": [1447, 2074, 2873]},
     lambda v: f"{v:,}")
style(ax, "Whole machine memory peak (MB)", "MB (lower is better)")
ax.set_ylim(0, 3400)
ax2 = f.add_axes([0.56, 0.14, 0.40, 0.62])
bars(ax2, M3, E3, {"Cluaiz": [1.89, 3.68, 6.44], "llama.cpp": [1.67, 3.89, 6.28], "Ollama": [1.61, 3.78, 6.50]},
     lambda v: f"{v:.2f}")
style(ax2, "Seconds per request", "Seconds (lower is better)")
ax2.set_ylim(0, 8)
callout(f, 0.07, 0.64, "Docker image on disk:\nOllama 8.52 GB, Cluaiz 405 MB\nDocker itself: +55 MB idle")
f.text(0.06, 0.075, "Older Cluaiz build. Speed is level on 1.7B and 3B; on 0.6B Cluaiz was slower here, "
       "a 60 ms per-request cost since fixed (now 2 ms).", fontsize=12, color=DIM)
f.savefig(os.path.join(OUT, "5-docker.png"), facecolor=BG); plt.close(f)

# 6. 4 GB vs 8 GB
f = fig("4 GB and 8 GB machines: same memory, same picture",
        "Google Cloud 2 vCPU, Node.js + PostgreSQL website with 20 users beside the engine. Seconds per request, mean of two rounds.")
groups = ["0.6B\n1 at a time", "1.7B\n1 at a time", "0.6B\n4 at once", "1.7B\n4 at once"]
for i, (title, c, o) in enumerate([("4 GB machine", [1.56, 3.25, 6.18, 13.21], [1.62, 5.19, 5.63, 12.93]),
                                   ("8 GB machine", [1.33, 2.79, 5.33, 11.34], [1.31, 3.72, 4.50, 10.04])]):
    ax = f.add_axes([0.06 + i * 0.48, 0.16, 0.42, 0.60])
    bars(ax, groups, ["Cluaiz", "Ollama"], {"Cluaiz": c, "Ollama": o}, lambda v: f"{v:.2f}", width=0.36)
    ax.set_xticklabels(groups, fontsize=12, fontweight="bold")
    style(ax, title, "Seconds per request (lower is better)")
    ax.set_ylim(0, 15)
f.text(0.06, 0.075, "Engine memory on both machines: Cluaiz 560 MB vs Ollama 782-788 MB (0.6B), 1,004 MB vs 1,440-1,448 MB (1.7B). "
       "Website: 90,111 / 90,111 OK (4 GB), 88,212 / 88,212 OK (8 GB).", fontsize=12, color=DIM)
f.savefig(os.path.join(OUT, "6-4gb-vs-8gb.png"), facecolor=BG); plt.close(f)

# 7. Scorecard
rows = [
    ("Test", "Machine", "Cluaiz", "llama.cpp", "Ollama"),
    ("Engine memory, Qwen3 1.7B", "2 vCPU, 4 / 8 GB", "1,004 MB", "-", "1,440-1,448 MB"),
    ("Machine memory, 1-hour chat soak", "1 vCPU VPS", "1,759 MB", "1,811 MB", "2,055 MB"),
    ("Machine memory in Docker, 1.7B", "1 vCPU VPS", "1,520 MB", "1,555 MB", "2,074 MB"),
    ("Docker image on disk", "-", "405 MB", "-", "8.52 GB"),
    ("Generation speed, 0.6B", "1 vCPU VPS", "36.3 t/s", "35.8 t/s", "35.3 t/s"),
    ("Per request, 1.7B, 1 at a time", "2 vCPU, 8 GB", "2.79 s", "-", "3.72 s"),
    ("Per request, 1.7B, 4 at once", "2 vCPU, 8 GB", "11.34 s", "-", "10.04 s"),
    ("1-hour chat soak, requests OK", "1 vCPU VPS", "1,048 / 1,048", "1,073 / 1,073", "1,102 / 1,102"),
    ("Chat + embeddings + TTS + STT, 1 hour", "1 vCPU VPS", "1,063 / 1,063", "no speech", "no speech"),
    ("Website requests OK beside the engine", "2 vCPU, 4 / 8 GB", "all OK", "-", "all OK"),
]
f = fig("Every test in one table", "\"-\" = not measured on that machine.")
ax = f.add_axes([0.04, 0.10, 0.92, 0.74]); ax.axis("off")
t = ax.table(cellText=rows[1:], colLabels=rows[0], loc="upper center", cellLoc="left",
             colWidths=[0.34, 0.17, 0.15, 0.15, 0.17])
t.auto_set_font_size(False); t.set_fontsize(15); t.scale(1, 3.1)
for (r, c), cell in t.get_celld().items():
    cell.set_edgecolor(GRID)
    cell.set_facecolor("#1d1d1d" if r == 0 else PANEL)
    cell.get_text().set_color("#ffffff" if (r == 0 or c == 2) else TXT)
    if r == 0 or c == 2:
        cell.get_text().set_fontweight("bold")
f.savefig(os.path.join(OUT, "7-every-test.png"), facecolor=BG); plt.close(f)
print(sorted(os.listdir(OUT)))
