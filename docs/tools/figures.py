"""Every docs image as a layout. Coordinates are CSS pixels inside a 1096 px wide canvas.

Black, white and greys only (the cluaiz.com theme). Tones: "white" marks the element the figure is
about, "light" and "mid" are ordinary steps, "dim" is a side or idle state.

Each fact in a figure is also written in the page that shows it, with its source (README, code or a
recorded run). Change a figure here, then run make_images.py.
"""

from make_images import Fig, esc, icon

W = 1096  # canvas width
HATCH = "repeating-linear-gradient(135deg, rgba(255,255,255,.10) 0 6px, transparent 6px 12px)"


# ---------------------------------------------------------------------------------------------- get started

def reader_paths() -> Fig:
    f = Fig("get-started/reader-paths.png", "Get started", "Three ways to start with Cluaiz",
            "Pick the path that matches what you want to do. All three meet at the same reference pages.", height=462)
    paths = [
        ("Use it", [("Install the Engine", "download"), ("Download a model", "cloud-download"),
                    ("Chat in the terminal", "square-terminal"), ("Give the agent a task", "bot")]),
        ("Build on it", [("Start cluaiz serve", "server"), ("Send a first API request", "send"),
                         ("Stream and cancel", "radio"), ("Add tools, memory, speech", "blocks")]),
        ("Create tools", [("Pick skill, WASM or MCP", "puzzle"), ("Run it locally", "play"),
                          ("Check its permissions", "shield-check"), ("Publish to the registry", "package")]),
    ]
    f.card("ref", 248, 362, 600, 100, "Reference pages", "The API, the CLI and the settings", "book-open", "white", strong=True)
    for c, (label, steps) in enumerate(paths):
        x = c * 376
        f.group(x, 0, 344, 304, label, "light")
        for i, (step, ic) in enumerate(steps):
            f.pill(f"p{c}{i}", x + 20, 30 + i * 66, 304, 46, f"{i + 1}  {step}", "", ic, "white" if i == 0 else "mid")
            if i:
                f.edge(f"p{c}{i - 1}", f"p{c}{i}", "b", "t")
        f.edge(f"p{c}3", "ref", "b", "t", ob=[0.15, 0.5, 0.85][c])
    f.source("Pages: docs/get-started/")
    return f


def model_download() -> Fig:
    f = Fig("get-started/model-download.png", "Models · cluaiz pull", "What cluaiz pull does",
            "From the name you type to the installed ID the API calls. A broken download resumes where it stopped.",
            height=330)
    f.card("id", 0, 0, 240, 128, "Model id", "", "hash", "light", num=1, chips=["qwen3", "owner/repo", "hf://…", "a URL", ":Q4_K_M"])
    f.card("resolve", 280, 0, 250, 128, "Resolve", "Short id to a Hugging Face repo; pick the variant", "search", "light", num=2)
    f.card("plan", 570, 0, 240, 128, "Plan the files", "Shards, companion files, config", "list", "light", num=3)
    f.card("dl", 850, 0, 246, 128, "Download", "`<name>.part`, resumes by byte range", "download", "white", num=4, strong=True)
    f.card("check", 850, 200, 246, 128, "Check", "The main file and its assets are complete", "circle-check", "light", num=5)
    f.card("reg", 570, 200, 240, 128, "Registry", "`model_registry.json` gets the installed ID", "database", "light", num=6)
    f.card("list", 280, 200, 250, 128, "cluaiz list", "Shows the installed ID: the `model` the API takes", "list-checks", "white", num=7, strong=True)
    for a, b in [("id", "resolve"), ("resolve", "plan"), ("plan", "dl")]:
        f.edge(a, b)
    f.edge("dl", "check", "b", "t")
    f.edge("check", "reg", "l", "r")
    f.edge("reg", "list", "l", "r")
    f.note("The installed ID is not the repository name. Use the one `cluaiz list` prints.")
    f.source("Details: engine/models/src/fetcher/README.md")
    return f


def from_ollama() -> Fig:
    f = Fig("get-started/from-ollama.png", "Get started · coming from Ollama", "From Ollama to Cluaiz",
            "The commands map one to one, and the Ollama library works against the Engine's Ollama routes (run 36).",
            height=730)
    rows = [("ollama pull", "cluaiz pull", "Hugging Face GGUF or a catalogue id", "same"),
            ("ollama run · list · rm · show · ps", "cluaiz run · list · rm · show · ps", "show also prints the plan for this machine", "same"),
            ("ollama serve  (port 11434)", "cluaiz serve  (port 8000)", "`CLUAIZ_PORT`, `CLUAIZ_HOST` for one run", "same"),
            ("POST /api/chat", "POST /api/chat", "Ollama's request and reply; streams one JSON object per line", "same"),
            ("POST /api/generate · /api/embed", "the same routes", "generate as one chat turn; embed through /v1/embeddings", "same"),
            ("GET /api/ps · POST /api/show", "cluaiz ps · cluaiz show", "these two routes are not there (HTTP 404)", "differs"),
            ("Modelfile", "settings.json and the request", "load settings, samplers, the system message", "differs")]
    marks = {"same": ("circle-check", "white"), "differs": ("triangle-alert", "mid")}
    f.text(0, 0, 400, "OLLAMA", 12, "#8a8a8a", weight=700)
    f.text(470, 0, 400, "CLUAIZ", 12, "#8a8a8a", weight=700)
    for i, (a, b, note, kind) in enumerate(rows):
        y = 28 + i * 100
        ic, tone = marks[kind]
        f.pill(f"o{i}", 0, y + 18, 400, 52, a, "", "square-terminal" if i < 3 or i == 6 else "globe", "dim")
        f.card(f"c{i}", 470, y, 626, 88, b, note, ic, tone, strong=kind == "same" and i == 0, mono=False)
        f.edge(f"o{i}", f"c{i}")
    f.note("Ollama's downloaded model files are not imported; pull the model again from Hugging Face.")
    f.note("keep_alive on the Ollama routes works as in Ollama (a number is seconds, \"5m\" a duration); on the OpenAI routes a number is minutes.", "timer")
    f.source("Details: docs/get-started/coming-from-ollama.md  ·  Run: test/cloud/runs/2026-11-21-run36-ollama-library")
    return f


# ---------------------------------------------------------------------------------------------- engine

def crate_layers() -> Fig:
    f = Fig("engine/crate-layers.png", "Engine · crates", "The Engine in layers: who may call whom",
            "A crate depends only on crates below it (from each Cargo.toml, 2026-11-21).", height=714)
    bands = [
        ("Entry points", [("api", "HTTP and CEP routes", "globe"), ("cmd", "The CLI", "square-terminal"),
                          ("automation", "Flows", "workflow")]),
        ("Model host and agents", [("runtime", "Loads, shares and unloads models; queues", "server"),
                                   ("agents", "Tools, agent turn, memory, OS sandbox", "bot")]),
        ("Engines and planning", [("plan", "GPU layers, context, KV type", "gauge"), ("llama", "llama.cpp: GGUF chat, thinking, tools", "brain"),
                                  ("onnx", "Speech, embeddings, vision", "radio"), ("media", "Images, audio, documents", "file-text")]),
        ("Foundation", [("models", "Files, catalogue, GGUF header", "package"), ("hardware", "CPU, RAM, GPU, free memory", "cpu"),
                        ("core", "Home, settings, prompts; no engine crate", "layers")]),
    ]
    for b, (label, crates) in enumerate(bands):
        y = b * 190
        f.group(0, y, W, 150, label, "light" if b else "white", id=f"b{b}")
        n = len(crates)
        cw = (W - 48 - (n - 1) * 20) / n
        for i, (name, text, ic) in enumerate(crates):
            f.card(f"c{b}{i}", 24 + i * (cw + 20), y + 26, cw, 106, name, text, ic,
                   "white" if b == 0 or name == "runtime" else "mid", strong=name == "runtime", mono=True)
        if b:
            f.edge(f"b{b - 1}", f"b{b}", "b", "t", label="depends on" if b == 1 else None)
    f.note("api, cmd and automation reach models only through runtime. core depends on no Engine crate.")
    f.source("Details: engine/README.md")
    return f


