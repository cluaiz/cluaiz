<!-- name: test/cloud/remote/reproduce
description: A short, one-command version of runs 19 and 40 (small LLM next to a website on a small VPS); written, not run yet. -->

# reproduce: a short version of run 19 / run 40 on a clean machine

**Status: written and syntax-checked only. It has NOT been run end to end yet.** No number from this folder is
public until the checks in the Pending table are done on a real VM.

`vps_small_llm.sh` runs one command on a fresh Ubuntu 24.04 machine (1-2 vCPU, 4 GB RAM, no GPU) and prints
one table: a small Qwen3 model answering requests while a website with PostgreSQL runs on the same machine.
It is a short form of [run 19](../../../benchmarks/release-and-platforms.md#run-19) and
[run 40](../../../benchmarks/vs-ollama-and-load.md#run-40), which needed the harness in `../vps26/`.

## What it measures

| Item | How |
|---|---|
| Machine memory peak | `MemTotal - MemAvailable`, once a second (`vps26/sampler.py`), the highest value in the engine's phases |
| Engine memory peak | anonymous memory the engine processes hold (`RssAnon`, the same as the first VPS load run (2026-11-20)) and the mapped model file (`RssFile`) |
| Tokens/s | generated tokens / wall seconds of the step, all requests together (`workloads.py`, `agg_tps`) |
| Requests OK | requests that returned a non-empty answer, out of all sent |
| Website median / p95 / max | one `GET /health` a second against `vps26/neighbour_app.py` (one indexed read and one write in PostgreSQL), over the engine's phases |
| Website failed checks | checks that did not return HTTP 200 (timeout counts as failed) |
| CPU steal | share of CPU time the host took back during the engine's block, from `/proc/stat` |

Settings, the same as the first VPS load run (2026-11-20): 1 thread, context 4096, Q8_0 KV cache, one slot, temperature 0, thinking off,
engine in a systemd scope with a memory limit of RAM minus 400 MB. Load: the W1 workload of the first VPS load run (2026-11-20) (a support
ticket classified as JSON, up to 32 tokens), 1 request at a time for 10 minutes, then 8 in flight for 5 minutes.

## Expected run time

About 25 to 30 minutes for Cluaiz alone: apt and downloads 3 to 6 minutes (the model is 0.4 GB for 0.6B,
1.1 GB for 1.7B), 30 s of website alone, 15 s warm-up, 10 + 5 minutes of load. With `--compare` add about 35
minutes and about 4 GB more disk (the Ollama archive is 1.5 GB). These times are estimates from the
phase lengths, not measured.

## How to run

```bash
git clone --depth 1 https://github.com/cluaiz/cluaiz.git
cd cluaiz/test/cloud/remote/reproduce
sudo bash vps_small_llm.sh                 # Cluaiz only, Qwen3 0.6B
sudo bash vps_small_llm.sh --model 1.7b    # the bigger model
sudo bash vps_small_llm.sh --compare       # also llama.cpp and Ollama, one after the other
```

Needs root, Ubuntu 24.04 x86_64, systemd (not a bare container), about 4 GB free disk (8 GB with
`--compare`), and free ports 3100, 8011, 8012, 8013 and 11435. The script stops with a message if one is
missing. Run it on a machine you can throw away: it installs apt packages and a PostgreSQL server.
Other options: `--seq-secs`, `--queue-secs`, `--queue-level`, `--threads`, `--out-dir`, `--keep-work`
(`--help` lists them). Results go to `./vps_small_llm_results_<date>/` (`table.txt` is the table).

### Pins (all checked with sha256 before use)

| What | Version | Source |
|---|---|---|
| Cluaiz | release `v0.1.0`, `cluaiz-0.1.0-linux-x64.tar.gz` | GitHub release; sha256 in the script equals the release's `SHA256SUMS.txt` line |
| llama.cpp | `b12428`, `llama-b12428-bin-ubuntu-x64.tar.gz` | official `ggml-org/llama.cpp` release (`--compare`) |
| Ollama | `v0.40.16`, `ollama-linux-amd64.tar.zst` | official `ollama/ollama` release (`--compare`) |
| Qwen3 0.6B / 1.7B Q4_K_M | `unsloth/Qwen3-0.6B-GGUF` @ `50968a44`, `unsloth/Qwen3-1.7B-GGUF` @ `d7f544ee` | sha256 = Hugging Face LFS id of the file at that revision |
| apt packages | Ubuntu 24.04 archive | versions written to `versions.txt` |

Nothing is piped into a shell. Downloads use https only. Cluaiz downloads the model itself with
`cluaiz pull` (the path the release smoke test uses); the script then checks the file against the pinned
sha256 and reuses the same file for llama.cpp and Ollama. The script needs no account, API key or token,
and the engine's session token is redacted from the result files if it appears there.

## Steps the script runs

1. Preflight: root, Ubuntu 24.04 x86_64, RAM of at least 2.5 GB, `systemd-run --scope` works, ports free, disk free.
2. `apt-get install` of curl, zstd, libgomp1, PostgreSQL, python3, psycopg2 and a few base tools.
3. Download and checksum Cluaiz (and with `--compare` llama.cpp and Ollama); unpack into a temporary folder.
4. Set the settings of the first VPS load run (2026-11-20) with `cluaiz config set`; `cluaiz pull` the model; check its sha256.
5. Create the database `cluaiz_repro_site` with the tables and 5,000 rows of `webapp/server.js`; start `neighbour_app.py` on port 3100.
6. Write `machine.json`, start `sampler.py` (one row a second), generate the fixed inputs, measure the website alone for 30 s.
7. For each engine: start it with `vps26/engines.sh` `start_engine`, 15 s warm-up (not counted), 1 request at a time for `--seq-secs`, `--queue-level` in flight for `--queue-secs` (both through `vps26/workloads.py`), stop it.
8. `vps26/analyze.py`, then `table.py` prints the table. On exit everything it started is stopped and the database (and the PostgreSQL role, if the script created it) is dropped.

Reused unchanged: `engines.sh`, `workloads.py`, `generate_inputs.py`, `neighbour_app.py`, `sampler.py`,
`analyze.py`. Overridden after sourcing `engines.sh`: the install paths, `model_gguf_host`, and a copy of
`configure_cluaiz_home` without its `/root` symlinks and Docker part (the original cannot run elsewhere).

## How to read the table

Columns are engines, rows are the items above. Read in this order:

1. **CPU steal.** A few percent or more means the host took CPU back; the first VPS load run (2026-11-20) lost its engine comparison
   this way. Then do not compare engines, and rerun.
2. **Requests OK** should equal the number sent; **OOM kills** should be 0.
3. **Website failed checks** and **p95 / max** show whether the website stayed usable. Compare with the
   "website alone" line below the table.
4. **Tokens/s.** If "OK requests with a token count" is lower than the OK count, that engine did not report
   usage and the rate counts streamed chunks, not tokens. Do not quote it as tokens/s then.
5. Memory: "machine" includes the website and PostgreSQL; "engine held" is what the engine really holds,
   "mapped model file" is the model file the kernel can drop again.

## What it does not prove

- Not run 19 or run 40: a 10 + 5 minute load, one model and fixed inputs. It does not repeat the 1-hour soaks,
  the larger models, embeddings or speech.
- One run, one machine. Hosts differ and a shared VPS can throttle CPU; there is no repeat or spread.
- Engines run one after the other, not interleaved, so a CPU cap that starts mid-run hits the later ones
  (the reason for the steal row; run 40 interleaves, this script does not).
- Tokens/s depends on the engine reporting usage (see above); answer quality is not checked.
- The website is a small Python app with one query per check, not a real site.
- It uses the `v0.1.0` release, which may differ from the build behind the first VPS load run (2026-11-20) (it used a build from
  before commit `fe21984c`).

## Pending

| Item | State |
|---|---|
| Run the script end to end on a clean Ubuntu 24.04, 1 vCPU / 4 GB VM and fix what breaks | open |
| Record that run as a numbered run in `runs/` with the table and raw files | open |
| Check that `cluaiz pull unsloth/Qwen3-0.6B-GGUF:Q4_K_M` registers `qwen3-0.6b-gguf-Q4_K_M` on a fresh home, and that the file's sha256 equals the pin | open |
| Check the `v0.1.0` binary: starts without extra libraries, sends the usage chunk in streamed chat (the first VPS load run (2026-11-20)'s build did not) | open |
| Check `systemd-run --scope` memory limit and the sampler's cgroup read on the clean VM | open |
| Check llama.cpp `b12428` flags (`--reasoning off`, `-fa auto`) and Ollama model creation from the shared GGUF | open |
| Measure the real total time with and without `--compare`; correct "Expected run time" | open |
| `shellcheck` run (not installed where this was written) | open |
| Decide where the public command points (a tagged commit, so the harness version matches the pins) | open |
| `--compare` runs engines one after the other; add an interleaved mode if CPU steal shows up | open |
