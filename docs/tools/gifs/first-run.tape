# First run: list the installed models, then ask one question. Black, white and greys only (docs rule).
# Remake: set CLUAIZ_HOME to a test home with a small chat model, then
#   vhs docs/tools/gifs/first-run.tape
# The model id below must be installed in that home.

Output docs/assets/get-started/first-run.gif

Set Shell "powershell"
Set FontSize 20
Set Width 1400
Set Height 340
Set Padding 24
Set TypingSpeed 45ms
Set Theme { "name": "cluaiz-grey", "black": "#000000", "red": "#d0d0d0", "green": "#e8e8e8", "yellow": "#d8d8d8", "blue": "#c8c8c8", "magenta": "#d0d0d0", "cyan": "#e0e0e0", "white": "#f0f0f0", "brightBlack": "#808080", "brightRed": "#e0e0e0", "brightGreen": "#f0f0f0", "brightYellow": "#e8e8e8", "brightBlue": "#d8d8d8", "brightMagenta": "#e0e0e0", "brightCyan": "#f0f0f0", "brightWhite": "#ffffff", "background": "#0a0a0a", "foreground": "#e6e6e6", "selection": "#404040", "cursor": "#ffffff" }

Hide
Type "function prompt { '> ' }; Clear-Host"
Enter
Sleep 1s
Show

Type "cluaiz list"
Sleep 400ms
Enter
Sleep 3s

Type 'cluaiz run qwen3.5-0.8b-gguf-Q4_K_M --think off --temp 0 "What is the capital of Japan? One short sentence."'
Sleep 500ms
Enter
Sleep 12s