def chat_request() -> Fig:
    f = Fig("engine/chat-request.png", "Engine · chat", "One chat request through the Engine",
            "Steps 3 to 5 run only when the model is not loaded yet with the same settings.", height=396)
    f.group(400, 0, 696, 186, "Only on a new load", "light")
    f.card("api", 0, 34, 170, 132, "api", "The chat handler receives the request", "globe", "light", num=1, mono=True)
    f.card("rt", 205, 34, 170, 132, "runtime", "`ensure_loaded()`: the model with these settings", "server", "white", num=2, strong=True, mono=True)
    f.card("hw", 420, 34, 200, 132, "hardware", "Free RAM and VRAM, measured now", "cpu", "mid", num=3, mono=True)
    f.card("plan", 650, 34, 200, 132, "plan", "GPU layers, context, KV cache type", "gauge", "mid", num=4, mono=True)
    f.card("load", 876, 34, 200, 132, "llama load", "llama.cpp loads the GGUF with the plan", "brain", "mid", num=5, mono=True)
    f.card("stream", 420, 270, 260, 116, "stream", "Prompt, sampling, thinking budget", "radio", "white", num=6, mono=True)
    f.card("ans", 740, 270, 356, 116, "Answer", "Tokens to the client, then timings and reports", "message-square", "light", num=7)
    f.edge("api", "rt")
    f.edge("rt", "hw")
    f.edge("hw", "plan")
    f.edge("plan", "load")
    f.edge("load", "stream", "b", "t", ob=0.5)
    f.edge("rt", "stream", "b", "l", label="already loaded: skip 3 to 5", dashed=True, at=0.35)
    f.edge("stream", "ans")
    f.source("Code: api/src/handlers/chat, engine/runtime/src/host.rs, engine/llama/src/native/stream.rs")
    return f


def hardware_profile() -> Fig:
    f = Fig("engine/hardware-profile.png", "Engine · hardware", "How the Engine learns the hardware",
            "A fast check at start decides whether the saved profile is still right. Free memory is measured again before every load.",
            height=270)
    f.card("start", 0, 60, 186, 132, "Start", "Reads the saved profile, `hardware_profile.json`", "power", "light", num=1)
    f.card("quick", 216, 60, 206, 132, "Quick facts", "Total RAM, CPU threads, NVIDIA GPUs (milliseconds)", "zap", "light", num=2)
    f.diamond("changed", 452, 66, 176, 120, "Changed or missing?")
    f.card("saved", 690, 0, 200, 116, "Saved profile", "Used as it is, plus free memory now", "archive", "light")
    f.card("probe", 690, 156, 200, 112, "Deep probe", "Seconds; the profile is saved again", "scan-search", "mid")
    f.card("live", 930, 60, 166, 132, "Before a load", "`with_live_memory()`: free RAM and VRAM again", "gauge", "white", strong=True)
    f.edge("start", "quick")
    f.edge("quick", "changed")
    f.edge("changed", "saved", "t", "l", label="no", via=[(540, 52)])
    f.edge("changed", "probe", "b", "l", label="yes", via=[(540, 212)])
    f.edge("saved", "live", "r", "l", ob=0.3)
    f.edge("probe", "live", "r", "l", ob=0.72)
    f.note("Changed means: RAM moved by more than 1/32, or the CPU thread count or the GPU list differs.")
    f.note("Non-NVIDIA GPUs (Metal, Vulkan) come from the device list of llama.cpp.")
    f.source("Details: engine/hardware/README.md")
    return f


def permission_copies() -> Fig:
    f = Fig("engine/permission-copies.png", "Security · permission.json", "permission.json has two copies while the server runs",
            "The token check reads the copy in memory. Only a CEP call updates both copies.", height=300)
    f.card("cli", 0, 0, 250, 112, "CLI", "`cluaiz token create`, `cluaiz permission …`", "square-terminal", "light")
    f.card("cep", 0, 188, 250, 112, "CEP", "`config.set`, `tokens.create`", "network", "light")
    f.card("file", 400, 0, 260, 112, "permission.json", "On disk in `<home>/config`", "file-json", "mid", mono=True)
    f.card("copy", 400, 188, 260, 112, "Server copy", "In memory, loaded once at boot", "memory-stick", "white", strong=True)
    f.card("check", 810, 188, 286, 112, "Bearer-token check", "Reads the server copy only", "key", "light")
    f.card("warn", 810, 0, 286, 112, "After a CLI change", "Restart the server; until then the check does not see it (run 29)", "triangle-alert", "white")
    f.edge("cli", "file", label="writes the file")
    f.edge("cep", "file", "r", "l", oa=0.3, ob=0.75, label="writes the file", at=0.2)
    f.edge("cep", "copy", "r", "l", oa=0.7, ob=0.7, label="and the copy", at=0.5)
    f.edge("file", "copy", "b", "t", label="read once at boot", dashed=True)
    f.edge("copy", "check", label="reads")
    f.source("Details: security/README.md")
    return f


def two_walls() -> Fig:
    f = Fig("engine/two-walls.png", "Agents · safety", "Two walls around every agent command",
            "A command must pass the approval policy, then runs inside the OS sandbox.", height=340)
    f.card("agent", 0, 40, 190, 112, "The agent", "Wants to run `npm test`", "bot", "light")
    f.card("w1", 250, 20, 260, 152, "1  Approval policy", "allow, ask or deny", "shield-check", "white", strong=True,
           chips=["agent_security_mode"])
    f.card("w2", 570, 20, 300, 152, "2  OS sandbox", "", "lock", "white", strong=True,
           chips=["bubblewrap · Linux", "sandbox-exec · macOS", "AppContainer · Windows"])
    f.card("sees", 930, 20, 166, 152, "Runs", "Sees the project folder", "folder", "light")
    f.card("deny", 250, 248, 260, 92, "Not run", "Denied, or the user said no", "ban", "dim")
    f.card("hidden", 570, 232, 526, 108, "Hidden from the command", "", "eye-off", "mid",
           chips=["~/.ssh", "~/.aws", "your home", "Cluaiz's own files"])
    f.edge("agent", "w1")
    f.edge("w1", "w2", label="allowed")
    f.edge("w2", "sees")
    f.edge("w1", "deny", "b", "t", label="deny", dashed=True)
    f.edge("w2", "hidden", "b", "t", ob=0.285, label="masked")
    f.note("Setting `os_sandbox`: auto (the default), required or off. Change it with `cluaiz permission os-sandbox <mode>`.")
    f.source("Details: engine/permission.md, agents/src/execution/README.md")
    return f


