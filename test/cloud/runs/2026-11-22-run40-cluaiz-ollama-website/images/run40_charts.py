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


# 1. Memory
f = fig("Same model, same settings: Cluaiz used the least memory",
        "Lower is better. More free memory means more room for your app and database.")
ax = f.add_axes([0.06, 0.14, 0.40, 0.62])
bars(ax, ["Whole machine", "Engine only"], ["Cluaiz", "llama.cpp", "Ollama"],
     {"Cluaiz": [1759, 1139], "llama.cpp": [1811, 1147], "Ollama": [2055, 1554]}, lambda v: f"{v:,} MB")
style(ax, "1-hour chat soak, Qwen3 1.7B, 3 engines", "Peak memory (MB)")
ax.set_ylim(0, 2500)
f.text(0.06, 0.075, SRC26, fontsize=12, color=DIM)
ax2 = f.add_axes([0.56, 0.14, 0.40, 0.62])
bars(ax2, ["Qwen3 0.6B", "Qwen3 1.7B"], ["Cluaiz", "Ollama"],
     {"Cluaiz": [560, 1004], "Ollama": [788, 1448]}, lambda v: f"{v:,} MB", width=0.34)
style(ax2, "Engine memory, 8 GB machine", "Peak engine memory (MB)")
ax2.set_ylim(0, 1800)
f.text(0.56, 0.09, "Google Cloud 2 vCPU. Same numbers on the 4 GB machine.", fontsize=12, color=DIM)
callout(f, 0.66, 0.70, "~30% less\nthan Ollama")
callout(f, 0.30, 0.70, "Same work in the hour:\n1,048 / 1,073 / 1,102\nrequests, all OK")
f.savefig(os.path.join(OUT, "1-memory.png"), facecolor=BG); plt.close(f)

# 2. Speed
f = fig("Speed: level with llama.cpp and Ollama",
        "Cluaiz runs on llama.cpp, so one request gets the same speed. Seconds per request: lower is better.")
ax = f.add_axes([0.06, 0.14, 0.26, 0.62])
bars(ax, ["Qwen3 0.6B"], ["Cluaiz", "llama.cpp", "Ollama"],
     {"Cluaiz": [36.3], "llama.cpp": [35.8], "Ollama": [35.3]}, lambda v: f"{v} t/s")
style(ax, "Generation speed, 1 vCPU", "Tokens per second (higher is better)")
ax.set_ylim(0, 45)
f.text(0.06, 0.09, "1 vCPU VPS, same settings", fontsize=12, color=DIM)
ax2 = f.add_axes([0.40, 0.14, 0.56, 0.62])
bars(ax2, ["0.6B, 1 at a time", "1.7B, 1 at a time", "0.6B, 4 at once", "1.7B, 4 at once"], ["Cluaiz", "Ollama"],
     {"Cluaiz": [1.33, 2.79, 5.33, 11.34], "Ollama": [1.31, 3.72, 4.50, 10.04]}, lambda v: f"{v:.2f} s", width=0.36)
style(ax2, "Seconds per request, 8 GB machine, website running", "Seconds per request (lower is better)")
ax2.set_ylim(0, 13.5)
f.text(0.40, 0.09, "Mean of two rounds. 1 at a time: level or Cluaiz sooner. 4 at once: Ollama ~12-15% sooner.",
       fontsize=12, color=DIM)
f.savefig(os.path.join(OUT, "2-speed.png"), facecolor=BG); plt.close(f)

# 3. All in one
f = fig("Chat, embeddings, text to speech and speech to text in one program",
        "Cluaiz alone for 1 hour on a 1 vCPU VPS, 16 requests in flight, a web app running next to it")
ax = f.add_axes([0.06, 0.14, 0.42, 0.62])
names, vals = ["Chat", "Embeddings", "Text to speech", "Speech to text"], [698, 204, 81, 80]
b = ax.bar(names, vals, 0.6, color=["#ffffff", "#d0d0d0", "#a8a8a8", "#808080"], edgecolor="#000000")
for i, v in enumerate(vals):
    ax.text(i, v, f"{v} / {v} OK", ha="center", va="bottom", fontsize=14, fontweight="bold", color=TXT)
