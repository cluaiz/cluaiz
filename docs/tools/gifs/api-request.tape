# One API request to a running server (port 8011 of a test home), answer printed from the JSON.
# Remake from the repository root with CLUAIZ_HOME set to a test home whose permission.json uses port 8011
# and that has the model below: vhs docs/tools/gifs/api-request.tape

Output docs/assets/get-started/api-request.gif

Set Shell "powershell"
Set FontSize 17
Set Width 1600
Set Height 250
Set Padding 24
Set TypingSpeed 35ms
Set Theme { "name": "cluaiz-grey", "black": "#000000", "red": "#d0d0d0", "green": "#e8e8e8", "yellow": "#d8d8d8", "blue": "#c8c8c8", "magenta": "#d0d0d0", "cyan": "#e0e0e0", "white": "#f0f0f0", "brightBlack": "#808080", "brightRed": "#e0e0e0", "brightGreen": "#f0f0f0", "brightYellow": "#e8e8e8", "brightBlue": "#d8d8d8", "brightMagenta": "#e0e0e0", "brightCyan": "#f0f0f0", "brightWhite": "#ffffff", "background": "#0a0a0a", "foreground": "#e6e6e6", "selection": "#404040", "cursor": "#ffffff" }

Hide
Type `function prompt { '> ' }; $s = Start-Process cluaiz -ArgumentList 'serve' -WindowStyle Hidden -PassThru; Start-Sleep 6; Clear-Host`
Enter
Sleep 8s
Show

Type "curl.exe -s http://127.0.0.1:8011/health"
Enter
Sleep 2s
Type `$body = '{"model": "qwen3.5-0.8b-gguf-Q4_K_M", "think_mode": "off", "messages": [{"role": "user", "content": "Name the capital of France in one word."}]}'`
Enter
Sleep 500ms
Type `(Invoke-RestMethod http://127.0.0.1:8011/v1/chat/completions -Method Post -ContentType application/json -Body $body).choices[0].message.content`
Enter
Sleep 10s

Hide
Type `Stop-Process -Id $s.Id; Clear-Host`
Enter
Sleep 1s