def agent_turn() -> Fig:
    f = Fig("engine/agent-turn.png", "Agents · one turn", "One agent turn",
            "The model answers or asks for tools. Each tool call is approved, run, and its result goes back to the model.",
            height=450)
    f.card("msg", 0, 0, 200, 112, "Message", "The user's request", "message-square", "light", num=1)
    f.card("search", 230, 0, 200, 112, "Tool search", "Installed tools that match", "search", "light", num=2)
    f.card("schema", 460, 0, 200, 112, "Schemas", "Matching tools, within a size budget", "file-json", "light", num=3)
    f.card("model", 690, 0, 200, 112, "Model", "Answers, or asks for tool calls", "brain", "white", num=4, strong=True)
    f.diamond("calls", 740, 168, 150, 108, "Tool calls?")
    f.card("answer", 946, 176, 150, 96, "Answer", "Turn ends", "circle-check", "light")
    f.card("approve", 690, 330, 200, 112, "Approval", "Security mode, tool permissions, chat grants", "shield-check", "white", num=5)
    f.card("run", 460, 330, 200, 112, "Run", "Skill, plugin, MCP or command (OS sandbox)", "play", "light", num=6)
    f.card("results", 230, 330, 200, 112, "Results", "Added to the conversation", "list-checks", "light", num=7)
    f.edge("msg", "search")
    f.edge("search", "schema")
    f.edge("schema", "model")
    f.edge("model", "calls", "b", "t", oa=0.625)
    f.edge("calls", "answer", "r", "l", label="no")
    f.edge("calls", "approve", "b", "t", ob=0.625, label="yes")
    f.edge("approve", "run", "l", "r")
    f.edge("run", "results", "l", "r")
    f.edge("results", "model", "t", "b", via=[(330, 236), (714, 236)], ob=0.12, label="the model answers again",
           tone="white", dashed=True, at=0.4)
    f.note("The turn also ends on a stop request or at the step limit. An unusable answer gets a short note, at most 2 times.")
    f.source("Code: agents/src/turn.rs  ·  Details: agents/README.md")
    return f


def plugin_runners() -> Fig:
    f = Fig("engine/plugin-runners.png", "Agents · plugins", "How a plugin runs",
            "A WebAssembly plugin runs inside the Engine with fixed limits. A native program runs as a child process in the OS sandbox.",
            height=400)
    f.card("call", 398, 0, 300, 92, "Plugin call", "A function and its JSON arguments", "plug", "white", strong=True)
    f.group(0, 150, 530, 250, "WebAssembly (.wasm) · inside the Engine", "white", id="g1")
    f.group(566, 150, 530, 250, "Native program · child process", "light", id="g2")
    f.card("lim", 20, 180, 490, 104, "Limits", "", "cpu", "light", chips=["2,000,000,000 fuel", "at most 256 MiB", "no files", "no programs"])
    f.card("net", 20, 298, 490, 88, "Network only through `cluaiz.http_fetch`", "https, public hosts, the hosts it declares; keys filled in by the Engine", "globe", "mid")
    f.card("sb", 586, 180, 490, 104, "OS sandbox", "", "lock", "light", chips=["secrets hidden", "home hidden", "its folder read-only"])
    f.card("win", 586, 298, 490, 88, "Windows with `os_sandbox: auto`", "Not confined. `os_sandbox: required` confines it.", "triangle-alert", "white")
    f.edge("call", "g1", "b", "t", oa=0.3, ob=0.5, label=".wasm")
    f.edge("call", "g2", "b", "t", oa=0.7, ob=0.5, label="native")
    f.source("Details: agents/src/plugins/README.md")
    return f


def mcp_server_life() -> Fig:
    f = Fig("engine/mcp-server-life.png", "Agents · MCP", "The life of an MCP server process",
            "One process per server, started on the first call and stopped when idle or crashing too often.", height=420)
    f.card("off", 0, 150, 190, 104, "Not running", "Started on the next call", "power", "dim")
    f.card("run", 330, 140, 250, 124, "Running", "One process serves every call; 60 s per request", "server", "white", strong=True)
    f.card("lru", 690, 0, 260, 104, "More than 4 running", "The least recently used one stops", "layers", "mid")
    f.card("crash", 640, 300, 200, 112, "Crash", "Restart after 1 s, then 2 s", "rotate-ccw", "light")
    f.card("dead", 900, 300, 196, 112, "Unavailable", "Until the 10-minute window passes", "ban", "dim")
    f.edge("off", "run", "r", "l", label="first call", at=0.5)
    f.edge("run", "off", "t", "t", via=[(455, 96), (95, 96)], label="idle 10 minutes", dashed=True)
    f.edge("run", "lru", "r", "l", oa=0.25, ob=0.5)
    f.edge("run", "crash", "r", "t", oa=0.75)
    f.edge("crash", "run", "l", "b", ob=0.3, label="restarted")
    f.edge("crash", "dead", label="3 in 10 min")
    f.note("First call: the process starts from a cleared environment and is initialized once (up to 180 s).")
    f.note("Transport: stdio only. On Windows a job object ends the server and everything it started.")
    f.source("Details: agents/src/mcp/README.md")
    return f


def auto_context() -> Fig:
    f = Fig("engine/auto-context.png", "Engine · plan", "Auto context and KV cache: stay on the GPU first",
            "Checked top to bottom; the first one that fits in free VRAM wins. The KV cache gets smaller before the context does.",
            height=500)
    tests = [("16,384 tokens of F16 KV fit?", "16,384 tokens · F16"),
             ("At least 4,096 tokens of Q8 fit?", "Largest that fits, up to 16,384 · Q8"),
             ("4,096 tokens of Q4 fit?", "4,096 tokens · Q4"),
             ("2,048 to 4,095 tokens fit?", "That many tokens · Q4")]
    for i, (q, r) in enumerate(tests):
        y = i * 104
        f.card(f"t{i}", 0, y, 520, 72, q, "", "gauge", "light", num=i + 1)
        f.pill(f"r{i}", 680, y + 11, 416, 50, r, "", "circle-check", "white" if i == 0 else "light")
        f.edge(f"t{i}", f"r{i}", label="yes")
        if i:
            f.edge(f"t{i - 1}", f"t{i}", "b", "t", oa=0.12, ob=0.12, label="no", at=0.5)
    f.card("t4", 0, 416, 520, 72, "Nothing fits on the GPU", "", "split", "dim", num=5)
    f.pill("r4", 680, 427, 416, 50, "Split GPU and RAM · 8,192 · Q8", "", "layers", "mid")
    f.edge("t3", "t4", "b", "t", oa=0.12, ob=0.12, label="no")
    f.edge("t4", "r4")
    f.note("Split mode drops to 4,096 tokens when RAM is short. CPU only keeps 4,096. A value the user sets is used as given.")
    f.source("Details: engine/plan/README.md (Auto context and KV cache)")
    return f


def memory_store() -> Fig:
    f = Fig("engine/memory-store.png", "Agents · memory", "How a chat is stored",
            "Every change is a new event in an append-only log. Nothing is deleted; an edit is a new event.", height=350)
    f.card("req", 0, 0, 200, 112, "Chat request", "The messages a client sends", "message-square", "light", num=1)
    f.card("diff", 236, 0, 236, 112, "transcript.rs", "Diff with the log: new, edited, retracted", "git-branch", "light", num=2, mono=True)
    f.card("red", 508, 0, 220, 112, "records.rs", "Secrets redacted", "eye-off", "light", num=3, mono=True)
    f.card("store", 764, 0, 332, 112, "EventStore", "Appends to the chat's log", "database", "white", num=4, strong=True)
    f.group(0, 190, W, 160, "On disk", "light", id="disk")
    f.card("ev", 24, 222, 336, 108, "events.jsonl", "`<home>/memory/chats/<chat>/`; full segments compressed (.zst)", "scroll-text", "light", mono=True)
    f.card("blob", 380, 222, 336, 108, "blobs", "Payloads over 8 KB, stored by sha256", "archive", "mid", mono=True)
    f.card("notes", 736, 222, 336, 108, "notes, vectors", "`notes.jsonl`; vectors per embedding model", "file-text", "mid", mono=True)
    f.edge("req", "diff")
    f.edge("diff", "red")
    f.edge("red", "store")
    f.edge("store", "disk", "b", "t", oa=0.5, ob=0.85)
    f.note("With `storage: external` the chat stays in RAM only and nothing is written.")
    f.source("Details: agents/src/memory/README.md")
    return f


