# cluaiz status: what the planner sees on this machine. Black, white and greys only.
# Remake from the repository root with CLUAIZ_HOME set to a test home: vhs docs/tools/gifs/status.tape

Output docs/assets/get-started/status.gif

Set Shell "powershell"
Set FontSize 20
Set Width 1200
Set Height 360
Set Padding 24
Set TypingSpeed 45ms
Set Theme { "name": "cluaiz-grey", "black": "#000000", "red": "#d0d0d0", "green": "#e8e8e8", "yellow": "#d8d8d8", "blue": "#c8c8c8", "magenta": "#d0d0d0", "cyan": "#e0e0e0", "white": "#f0f0f0", "brightBlack": "#808080", "brightRed": "#e0e0e0", "brightGreen": "#f0f0f0", "brightYellow": "#e8e8e8", "brightBlue": "#d8d8d8", "brightMagenta": "#e0e0e0", "brightCyan": "#f0f0f0", "brightWhite": "#ffffff", "background": "#0a0a0a", "foreground": "#e6e6e6", "selection": "#404040", "cursor": "#ffffff" }

Hide
Type "function prompt { '> ' }; Clear-Host"
Enter
Sleep 1s
Show

Type "cluaiz status"
Sleep 400ms
Enter
Sleep 4s
