# What Cluaiz can do, in one terminal: the models it holds, the plan for this machine, an image question,
# speech made and read back, and a document turned into text. Black, white and greys only (docs rule).
#
# Remake from the repository root, with `cluaiz` on PATH and CLUAIZ_HOME set to a test home whose
# permission.json uses port 8011 and that has these models installed:
#   qwen3.5-0.8b-gguf-Q4_K_M (with its mmproj), qwen3.5-4b-gguf-Q4_K_M, embeddinggemma-300m-gguf-Q8_0,
#   kokoro-82m-v1.0-onnx-UINT8, moonshine-tiny-onnx-INT8
# then: vhs docs/tools/gifs/tour.tape
#
# The input files are docs/tools/gifs/tour/receipt.png and handbook.docx. The speech sentence and speed 0.9
# were chosen because Moonshine Tiny (a 0.05 GB model) read them back word for word in 3 of 3 tries; at the
# default speed it got the same sentence wrong (2026-11-25). The three server models are loaded once, hidden, after
# the `cluaiz show` scene (so that scene plans on a machine with nothing
# loaded), so the GIF shows answers rather than first-load time.

Output docs/assets/engine/tour.gif

Set Shell "powershell"
Set FontSize 16
Set Width 1500
Set Height 620
Set Padding 24
Set TypingSpeed 18ms
Set Theme { "name": "cluaiz-grey", "black": "#000000", "red": "#d0d0d0", "green": "#e8e8e8", "yellow": "#d8d8d8", "blue": "#c8c8c8", "magenta": "#d0d0d0", "cyan": "#e0e0e0", "white": "#f0f0f0", "brightBlack": "#808080", "brightRed": "#e0e0e0", "brightGreen": "#f0f0f0", "brightYellow": "#e8e8e8", "brightBlue": "#d8d8d8", "brightMagenta": "#e0e0e0", "brightCyan": "#f0f0f0", "brightWhite": "#ffffff", "background": "#0a0a0a", "foreground": "#e6e6e6", "selection": "#404040", "cursor": "#ffffff" }

Hide
Type `function prompt { '> ' }; $d = New-Item -ItemType Directory -Force "$env:TEMP\cluaiz-tour"; Copy-Item docs\tools\gifs\tour\* $d; Set-Location $d; Clear-Host`
Enter
Sleep 2s
Show

Type "# 1. One engine for chat, vision, embeddings, speech and transcription"
Enter
Type "cluaiz list"
Enter
Wait@30s
Sleep 2500ms

Type "# 2. It measures this machine and plans where the model runs"
Enter
Type "cluaiz show qwen3.5-4b-gguf-Q4_K_M | Select-Object -Last 15"
Enter
Wait@30s
Sleep 5s

Hide
Type `$s = Start-Process cluaiz -ArgumentList 'serve' -WindowStyle Hidden -PassThru; Start-Sleep 8`
Enter
Sleep 10s
Type `$null = irm 127.0.0.1:8011/v1/chat/completions -Method Post -ContentType application/json -Body '{"model": "qwen3.5-0.8b-gguf-Q4_K_M", "think_mode": "off", "max_tokens": 4, "messages": [{"role": "user", "content": "Hi"}]}'; $w = (irm 127.0.0.1:8011/v1/audio/speech -Method Post -ContentType application/json -Body '{"model": "kokoro-82m-v1.0-onnx-UINT8", "input": "Hi"}').output.audio_data; $null = irm 127.0.0.1:8011/v1/audio/transcriptions -Method Post -ContentType application/json -Body (@{ model = 'moonshine-tiny-onnx-INT8'; input_source = @{ type = 'audio'; data = $w } } | ConvertTo-Json); Clear-Host`
Enter
Wait@180s
Sleep 500ms
Show

Type "# 3. Ask about an image"
Enter
Type `$ask = @{ model = 'qwen3.5-0.8b-gguf-Q4_K_M'; think_mode = 'off'; temperature = 0; messages = @(@{ role = 'user'; content = @(@{ type = 'image_url'; image_url = @{ url = "$PWD\receipt.png" } }, @{ type = 'text'; text = 'What was bought, and what is the total? One sentence.' }) }) }`
Enter
Type `(irm 127.0.0.1:8011/v1/chat/completions -Method Post -ContentType application/json -Body ($ask | ConvertTo-Json -Depth 8)).choices[0].message.content`
Enter
Wait@120s
Sleep 3s

Type "# 4. Make speech, then turn it back into text"
Enter
Type `$wav = (irm 127.0.0.1:8011/v1/audio/speech -Method Post -ContentType application/json -Body '{"model": "kokoro-82m-v1.0-onnx-UINT8", "voice": "af_bella", "speed": 0.9, "input": "This voice was made on my own computer."}').output.audio_data`
Enter
Wait@120s
Type `(irm 127.0.0.1:8011/v1/audio/transcriptions -Method Post -ContentType application/json -Body (@{ model = 'moonshine-tiny-onnx-INT8'; input_source = @{ type = 'audio'; data = $wav } } | ConvertTo-Json)).output.text`
Enter
Wait@120s
Sleep 4s

Hide
Type "Clear-Host"
Enter
Sleep 500ms
Show

Type "# 5. Turn a Word document into text chunks, no model needed"
Enter
Type "cluaiz ingest handbook.docx"
Enter
Wait@30s
Sleep 5s

Hide
Type `Stop-Process -Id $s.Id; Set-Location $env:TEMP; Remove-Item -Recurse -Force $d; Clear-Host`
Enter
Sleep 1s