def provider_call() -> Fig:
    f = Fig("engine/provider-call.png", "API · providers", "A chat sent to a cloud provider",
            "A provider model goes through the same chat route and comes back in the same shape as an installed model.",
            height=344)
    f.card("req", 0, 0, 250, 132, "Request", "`model: <provider>/<model>` from providers.json", "message-square", "light", num=1)
    f.card("compat", 282, 0, 250, 132, "Compat", "The provider's differences, told from its URL", "scale", "light", num=2)
    f.card("body", 564, 0, 250, 132, "Body", "Messages, samplers, limit, thinking, tools in its format", "file-json", "light", num=3)
    f.card("send", 846, 0, 250, 132, "Send", "Key from providers.json or the environment; retry and shrink on errors", "send", "white", num=4, strong=True)
    f.card("stream", 846, 212, 250, 132, "Stream", "Thinking, text, tool calls and usage decoded", "radio", "light", num=5)
    f.card("same", 470, 212, 316, 132, "Same output", "As an installed model: `<think>` and `<tool_call>` blocks", "circle-check", "white", num=6)
    f.card("use", 0, 212, 410, 132, "Agent, memory, tools", "Work the same with a provider model as with an installed one", "bot", "light")
    f.edge("req", "compat")
    f.edge("compat", "body")
    f.edge("body", "send")
    f.edge("send", "stream", "b", "t")
    f.edge("stream", "same", "l", "r")
    f.edge("same", "use", "l", "r")
    f.source("Details: api/src/providers/README.md")
    return f


def automation_layers() -> Fig:
    f = Fig("engine/automation-layers.png", "Automation", "Where Automation sits",
            "Automation is its own layer: not part of the Engine and not part of the API. It reaches the Engine through small hooks.",
            height=446)
    f.pill("cli", 348, 0, 400, 52, "cluaiz serve  ·  cluaiz flow", "", "square-terminal", "white")
    f.group(0, 96, W, 154, "Automation layer", "white")
    f.card("host", 24, 126, 500, 104, "automation/host", "Triggers, runs, records", "workflow", "white", strong=True, mono=True)
    f.card("core", 572, 126, 500, 104, "automation/core", "The flow format, checks and step kinds", "file-code", "light", mono=True)
    f.group(0, 306, W, 140, "Engine hooks", "light", id="eng")
    hooks = [("Events", "radio"), ("Extension routes and tools", "plug"), ("Long approvals", "stamp"), ("WebAssembly runner", "cpu")]
    for i, (h, ic) in enumerate(hooks):
        f.card(f"h{i}", 24 + i * 266, 334, 248, 88, h, "", ic, "mid")
    f.edge("cli", "host", "b", "t", ob=0.65)
    f.edge("host", "core", label="uses")
    f.edge("host", "eng", "b", "t", oa=0.5, ob=0.23, label="small hooks")
    f.source("Details: automation/README.md")
    return f


def kv_reuse() -> Fig:
    f = Fig("engine/kv-reuse.png", "Engine · KV cache", "The next turn reuses the KV cache",
            "Only the tokens after the common prefix are computed. A saved chat is read back from disk after a reload or restart.",
            height=430)

    def seg(x, w, y, label, bg, fg="#efefef"):
        return (f'<div style="position:absolute;left:{x}px;top:{y}px;width:{w - 4}px;height:44px;border-radius:10px;background:{bg};'
                f'display:flex;align-items:center;padding-left:12px;font-size:13.5px;font-weight:600;color:{fg}">{label}</div>')

    out = ['<div class="text" style="left:0;top:12px;width:150px;color:#efefef;font-weight:600;font-size:14.5px">In the KV cache</div>',
           '<div class="text" style="left:0;top:84px;width:150px;color:#efefef;font-weight:600;font-size:14.5px">New prompt</div>',
           seg(160, 520, 0, "system, tools, turns 1 to 3", "linear-gradient(90deg,#5a5a5a,#8f8f8f)"),
           seg(680, 200, 0, "last answer", "#3a3a3a"),
           seg(160, 520, 72, "the same tokens: kept", "linear-gradient(90deg,#5a5a5a,#8f8f8f)"),
           seg(680, 200, 72, "last answer: kept", "#3a3a3a"),
           seg(880, 216, 72, "new message: computed", "#f2f2f2", "#0a0a0a"),
           '<div style="position:absolute;left:160px;top:128px;width:716px;height:1.5px;background:#a6a6a6"></div>'
           '<div class="elabel" style="left:518px;top:130px">common prefix</div>']
    f.raw("".join(out))
    f.card("save", 0, 200, 330, 124, "After each answer", "The KV is saved to `<home>/kv_cache/chats/<chat>.kv`", "hard-drive", "light", num=1)
    f.card("later", 383, 200, 330, 124, "The chat continues later", "After a reload, another chat or a restart", "history", "light", num=2)
    f.diamond("same", 766, 200, 330, 124, "Same model file and load settings?")
    f.pill("yes", 766, 370, 150, 48, "Read back", "", "circle-check", "white")
    f.pill("no", 926, 370, 170, 48, "Computed again", "", "refresh-cw", "dim")
    f.edge("save", "later")
    f.edge("later", "same")
    f.edge("same", "yes", "b", "t", oa=0.5, ob=0.5, label="yes")
    f.edge("same", "no", "b", "t", oa=0.5, ob=0.5, label="no")
    f.note("Each reply says what happened: cache_n, processed_tokens and cause (cold, append, edit, history_drop, shift, restart, media).")
    f.note("Limits in settings.json, chat_memory: 5 GB on disk and 30 days by default. Private chats (temporary_chat) are not saved.")
    f.source("Code: engine/llama/src/native/stream.rs, engine/runtime/src/chat_memory.rs")
    return f


def moe_placement() -> Fig:
    f = Fig("engine/moe-placement.png", "Engine · MoE", "A MoE model larger than the GPU",
            "Like llama.cpp's --n-cpu-moe: the routed experts of the first N blocks stay in RAM; everything else goes on the GPU.",
            height=360)
    f.group(0, 0, 640, 254, "Blocks 1 to N", "light")
    f.group(676, 0, 420, 254, "Blocks N+1 to the end", "white")
    f.card("e1", 20, 30, 290, 100, "Routed experts", "In RAM (CPU buffers)", "memory-stick", "mid")
    f.card("a1", 330, 30, 290, 100, "Attention, shared, KV", "On the GPU", "cpu", "white")
    f.card("c1", 20, 144, 600, 96, "MoE GPU cache", "The most used experts copied to VRAM; the CPU computes a share of the misses", "layers", "light")
    f.card("all", 696, 30, 380, 100, "The whole block", "On the GPU", "cpu", "white", strong=True)
    f.card("disk", 696, 144, 380, 96, "Weights that do not fit in RAM", "Read from the memory-mapped GGUF file by the OS", "hard-drive", "dim")
    f.pill("n", 0, 294, W, 50, "N", "chosen by the planner from free VRAM; optimization.n_cpu_moe sets it by hand", "gauge", "light")
    f.note("Run 13 (L4 limited to 8 GB): the cache made generation 1.3-2.2x faster than llama.cpp's --fit placement without it.", "flask-conical")
    f.note("A model larger than RAM gets no cache: there the disk limits generation. CLUAIZ_MOE_CACHE=off turns it off.")
    f.source("Code: engine/llama/src/moe_override.rs, engine/plan/  ·  Details: engine/plan/README.md")
    return f


