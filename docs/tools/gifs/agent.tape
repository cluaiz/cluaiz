# cluaiz agent writes a file and runs it, in an empty folder. Shown at 2x speed. Black, white and greys only.
# Remake from the repository root with CLUAIZ_HOME set to a test home that has the model below, and
# AGENT_DIR set to an empty scratch folder: vhs docs/tools/gifs/agent.tape

Output docs/assets/engine/agent.gif

Set Shell "powershell"
Set FontSize 18
Set Width 1400
Set Height 520
Set Padding 24
Set TypingSpeed 35ms
Set PlaybackSpeed 2.0
Set Theme { "name": "cluaiz-grey", "black": "#000000", "red": "#d0d0d0", "green": "#e8e8e8", "yellow": "#d8d8d8", "blue": "#c8c8c8", "magenta": "#d0d0d0", "cyan": "#e0e0e0", "white": "#f0f0f0", "brightBlack": "#808080", "brightRed": "#e0e0e0", "brightGreen": "#f0f0f0", "brightYellow": "#e8e8e8", "brightBlue": "#d8d8d8", "brightMagenta": "#e0e0e0", "brightCyan": "#f0f0f0", "brightWhite": "#ffffff", "background": "#0a0a0a", "foreground": "#e6e6e6", "selection": "#404040", "cursor": "#ffffff" }

Hide
Type `function prompt { '> ' }; Set-Location $env:AGENT_DIR; Remove-Item hello.py -ErrorAction SilentlyContinue; Clear-Host`
Enter
Sleep 1s
Show

Type `cluaiz agent --ui plain -m qwen3.5-4b-gguf-Q4_K_M -y "Create a file hello.py that prints Hello from Cluaiz, then run it."`
Sleep 500ms
Enter
Sleep 75s
