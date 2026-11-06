# CPU test (Windows PC without a graphics card)

Installs the newest release with the website's command and tests it on the CPU: chat, speed and memory on small
models, the coding agent, plugins and skills, speech, embeddings and a document. It needs one command, takes
about an hour on a laptop, and packs all results into one archive on the Desktop.

Everything is kept in `~\cluaiz-cputest`, which can be removed afterwards. The test uses its own home and port
8011, so another Cluaiz installation on the same PC is not touched. The installer adds a PATH entry; the test puts
the user PATH back as it was.

## Requirements

| | |
|---|---|
| PC | Windows 10 or 11, x64 |
| Memory | 8 GB or more; the default models were chosen for 16 GB |
| Disk | At least 12 GB free |
| Network | About 5 GB of downloads |
| Power | Connected to the charger: on battery the numbers are lower |

## Run

Open PowerShell and run:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -Command "irm https://raw.githubusercontent.com/cluaiz/cluaiz/main/test/cpu/cpu_test.ps1 | iex"
```

Every step prints its number and the time so far. Keep the PC awake and avoid other heavy work. Wait for:

```
== done in 58:12
== results: C:\Users\<you>\Desktop\cluaiz-cpu-results-<date>.zip (send this file)
```

## What it does

1. Records the machine: processor, memory, disk, graphics and power.
2. Installs the release (`irm https://cluaiz.com/install.ps1 | iex`) and records which packs it chose.
3. Downloads the models: `prism-ml/Bonsai-4B-gguf:Q1_0` (0.5 GB), `prism-ml/Bonsai-8B-gguf:Q1_0` (1.1 GB) and
   `unsloth/Qwen3-4B-GGUF:Q4_K_M` (2.5 GB).
4. Starts the server.
5. For each model: first answer, placement, generation and prompt speed (three runs), server memory, a second
   turn in the same chat and how much of the KV cache it reused, the default thinking mode, streaming.
6. Installs the `math`, `time`, `sysinfo` and `text` plugins and a skill, and calls each plugin.
7. Text to speech (Kokoro), speech to text of that audio (Moonshine), embeddings (MiniLM), a text document.
8. The coding agent, on Qwen and each Bonsai model: reads a file, `--continue`, runs a command, a slow command that must not crash it, a change
   nobody can approve, a plugin.
9. A chat of 12 turns on the smallest model.
10. Writes the results.

## Options

| Variable | Effect |
|---|---|
| `CLUAIZ_VERSION=<x.y.z>` | Release to install (default: the newest, pre-releases included) |
| `CPU_TEST_MODELS="<id> <id>"` | Models to test instead of the default ones |
| `CPU_TEST_DIR=<folder>` | Where everything is kept |
| `CPU_TEST_EXE=<cluaiz.exe>` | Use this program and skip the install |

## States

| State | Meaning |
|---|---|
| `PASS` | It worked |
| `WARN` | The Engine answered, but the model's answer was not the expected one, or the test ran on battery |
| `FAIL` | The Engine or a tool did not work |
| `SKIP` | The step was not run |

## Results archive

| File | Content |
|---|---|
| `SUMMARY.md` | Machine, speed table and every test with its state |
| `results.json`, `speeds.json` | The same data for a program |
| `machine.txt` | Processor, memory, disk, graphics, power |
| `install.log`, `installed_files.txt` | Installer output and the installed files |
| `status.out.txt` | The hardware the Engine sees |
| `pull_*.txt`, `show_*.txt` | Downloads and each model's plan |
| `chat_*.json` | The first full answer of each model, with the Engine report |
| `agent_*.txt` | Every event of each agent run |
| `server-*.txt` | Engine logs |
| `speech.wav` | The speech the Engine made |
| `cpu_test.log` | Everything the script printed |

## Clean up

```powershell
Remove-Item -Recurse -Force ~\cluaiz-cputest
```