def two_runtimes() -> Fig:
    f = Fig("engine/two-runtimes.png", "Engine · runtimes", "Two runtimes in one process",
            "llama.cpp runs GGUF files, ONNX Runtime runs ONNX files. They share memory by rules, not by one allocator.",
            height=446)
    f.card("host", 348, 0, 400, 96, "EngineHost", "engine/runtime: loads and unloads every model", "server", "white", strong=True)
    f.group(0, 144, 530, 172, "llama.cpp · GGUF", "white", id="g1")
    f.group(566, 144, 530, 172, "ONNX Runtime · ONNX", "light", id="g2")
    f.card("chat", 20, 174, 236, 124, "Chat", "Text, images, audio input", "message-square", "white")
    f.card("gg", 274, 174, 236, 124, "Speech, vectors", "Qwen3-TTS, bge-m3, EmbeddingGemma", "radio", "light")
    f.card("tts", 586, 174, 236, 124, "Speech", "Kokoro, Piper TTS; Whisper STT", "radio", "light")
    f.card("emb", 840, 174, 236, 124, "Embeddings", "Text and image vectors", "binary", "light")
    f.edge("host", "g1", "b", "t", oa=0.3)
    f.edge("host", "g2", "b", "t", oa=0.7)
    f.card("rule", 0, 350, W, 96, "When memory is short", "Idle ONNX, GGUF speech and GGUF embedding models are unloaded first. A model in use, and the chat model, never are.", "split", "light")
    f.note("GPU backends: CUDA 12, CUDA 13, Vulkan or Metal for llama.cpp; CPU, or CUDA with the ONNX GPU pack, for ONNX Runtime.")
    f.source("Code: engine/runtime/src/host.rs, onnx_host.rs, media_room.rs")
    return f


def ffi_call() -> Fig:
    f = Fig("engine/ffi-call.png", "Engine · C library", "The Engine inside another program",
            "The C library takes the same JSON-RPC messages as POST /v1/cep. Two functions; every reply must be freed.",
            height=300)
    f.card("host", 0, 40, 230, 124, "Your program", "C, or any language that calls C (Python ctypes)", "code", "light")
    f.card("call", 290, 0, 260, 96, "cep_call(request)", "JSON-RPC 2.0 in, the reply out", "send", "white", mono=True)
    f.card("free", 290, 118, 260, 96, "cep_free(reply)", "Frees each reply", "circle-x", "light", mono=True)
    f.card("cep", 610, 40, 220, 124, "CEP", "The same methods as /v1/cep and /mcp", "network", "white", strong=True)
    f.card("eng", 890, 40, 206, 124, "Engine", "Started by the first call, with CLUAIZ_HOME", "server", "light")
    f.edge("host", "call", "r", "l", oa=0.3)
    f.edge("host", "free", "r", "l", oa=0.7)
    f.edge("call", "cep", "r", "l", ob=0.3)
    f.edge("cep", "eng")
    f.pill("panic", 290, 244, 806, 50, "A panic inside the Engine", "becomes a -32603 reply; it never unwinds into your program", "shield-check", "light")
    f.note("Python ctypes test: 12/12 checks, p95 0.11 ms per call, 10,000 calls added no measurable memory (test/cep/ffi_python.py).", "flask-conical")
    f.note("Built from source (ffi/); not in the release packages yet.")
    f.source("Details: ffi/README.md")
    return f


def install_packs() -> Fig:
    f = Fig("get-started/install-packs.png", "Get started · install", "What the installer puts on your machine",
            "A core that runs every model on the CPU, plus one GPU pack chosen from the GPU it finds.", height=400)
    f.card("core", 0, 0, 300, 128, "Core", "The cluaiz program and the CPU builds of llama.cpp (SSE4.2 to AVX-512, NEON)", "package", "white", strong=True)
    f.diamond("gpu", 360, 4, 230, 120, "Which GPU?")
    packs = [("NVIDIA", "CUDA 13 or CUDA 12 pack, by the driver's CUDA version", "cpu", "white"),
             ("AMD Radeon, Intel Arc", "Vulkan pack", "cpu", "light"),
             ("Apple Silicon", "Metal, inside the macOS core", "cpu", "light"),
             ("None found", "CPU only", "power", "dim")]
    for i, (t, x, ic, tone) in enumerate(packs):
        f.card(f"p{i}", 660, i * 102, 436, 92, t, x, ic, tone)
        f.edge("gpu", f"p{i}", "r", "l")
    f.card("check", 0, 270, 300, 128, "Checked and unpacked", "Every file against SHA256SUMS.txt, into ~/.cluaiz/bin, added to PATH", "shield-check", "light")
    f.edge("core", "gpu")
    f.edge("core", "check", "b", "t")
    f.note("CLUAIZ_GPU=cpu|cuda|vulkan chooses the pack yourself. Cluaiz never installs the GPU driver.")
    f.note("A pack works only with the core of the same release (the same llama.cpp commit).")
    f.source("Details: docs/get-started/quickstart.md, docs/deployments/platforms.md")
    return f


def context_plan() -> Fig:
    f = Fig("engine/context-plan.png", "Context · planner", "What goes into the next prompt",
            "Segments with real token counts are taken in this order until the room is full. Same input, same plan.",
            height=560)
    f.pill("room", 0, 0, W, 52, "room = n_ctx − answer − thinking − margin", "available = room − protected", "ruler", "white")
    rows = [("0", "Protected", "system prompt, tool protocol, current message, unfinished tool step: must fit", "white"),
            ("1", "Recent history", "the newest 2 turns (recent_turns_min), whole turns", "light"),
            ("2", "Tools in use", "by relevance, then recency", "light"),
            ("3", "Evidence", "at most 15% of available", "light"),
            ("4", "Suggestions", "at most 5% of available", "light"),
            ("5", "Older history", "by relevance, then recency, whole turns", "mid"),
            ("6", "Evictable", "last; leaving it out does not ask for compaction", "dim")]
    for i, (n, t, x, tone) in enumerate(rows):
        f.card(f"r{i}", 0, 84 + i * 66, 700, 56, f"{n}   {t}", "", None, tone, strong=i == 0)
        f.text(220, 84 + i * 66 + 18, 470, x, 13.2, "#a3a3a3")
    f.card("cmp", 760, 84, 336, 188, "Compaction", "At 85% of available, or when anything not evictable is left out: old tool hints first, then older tool results become placeholders (the newest stay whole)", "scissors" if False else "archive", "light")
    f.card("grp", 760, 296, 336, 108, "Groups", "A tool call and its result are kept or left out together", "layers", "mid")
    f.card("io", 760, 428, 336, 108, "No I/O", "Reads no files, calls no model; the API does the I/O around it", "cpu", "dim")
    f.note("Compaction runs only when it frees enough tokens, because every compaction re-processes the prompt from the first change.")
    f.source("Code: context/src/planner.rs, compaction.rs, policy.rs  ·  Details: context/README.md")
    return f


