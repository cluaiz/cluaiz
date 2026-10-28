# Mac test (Apple Silicon)

Runs Cluaiz on a Mac with Apple Silicon and compares it with plain llama.cpp on the same model files and the
same settings. It takes about an hour and needs one command; all results are packed into one archive.

Everything is kept in `~/cluaiz-mactest`, which can be removed afterwards (see [Clean up](#clean-up)).

## Requirements

| | |
|---|---|
| Mac | Apple Silicon (M1 or newer). Intel Macs are not supported. |
| Power | Connected to the charger: on battery the Mac slows down and the numbers are not comparable. |
| Network | 10 to 40 GB of downloads, depending on the RAM (see [What it does](#what-it-does)). |
| Disk | At least 40 GB free. |
| During the test | Keep the Mac awake with the lid open and avoid other heavy work. |

## Run

1. Open Terminal.
2. Run:

   ```bash
   bash -c "$(curl -fsSL https://raw.githubusercontent.com/cluaiz/cluaiz/main/test/mac/mac_test.sh)"
   ```

3. If macOS asks to install the Command Line Tools, install them and run the command again.
4. Wait until the last lines read:

   ```
   == [55 min] done in 55 min
   == results: /Users/<you>/Desktop/cluaiz-mac-results-<date>.tar.gz (send this file)
   ```

The archive on the Desktop is the result of the test.

## What it does

1. Records the machine: chip, RAM, macOS version, SSD write speed, power and thermal state.
2. Installs the latest release with the official installer (`install.sh`).
3. Downloads Q4_K_M models that fit the RAM:

   | RAM | Models | Download |
   |---|---|---|
   | 16 GB | Qwen3 4B, Gemma 3 12B | ~9 GB |
   | 24 GB | the above + Mistral Small 3.2 24B | ~22 GB |
   | 32 GB or more | the above + Gemma 4 31B | ~40 GB |

4. Builds `llama-bench` with Metal from the llama.cpp commit the release uses, while the models download.
5. Runs [`test/engine_regression/run.py`](../engine_regression/run.py) for every model in four placement modes,
   on Cluaiz and on llama.cpp with the same placement:

   | Mode | Placement |
   |---|---|
   | `auto` | chosen by the Cluaiz planner |
   | `gpu` | every layer on the GPU |
   | `split` | half of the layers on the GPU |
   | `cpu` | CPU only |

6. Packs every log and report into the archive on the Desktop.

## Options

| Variable | Effect |
|---|---|
| `BIG=1` | Also runs Gemma 4 31B (17 GB) on a Mac with less than 32 GB RAM, read from the SSD while it runs (`auto` mode only, about 20 to 30 minutes more). |
| `RUNS=<n>` | Requests per model and mode (default 3; the first one is the cold run). |

```bash
BIG=1 bash -c "$(curl -fsSL https://raw.githubusercontent.com/cluaiz/cluaiz/main/test/mac/mac_test.sh)"
```

## If something goes wrong

- Stopping with `Ctrl + C` still writes the archive with the results finished so far.
- Running the command again reuses the models already downloaded.
- If there is an error, or no new line appears for 15 to 20 minutes, keep a screenshot of the Terminal
  together with the archive.

## Results archive

| File | Content |
|---|---|
| `machine.txt` | Hardware, macOS, SSD write speed, power and thermal state |
| `install.log`, `installed_files.txt` | Installer output and the installed files |
| `pulls.log`, `pull_*.log` | Model downloads |
| `llama_bench_build.log` | llama.cpp build |
| `server.log`, `planner.log` | Engine log; the planner's placement decisions |
| `RESULTS_*.md`, `run_*.log` | Cluaiz vs llama.cpp per model and mode, with every setting and the exact llama-bench command |
| `loaded_libs.txt` | Libraries the engine loaded |
| `mac_test.log` | Everything the script printed |

The server runs with its own home (`CLUAIZ_HOME=~/cluaiz-mactest/home`) on port 8011, so another Cluaiz
installation on the same Mac is not touched.

## Clean up

```bash
rm -rf ~/cluaiz-mactest
```

The installer also adds a `# Cluaiz` PATH line to `~/.zshrc`; remove that line and the `export PATH=...` line
below it if the Mac is not used for Cluaiz afterwards.
