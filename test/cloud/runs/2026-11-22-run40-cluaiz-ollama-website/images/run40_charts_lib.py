"""Clear charts for the small-server post: runs 26 (1 vCPU, 3 engines) and 40 (2 vCPU, website). Greyscale."""
import os
import matplotlib
matplotlib.use("Agg")
import matplotlib.pyplot as plt

OUT = os.path.dirname(os.path.abspath(__file__))
os.makedirs(OUT, exist_ok=True)
BG, PANEL, GRID, TXT, DIM = "#0b0b0b", "#151515", "#333333", "#f2f2f2", "#9a9a9a"
COL = {"Cluaiz": "#ffffff", "llama.cpp": "#a8a8a8", "Ollama": "#5c5c5c"}
plt.rcParams.update({"font.family": "Segoe UI", "font.size": 13, "text.color": TXT, "axes.labelcolor": DIM,
                     "xtick.color": TXT, "ytick.color": DIM, "axes.edgecolor": GRID, "figure.facecolor": BG,
                     "axes.facecolor": PANEL})
SRC26 = "1 vCPU, 3.9 GB VPS, web app + PostgreSQL running beside the engine"
SRC40 = "Google Cloud 2 vCPU, 4 GB and 8 GB, Node.js + PostgreSQL website with 20 users beside the engine"
FOOT = "Cluaiz is open source, built on llama.cpp and ONNX Runtime"


def fig(title, sub):
    f = plt.figure(figsize=(16, 9), dpi=120)
    f.text(0.04, 0.94, title, fontsize=30, fontweight="bold", color=TXT)
    f.text(0.04, 0.895, sub, fontsize=15, color=DIM)
    f.text(0.04, 0.025, FOOT, fontsize=12, color=DIM)
    return f


def style(ax, title, ylabel):
    ax.set_title(title, fontsize=17, fontweight="bold", color=TXT, loc="left", pad=14)
    ax.set_ylabel(ylabel)
    ax.grid(axis="y", color=GRID, linestyle="--", linewidth=0.8)
    ax.set_axisbelow(True)
    for s in ("top", "right"):
        ax.spines[s].set_visible(False)


def bars(ax, groups, engines, data, fmt, width=0.26):
    n = len(engines)
    for i, e in enumerate(engines):
        xs = [g + (i - (n - 1) / 2) * width for g in range(len(groups))]
        vals = [data[e][j] for j in range(len(groups))]
        b = ax.bar(xs, vals, width * 0.92, color=COL[e], edgecolor="#000000", label=e)
        for x, v in zip(xs, vals):
            ax.text(x, v, fmt(v), ha="center", va="bottom", fontsize=13, fontweight="bold", color=TXT)
    ax.set_xticks(range(len(groups)))
    ax.set_xticklabels(groups, fontsize=14, fontweight="bold")
    ax.legend(frameon=False, loc="upper left", fontsize=13)


def callout(f, x, y, text):
    f.text(x, y, text, fontsize=15, fontweight="bold", color="#000000", va="top",
           bbox=dict(boxstyle="round,pad=0.6", facecolor="#ffffff", edgecolor="#ffffff"))