def chat_pipeline() -> Fig:
    f = Fig("api/chat-pipeline.png", "API · chat handler", "Inside one chat request",
            "Preparation gathers memory, project files, tools and the context plan; the agent turn runs tools behind the approval gate.",
            height=430)
    f.card("req", 0, 0, 220, 110, "Request", "`ExternalChatRequest` (types.rs)", "message-square", "light", num=1)
    f.card("pre", 260, 0, 836, 110, "Preflight", "", "list-checks", "white", num=2, strong=True,
           chips=["model: load or reuse, or a provider", "memory: events, recall, notes", "project: AGENTS.md, rules, skills", "tools: suggest, bind", "context plan"])
    f.card("disp", 0, 170, 220, 110, "Dispatch", "SSE stream, or one JSON reply", "send", "light", num=3)
    f.card("turn", 260, 170, 260, 110, "Agent turn", "api_turn.rs: tool calls, filters", "bot", "white", num=4)
    f.diamond("call", 560, 166, 170, 118, "A tool call?")
    f.card("auth", 770, 170, 326, 110, "Approval gate", "authorize.rs, then the tool runs; its result is added", "shield-check", "light", num=5)
    f.card("out", 260, 330, 520, 96, "Answer", "content, reasoning_content, tool events, context_report", "circle-check", "white", num=6)
    f.card("bg", 820, 330, 276, 96, "When idle", "background.rs enriches the cards", "history", "dim")
    f.edge("req", "pre")
    f.edge("pre", "disp", "b", "t", oa=0.05)
    f.edge("disp", "turn")
    f.edge("turn", "call")
    f.edge("call", "auth", label="yes")
    f.edge("auth", "turn", "t", "t", via=[(933, 148), (390, 148)], label="the model answers again", dashed=True, at=0.5)
    f.edge("call", "out", "b", "t", ob=0.62, label="no")
    f.edge("out", "bg", dashed=True)
    f.source("Code: api/src/handlers/chat/  ·  Details: api/src/handlers/chat/README.md")
    return f


def tool_install() -> Fig:
    f = Fig("engine/tool-install.png", "Agents · installer", "Installing a tool from the registry",
            "Plan first, ask what the user's mode leaves open, then fetch, check and place the files.",
            height=330)
    f.card("ask", 0, 0, 230, 110, "Install request", "type, id; dependencies come too", "package", "light", num=1)
    f.card("plan", 270, 0, 250, 110, "Plan", "What to install and which permissions to ask for", "list", "light", num=2)
    f.diamond("q", 560, 0, 190, 110, "Questions left?")
    f.card("user", 790, 0, 306, 110, "The user answers", "In the terminal or the App", "user", "white")
    f.card("fetch", 790, 196, 306, 120, "Fetch", "registry.json, package.json, files; 30 s connect, 60 s stall", "download", "light", num=3)
    f.card("stage", 400, 196, 350, 120, "Stage and place", "Paths checked (no .., no roots); the old version stays until the new one is in", "shield-check", "white", num=4, strong=True)
    f.card("perm", 0, 196, 360, 120, "Permissions", "Saved to config/tool_permissions.json; venv or npm set up on first use", "key", "light", num=5)
    f.edge("ask", "plan")
    f.edge("plan", "q")
    f.edge("q", "user", label="yes")
    f.edge("q", "fetch", "b", "t", ob=0.2, label="no")
    f.edge("user", "fetch", "b", "t", oa=0.6, ob=0.6)
    f.edge("fetch", "stage", "l", "r")
    f.edge("stage", "perm", "l", "r")
    f.source("Code: agents/src/installer/  ·  Details: agents/src/installer/README.md")
    return f


def speech_pipeline() -> Fig:
    f = Fig("engine/speech-pipeline.png", "ONNX · speech", "Text to speech and speech to text",
            "Both directions share the media decoders. Each TTS family has its own adapter; all of them end in one WAV writer.",
            height=380)
    f.card("txt", 0, 0, 200, 124, "Text", "From the API or the CLI", "message-square", "light", num=1)
    f.card("read", 240, 0, 260, 124, "Reading", "Spoken symbols, bracket content, phrase breaks", "file-text", "light", num=2)
    f.card("fam", 540, 0, 556, 124, "Family adapter", "", "split", "white", num=3, strong=True,
           chips=["Kokoro", "Piper (VITS)", "Supertonic", "Chatterbox", "CosyVoice", "Matcha", "OmniVoice"])
    f.card("wav", 540, 180, 556, 110, "PCM, cleaned, then WAV", "Boundary pauses, sanitize, PCM16 WAV encoder", "radio", "light", num=4)
    f.card("aud", 0, 180, 200, 110, "Audio", "A file or inline bytes", "radio", "light")
    f.card("dec", 240, 180, 260, 110, "Decode, prepare", "Mono, peak, sample rate (shared decoders)", "cpu", "light")
    f.pill("wh", 0, 330, 500, 46, "Whisper", "spectrogram, encoder and decoder graphs: text out", "brain", "white")
    f.edge("txt", "read")
    f.edge("read", "fam")
    f.edge("fam", "wav", "b", "t")
    f.edge("aud", "dec")
    f.edge("dec", "wh", "b", "t", oa=0.5, ob=0.75)
    f.source("Code: engine/onnx/src/audio/, engine/media/  ·  Details: engine/onnx/src/audio/README.md")
    return f


def media_sources() -> Fig:
    f = Fig("engine/media-sources.png", "Media · inputs", "How an attachment reaches a model",
            "One resolver reads every source with size limits; shared decoders turn it into what the model takes.",
            height=420)
    f.card("src", 0, 140, 220, 120, "Source", "A local path, an http(s) URL or base64", "file", "light", num=1)
    f.card("res", 260, 140, 250, 120, "SourceResolver", "Bounded bytes, format detection, an owned temporary file", "shield-check", "white", num=2, strong=True)
    outs = [("Image", "RGB8 for the llama.cpp projector or an ONNX encoder", "file"),
            ("Audio", "PCM F32: mono, peak, rate prepared", "radio"),
            ("Document", "Text in order from TXT, DOC, DOCX, PDF; what was left out is said", "file-text"),
            ("Video", "Timestamped frames and audio (packaged decoder)", "play")]
    for i, (t, x, ic) in enumerate(outs):
        f.card(f"o{i}", 570, i * 106, 526, 96, t, x, ic, "light")
        f.edge("res", f"o{i}", "r", "l")
    f.edge("src", "res")
    f.note("Size limits come from settings.json, media. A PDF can also be sent as page images to a chat model that reads images.")
    f.source("Code: engine/media/src/  ·  Details: engine/media/README.md")
    return f


def terminal_screen() -> Fig:
    f = Fig("engine/terminal-screen.png", "CLI · terminal screen", "The terminal screen only draws",
            "terminal/ draws and edits text. The agent in cmd/ runs the model and the tools and tells the screen what really happened.",
            height=330)
    f.group(0, 0, 520, 330, "terminal/ (Ratatui, Crossterm)", "white", id="g1")
    f.group(576, 0, 520, 330, "cmd/src/agent", "light", id="g2")
    f.card("keys", 20, 30, 230, 122, "Keys, paste, resize", "Crossterm events", "square-terminal", "light")
    f.card("app", 270, 30, 230, 122, "App.handle", "Input box and approvals; returns send, stop, approve or exit", "list-checks", "white", strong=True)
    f.card("draw", 270, 196, 230, 112, "Draw", "One worker owns the screen; a bounded queue feeds it", "monitor", "light")
    f.card("turn", 596, 30, 480, 122, "Agent turn", "Model, tools, approvals, files (the same core as the API chat)", "bot", "white")
    f.card("ent", 596, 196, 480, 112, "Entries", "Model text, thinking, one row per tool call with its real result", "scroll-text", "light")
    f.edge("keys", "app")
    f.edge("app", "turn", oa=0.25, ob=0.25)
    f.edge("turn", "ent", "b", "t")
    f.edge("ent", "draw", "l", "r", label="what happened")
    f.edge("app", "draw", "b", "t")
    f.note("Normal close, an error or a crash puts the terminal back into its normal mode.")
    f.source("Code: terminal/, cmd/src/agent/terminal.rs  ·  Details: docs/cmd/tui.md")
    return f


# ---------------------------------------------------------------------------------------------- api