ax.set_xticks(range(4)); ax.set_xticklabels(names, fontsize=13, fontweight="bold")
style(ax, "Requests completed in 1 hour", "Requests")
ax.set_ylim(0, 820)
f.text(0.06, 0.075, SRC26, fontsize=12, color=DIM)
ax2 = f.add_axes([0.58, 0.14, 0.38, 0.62])
ax2.barh(["Total RAM", "Machine peak"], [3915, 2434], color=["#444444", "#ffffff"], edgecolor="#000000", height=0.35)
ax2.set_ylim(-0.5, 2.6)
for y, v in enumerate([3915, 2434]):
    ax2.text(v + 40, y, f"{v:,} MB", va="center", fontsize=14, fontweight="bold", color=TXT)
ax2.set_xlim(0, 4800)
style(ax2, "Memory, all four models loaded", "")
ax2.grid(axis="x", color=GRID, linestyle="--"); ax2.grid(axis="y", visible=False)
ax2.tick_params(axis="y", labelsize=14)
callout(f, 0.60, 0.74, "1,063 / 1,063 requests OK\nWeb app: 3,492 / 3,492 checks OK\nNo out-of-memory kill")
f.savefig(os.path.join(OUT, "3-all-in-one.png"), facecolor=BG); plt.close(f)

# 4. Website + parallel embeddings/speech
f = fig("Your website keeps running while the AI works",
        "Node.js + PostgreSQL shop, 20 simulated users, ~1,750 requests a minute, the whole time the engines ran")
ax = f.add_axes([0.06, 0.14, 0.42, 0.62])
x = range(3)
for i, (m, vals) in enumerate([("4 GB", [7, 16]), ("8 GB", [4, 11])]):
    xs = [0 + (i - 0.5) * 0.36, 1 + (i - 0.5) * 0.36]
    ax.bar(xs, vals, 0.33, color=["#ffffff", "#8a8a8a"][i], edgecolor="#000000", label=f"{m} machine")
    for xx, v in zip(xs, vals):
        ax.text(xx, v, f"{v} ms", ha="center", va="bottom", fontsize=14, fontweight="bold", color=TXT)
ax.set_xticks([0, 1]); ax.set_xticklabels(["Median page", "95% of pages within"], fontsize=14, fontweight="bold")
ax.legend(frameon=False, loc="upper left", fontsize=13)
style(ax, "Website response time with AI running", "Milliseconds (lower is better)")
ax.set_ylim(0, 22)
f.text(0.06, 0.09, "Google Cloud 2 vCPU, 4 GB and 8 GB", fontsize=12, color=DIM)
callout(f, 0.07, 0.62, "90,111 / 90,111 and\n88,212 / 88,212 requests OK")
ax2 = f.add_axes([0.58, 0.14, 0.38, 0.62])
lv = ["1 at once", "2 at once", "4 at once"]
for i, (n, vals) in enumerate([("Embeddings", [10, 12, 14]), ("Speech (TTS + STT)", [17, 18, 21])]):
    xs = [j + (i - 0.5) * 0.36 for j in range(3)]
    ax2.bar(xs, vals, 0.33, color=["#ffffff", "#8a8a8a"][i], edgecolor="#000000", label=n)
    for xx, v in zip(xs, vals):
        ax2.text(xx, v, f"{v}/{v}", ha="center", va="bottom", fontsize=13, fontweight="bold", color=TXT)
ax2.set_xticks(range(3)); ax2.set_xticklabels(lv, fontsize=14, fontweight="bold")
ax2.legend(frameon=False, loc="upper left", fontsize=13)
style(ax2, "Parallel embeddings and speech: all answered", "Requests OK (8 GB machine)")
ax2.set_ylim(0, 30)
f.text(0.58, 0.09, "Requests wait in line, none refused. 4 GB machine: same, all OK.", fontsize=12, color=DIM)
f.savefig(os.path.join(OUT, "4-website-and-parallel.png"), facecolor=BG); plt.close(f)
print(sorted(os.listdir(OUT)))