def route_map() -> Fig:
    f = Fig("api/route-map.png", "API · routes", "The HTTP routes of the Engine",
            "OpenAI and Ollama clients work as they are. Everything Engine-specific goes through CEP.", height=398)
    f.card("client", 0, 128, 230, 128, "Client", "", "monitor", "light", chips=["OpenAI library", "Ollama library", "curl", "the App"])
    f.card("auth", 290, 128, 210, 128, "Auth", "Bearer token when `api_auth.required` is on; `/health` stays open", "lock", "white")
    routes = [("/v1/chat/completions · /api/chat · /api/generate", "Chat (OpenAI and Ollama shapes)", "message-square"),
              ("/v1/models · /api/tags · /api/pull", "Models", "package"),
              ("/v1/embeddings · /api/embed · /v1/audio/*", "Embeddings and speech", "radio"),
              ("/v1/cep · /mcp", "Every Engine-specific method", "network")]
    for i, (path, what, ic) in enumerate(routes):
        f.card(f"r{i}", 570, i * 102, 526, 92, path, what, ic, "white" if i == 3 else "light", strong=i == 3, mono=True)
        f.edge("auth", f"r{i}", "r", "l")
    f.edge("client", "auth")
    f.note("New Engine features are CEP methods, not new HTTP routes. `/v1/ingest` reads documents.")
    f.source("Code: api/src/routes.rs  ·  Details: api/README.md")
    return f


def cep_call() -> Fig:
    f = Fig("api/cep-call.png", "API · CEP", "One CEP call",
            "Every transport reaches the same checks: envelope, method, caller. Non-read calls and denials are audited.",
            height=360)
    for i, (t, ic) in enumerate([("POST /v1/cep", "globe"), ("/mcp", "plug"), ("C library", "code")]):
        f.pill(f"t{i}", 0, 18 + i * 62, 200, 46, t, "", ic, "light")
    f.card("env", 256, 30, 200, 128, "Envelope", "JSON-RPC 2.0; batch and depth limits", "file-code", "light", num=1)
    f.card("reg", 496, 30, 200, 128, "Registry", "Method by name; params checked by its schema", "list", "light", num=2)
    f.card("pri", 736, 30, 360, 128, "Principal", "Who calls: scope, roots, owner", "key", "white", num=3, strong=True,
           chips=["read", "write", "exec", "admin"])
    f.card("meth", 736, 240, 360, 112, "Method", "The domain call", "cog", "white", num=4)
    f.card("appr", 380, 240, 316, 112, "Approval or task", "Approval bound to this call; long work runs as a task", "stamp", "light")
    f.card("audit", 0, 240, 340, 112, "Audit log", "One line per non-read call or denial; params as a hash", "scroll-text", "mid")
    for i in range(3):
        f.edge(f"t{i}", "env", "r", "l")
    f.edge("env", "reg")
    f.edge("reg", "pri")
    f.edge("pri", "meth", "b", "t")
    f.edge("meth", "appr", "l", "r")
    f.edge("appr", "audit", "l", "r")
    f.note("A panic inside a method becomes error -32603; the connection stays open.")
    f.source("Details: cep/README.md")
    return f


def request_queue() -> Fig:
    f = Fig("api/request-queue.png", "API · queues", "One model serves one request; the others wait in line",
            "Embeddings, speech and chat attachments run one at a time, in arrival order. Example with requests of about 8 s.",
            height=268)
    left, right = 150, W
    scale = (right - left) / 30
    rows = [("Request A", 0, 0, 8), ("Request B", 2, 8, 16), ("Request C", 3, 16, 24)]
    out = []
    for i, (name, arrive, start, end) in enumerate(rows):
        y = i * 72
        out.append(f'<div class="text" style="left:0;top:{y + 12}px;width:140px;font-size:15px;color:#efefef;font-weight:600">{name}</div>')
        if start > arrive:
            out.append(f'<div style="position:absolute;left:{left + arrive * scale}px;top:{y}px;width:{(start - arrive) * scale - 4}px;height:46px;'
                       f'border:1.5px dashed #6b6b6b;border-radius:10px;background:{HATCH};display:flex;align-items:center;padding-left:12px;'
                       f'font-size:13.5px;color:#cacaca">waits</div>')
        out.append(f'<div style="position:absolute;left:{left + start * scale}px;top:{y}px;width:{(end - start) * scale}px;height:46px;'
                   f'border-radius:10px;background:linear-gradient(180deg,#f2f2f2,#cfcfcf);display:flex;align-items:center;padding-left:12px;'
                   f'font-size:13.5px;font-weight:700;color:#0a0a0a">runs</div>')
    for s in range(0, 31, 5):
        x = left + s * scale
        out.append(f'<div style="position:absolute;left:{x}px;top:226px;width:1px;height:10px;background:#6b6b6b"></div>'
                   f'<div class="text" style="left:{x - 20}px;top:242px;width:40px;text-align:center;font-size:12.5px">{s} s</div>')
    out.append(f'<div style="position:absolute;left:{left}px;top:230px;width:{right - left}px;height:1px;background:#4a4a4a"></div>')
    f.raw("".join(out))
    f.note("A request still waiting after 120 s gets HTTP 429 (`embedding_busy`, `speech_busy`, `media_busy`).", "timer")
    f.source("Details: engine/runtime/README.md")
    return f


def thinking_budget() -> Fig:
    f = Fig("api/thinking-budget.png", "API · reasoning_effort", "How much a model may think",
            "Each level is a share of the room, with a cap. Room = context left after the prompt, minus max_tokens kept for the answer.",
            height=330)
    rows = [("low", 0.25, "25% of the room", "at most 2,048 tokens"), ("medium", 0.50, "50% of the room", "at most 8,192 tokens"),
            ("high", 0.75, "75% of the room", "at most 32,768 tokens"), ("max", 1.00, "the whole room", "no cap")]
    out = []
    bx, bw = 120, 600
    for i, (name, share, a, b) in enumerate(rows):
        y = i * 66
        out.append(f'<div class="text mono" style="left:0;top:{y + 12}px;width:110px;font-size:15px;color:#efefef">{name}</div>')
        out.append(f'<div style="position:absolute;left:{bx}px;top:{y}px;width:{bw}px;height:44px;border-radius:10px;'
                   f'background:#151515;border:1px solid rgba(255,255,255,.09)"></div>')
        fill = "linear-gradient(90deg,#8a8a8a,#f2f2f2)" if name != "max" else "#ffffff"
        out.append(f'<div style="position:absolute;left:{bx}px;top:{y}px;width:{bw * share}px;height:44px;border-radius:10px;background:{fill}"></div>')
        out.append(f'<div class="text" style="left:{bx + bw + 28}px;top:{y + 4}px;width:300px;font-size:15px;color:#efefef;font-weight:600">{a}'
                   f'<div style="font-weight:400;color:#a3a3a3;font-size:13.5px">{b}</div></div>')
    out.append(f'<div class="pill" style="--t:#ffffff;left:0;top:276px;width:{W}px;height:50px">{icon("timer")}'
               f'<span>auto</span><span class="tx">3 minutes at the measured speed of the model, from 1,024 tokens up to the high level; never max</span></div>')
    f.raw("".join(out))
    f.note("A number (for example 256) is that many tokens; 0 answers without thinking.")
    f.source("Code: engine/core/src/settings.rs  ·  Details: docs/api/inference/chat_completions.md")
    return f


def thinking_close() -> Fig:
    f = Fig("api/thinking-close.png", "API · reasoning_effort", "When the thinking budget runs out",
            "The model gets room to finish its line, then a short wrap-up sentence, and then it answers.",
            height=250)
    segs = [(0, 560, "Thinking", "auto, low, medium or high", "linear-gradient(90deg,#3a3a3a,#8f8f8f)", "#f2f2f2"),
            (560, 760, "Finish the line", "up to 1/4 of the budget, at most 500", HATCH + ",#1a1a1a", "#f2f2f2"),
            (760, 900, "Wrap-up", "one sentence", "#f2f2f2", "#0a0a0a"),
            (900, W, "Answer", "after the end tag", "#0f0f0f", "#f2f2f2")]
    out = []
    for x0, x1, a, b, bg, fg in segs:
        border = "border:1.5px solid #f2f2f2;" if a == "Answer" else ""
        out.append(f'<div style="position:absolute;left:{x0}px;top:40px;width:{x1 - x0 - 4}px;height:76px;border-radius:12px;background:{bg};{border}'
                   f'padding:14px 14px;color:{fg}"><div style="font-size:15px;font-weight:700">{a}</div>'
                   f'<div style="font-size:12.8px;opacity:.85;margin-top:4px">{b}</div></div>')
    out.append('<div style="position:absolute;left:557px;top:0;width:2px;height:132px;background:#ffffff"></div>'
               '<div class="elabel" style="left:558px;top:12px;transform:translate(-50%,0)">budget spent</div>')
    out.append('<div style="position:absolute;left:0;top:156px;width:1096px;padding:18px 22px;border-radius:14px;background:#121212;'
               'border:1px solid rgba(255,255,255,.1);font-size:15px;color:#e6e6e6;line-height:1.5">'
               '<div style="font-size:12px;letter-spacing:.12em;text-transform:uppercase;color:#a3a3a3;font-weight:700;margin-bottom:6px">'
               'The wrap-up sentence (from Qwen\'s guidance)</div>'
               '&ldquo;Considering the limited time by the user, I have to give the solution based on the thinking directly now.&rdquo;</div>')
    f.raw("".join(out))
    f.note("A number of tokens and max close exactly at the budget, with the same sentence.")
    f.note("Run 31: low (2,048 tokens) closed at 2,069 and at 2,124 tokens, both at a line end.", "flask-conical")
    f.source("Code: engine/llama/src/native/stream.rs, engine/llama/src/ffi/reasoning.rs")
    return f


def openai_fields() -> Fig:
    f = Fig("api/openai-fields.png", "API · OpenAI compatibility", "OpenAI chat fields on an installed model",
            "stop and max_completion_tokens work since run 34; n was recorded in run 32 (Engine 0.1.0, Qwen3-0.6B). logit_bias reaches provider models only.", height=400)
    ok = ["messages, stream", "max_tokens, max_completion_tokens", "stop (a string or a list)", "temperature, top_p, seed",
          "frequency and presence penalty", "response_format (JSON, JSON schema)", "tools, tool_choice", "reasoning_effort"]
    no = [("n", "always one choice"), ("logprobs, top_logprobs", "ignored"), ("logit_bias", "ignored")]

    def column(x, head, ic, items, strong):
        border = "border:1.5px solid #f2f2f2;" if strong else "border:1px solid rgba(255,255,255,.1);"
        rows = "".join(
            f'<div style="display:flex;gap:10px;align-items:flex-start;padding:9px 0;border-top:1px solid rgba(255,255,255,.06)">'
            f'<span style="width:18px;height:18px;flex:none;color:{"#f2f2f2" if strong else "#8a8a8a"}">{icon("check" if strong else "x")}</span>'
            f'<span class="mono" style="font-size:13.4px;color:#efefef">{esc(a)}</span>'
            + (f'<span style="font-size:13px;color:#a3a3a3;margin-left:auto;text-align:right">{esc(b)}</span>' if b else "") + '</div>'
            for a, b in items)
        return (f'<div style="position:absolute;left:{x}px;top:0;width:530px;min-height:400px;border-radius:14px;padding:18px 20px;'
                f'background:#131313;{border}"><div style="display:flex;gap:10px;align-items:center;font-size:17px;font-weight:700;'
                f'margin-bottom:10px"><span class="ic" style="--t:#ffffff">{icon(ic)}</span>{head}</div>{rows}</div>')

    f.raw(column(0, "Honoured", "circle-check", [(a, "") for a in ok], True)
          + column(566, "Ignored today", "circle-x", no, False))
    f.note("Unknown fields are ignored; no error is returned.")
    f.source("Details: docs/api/inference/openai_compatibility.md")
    return f


# ---------------------------------------------------------------------------------------------- contributing

def test_kinds() -> Fig:
    f = Fig("contributing/test-kinds.png", "Contributing · tests", "Two kinds of tests in the repository",
            "Unit and contract tests need no server. The test/ folder drives a started server and real models.", height=330)
    f.card("cargo", 0, 0, 520, 140, "cargo test", "Unit tests in `src/`, contract tests in `<crate>/tests/`", "flask-conical", "light",
           chips=["no server needed"])
    f.card("dir", 576, 0, 520, 140, "test/ folder", "Journeys against a started server, model regressions, Mac, CPU and cloud scripts",
           "folder", "light", chips=["isolated home", "port 8011"])
    f.card("dev", 0, 220, 520, 110, "Developer machine", "Runs both before a commit", "monitor", "white")
    f.card("runs", 576, 220, 520, 110, "Recorded runs", "`test/cloud/runs/<date>-runN/` and a row in BENCHMARKS.md", "history", "white", strong=True)
    f.edge("cargo", "dev", "b", "t")
    f.edge("dir", "dev", "b", "t", oa=0.25, ob=0.8)
    f.edge("dir", "runs", "b", "t", oa=0.6, ob=0.6)
    f.note("GitHub runs cargo test for the crates without llama.cpp (rust-tests.yml); llama, onnx, runtime, api and cmd run on developer machines.", "info", "white")
    f.source("Details: test/README.md")
    return f


def cloud_run() -> Fig:
    f = Fig("contributing/cloud-run.png", "Contributing · cloud runs", "How a cloud run is done and recorded",
            "A Spot VM is created for one run, the results come back with their raw output, and the VM is deleted.", height=316)
    top = [("create", "vm.sh create", "Spot VM; stops itself after 3 h", "cloud"), ("put", "vm.sh put", "The remote scripts", "send"),
           ("build", "Build", "`remote/linux_build.sh`", "wrench"), ("test", "Test", "`linux_tests.sh` or a run script", "flask-conical")]
    bottom = [("get", "vm.sh get", "Results back to the laptop", "download"), ("folder", "Run folder", "SUMMARY.md, raw output, the script", "folder"),
              ("bench", "BENCHMARKS.md", "One row per run", "list"), ("del", "vm.sh delete", "VM and disk removed", "circle-x")]
    for i, (id, t, x, ic) in enumerate(top):
        f.card(id, i * 282, 0, 250, 112, t, x, ic, "light", num=i + 1)
    for i, (id, t, x, ic) in enumerate(bottom):
        f.card(id, (3 - i) * 282, 196, 250, 112, t, x, ic, "white" if id == "folder" else "light", num=i + 5, strong=id == "folder")
    for a, b in [("create", "put"), ("put", "build"), ("build", "test")]:
        f.edge(a, b)
    f.edge("test", "get", "b", "t")
    for a, b in [("get", "folder"), ("folder", "bench"), ("bench", "del")]:
        f.edge(a, b, "l", "r")
    f.note("Speeds are the timings llama.cpp reports. User names, keys and addresses are removed before a commit.")
    f.source("Details: test/cloud/README.md")
    return f


FIGURES = [terminal_screen, context_plan, chat_pipeline, tool_install, speech_pipeline, media_sources, reader_paths, model_download, from_ollama, install_packs, kv_reuse, moe_placement, two_runtimes, ffi_call, crate_layers, chat_request, hardware_profile, permission_copies, two_walls,
           agent_turn, plugin_runners, mcp_server_life, auto_context, memory_store, provider_call, automation_layers,
           route_map, cep_call, request_queue, thinking_budget, thinking_close, openai_fields, test_kinds, cloud_run]
