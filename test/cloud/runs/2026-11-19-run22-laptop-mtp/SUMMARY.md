# Run 22: MTP speculative decoding on a 4 GB laptop GPU (2026-11-19)

| | |
|---|---|
| Machine | Laptop, AMD Ryzen 7 7435HS (8 cores), 23.7 GB RAM, RTX 3050 Laptop 4 GB (driver 610.47), Windows 11 |
| Cluaiz | local release builds of `main` during the MTP work: before commit `d9ae0a48` (server runs), then `1cf2d55b` plus the Step 1 changes (`bench --mtp-compare`, settings `optimization.mtp`, MTP timings fix, `CLUAIZ_MTP_PMIN`) |
| llama.cpp | official Windows CUDA 12.4 build `b12534`, `llama-server --spec-type draft-mtp` |
| Model | `unsloth/Qwen3.5-2B-MTP-GGUF` Q4_K_M, file `Qwen3.5-2B-Q4_K_M.gguf` 1.33 GB with one NextN layer inside; hybrid model (recurrent + attention layers) |
| Isolation | separate `CLUAIZ_HOME`, port 8011 (llama-server 8012), `--local` for `bench` |
| Other apps | browser, IDEs and desktop apps open; they held part of the GPU, so the planner saw 1.44-1.52 GB usable VRAM in the server runs and pinned placements were used |
| Records | [`results/bench-mtp/`](results/bench-mtp/): every `cluaiz bench --mtp-compare` table as printed; [`results/server/`](results/server/): server logs, answers and printed API numbers (`api_results.txt`) of the API comparison; [`scripts/`](scripts/): the scripts that produced them (set `S` to a work folder) |

What MTP is and how the Engine does it: `engine/llama/README.md`, section "MTP speculative decoding".
In short, the model's NextN layer guesses the next 1-4 tokens, the model checks all guesses in one
decode, and every guess its own sampler would have picked saves one decode.

## 1. Development runs (API, one prompt, before the commit)

Prompt: "Write a Python function that checks whether a number is prime, then explain it in three
sentences.", temperature 0, thinking off, `max_tokens` 256, `/v1/chat/completions`, 4 runs per server
(run 0 is the warm-up). t/s is `usage.tokens_per_second` printed by `scripts/bench_cluaiz.sh`.

| Log | Setup | Without MTP, runs 1-3 | With MTP, runs 1-3 | What it showed |
|---|---|---|---|---|
| `cluaiz-plain`, `cluaiz-mtp` | planner Auto: full GPU, context 16384 | 47.3 / 51.8 / 52.6 | 18.7 / 22.0 / 26.5 | MTP took 2.51 GB VRAM against a plan of 2.22 GB and spilled into shared GPU memory. Cause: the MTP context kept logits for 512 output rows (+0.63 GB at a 248k vocabulary); fixed with `n_outputs_max = 1` |
| `cluaiz-plain4k`, `cluaiz-mtp4k` | context 4096 | 53.7 / 49.4 / 44.8 | 58.0 / 56.7 / 65.3 | speeds still moved run to run |
| `cluaiz-plain4kb`, `cluaiz-mtp4kb` | context 4096, KV q8_0, planner chose the layers | 49.5 / 51.5 / 52.7 (22 GPU layers, 221 tokens) | 67.6 / 64.5 / 65.9 (23 GPU layers, 224 tokens) | the two loads got different placements, so the answers differed: an unfair comparison. Since then MTP comparisons pin `n_gpu_layers` |
| `cluaiz-plainG`, `cluaiz-mtpG` | `n_gpu_layers 99` pinned, context 4096 | 68.3 / 67.4 / 67.8 (mean 67.8) | 86.4 / 87.3 / 81.9 (mean 85.2) | **1.26x**, 145 of 225 guesses kept (64%), answer byte-identical (`answer_out_cluaiz_plainG_1.txt` = `answer_out_cluaiz_mtpG_1.txt`) |
| `cluaiz-xplain`, `cluaiz-xmtp` | same, thinking on (268 tokens) and a JSON schema answer (39 tokens) | 66.0 and 50.9 | 95.3 and 76.0 | 75% and 79% kept; answers identical (`answer_x_*`) |

llama-server `b12534`, same file and settings (`scripts/bench_llama_server.sh`, `-ngl 99 -c 4096`):

| Pass | Without MTP | With MTP (`--spec-type draft-mtp`) |
|---|---|---|
| 1 | 61.2 / 54.9 / 62.0 | 70.7 / 68.1 / 68.0 (145 of 228 kept) |
| 2 (used in the README) | 65.0 / 66.6 / 68.2 | 78.0 / 84.2 / 83.7 (145 of 228 kept) |

Cluaiz with MTP was at parity with llama-server (85.2 against 82.0 t/s). llama-server's own answers
with and without MTP were not byte-identical (`answer_out_llama_plain.txt` against
`answer_out_llama_1.txt`, the MTP run).

The server logs show llama.cpp's own counters at "generation 1 tokens" with MTP on: llama.cpp counts
every decode of more than one token as prompt processing. This was fixed in Step 1 (the stream times
the generation itself when MTP drafted); the API numbers above come from the wall clock and are not
affected.

## 2. The measuring tool: `cluaiz bench --mtp-compare`

`cluaiz bench <model> --mtp-compare` (`cmd/src/cli/bench_mtp.rs`) plans once with MTP off, then loads
MTP off and MTP on with exactly that placement, context and KV type, and answers each prompt of
`cmd/src/cli/data/bench_mtp_prompts.json` greedily with thinking off: one warm-up, then `-r` runs, the
prompt cache cleared before every run so prompt speed is measured too.

| Prompt | What it is | Max tokens |
|---|---|---|
| `code` | a Python function and three sentences about it | 256 |
| `chat` | free prose: how a refrigerator works, about 150 words | 256 |
| `list` | ten European countries and capitals as a JSON array | 256 |
| `long` | five-bullet summary of a 690-word text | 128 |

Found while building it (fixed before the record): the bench flag was first named `--mtp` and its
field `mtp`, the same id as the load flag `--mtp on|off|auto` that `bench` also takes; clap matched
`--mtp on` to the wrong argument. Renamed to `--mtp-compare` (field `mtp_compare`). The settings test
`field_list_matches_the_struct` caught that `config set optimization.mtp` did not know the new field.

## 3. Results

Gain = generation t/s with MTP over without; kept share in brackets. Each line is one run of the
tool (files in `results/bench-mtp/`, numbered in the order they ran).

| File | Setup | Runs | Off t/s | code | chat | list | long |
|---|---|---|---|---|---|---|---|
| 1 | Full GPU, draft 3 (run A) | 3 | 55 | 1.19x (63%) | 0.85x (35%) | 1.49x (90%) | 0.95x (44%) |
| 2 | Full GPU, draft 1 (run A) | 2 | 56 | 1.16x (82%) | 1.07x (64%) | 1.24x (98%) | 1.09x (74%) |
| 2 | Full GPU, draft 2 (run A) | 2 | 57 | 1.25x (76%) | 1.01x (52%) | 1.38x (95%) | 1.05x (56%) |
| 2 | Full GPU, draft 4 | 2 | 57 | 1.09x (52%) | 0.84x (28%) | 1.91x (86%) | 1.08x (39%) |
| 3 | Split, `--n-gpu-layers 12`, draft 3 | 2 | 30 | 1.10x (62%) | 0.61x (32%) | 1.51x (91%) | 0.85x (33%) |
| 4 | CPU only, draft 3 | 1 | 21 | 0.97x (66%) | 0.65x (34%) | 1.30x (90%) | 0.85x (48%) |
| 5 | Full GPU, draft 3, `p_min` 0.5 | 2 | 75 | 1.03x (82%) | 0.76x (74%) | 1.40x (95%) | 0.75x (76%) |
| 5 | Full GPU, draft 3, `p_min` 0.75 | 2 | 75 | 0.96x (90%) | 0.70x (90%) | 1.21x (95%) | 0.76x (88%) |
| 5 | CPU only, draft 3, `p_min` 0.5 | 1 | 22 | 1.04x (77%) | 0.85x (72%) | 1.33x (96%) | 0.92x (75%) |
| 5 | CPU only, draft 3, `p_min` 0.75 | 1 | 23 | 1.04x (89%) | 0.82x (89%) | 1.17x (96%) | 0.86x (92%) |
| 6 | Full GPU, draft 3 (run B) | 2 | 72 | 1.12x (63%) | 0.81x (35%) | 1.38x (90%) | 0.90x (44%) |
| 6 | Full GPU, draft 1 (run B) | 2 | 71 | 1.05x (82%) | 0.96x (64%) | 1.14x (98%) | 1.02x (74%) |
| 6 | Full GPU, draft 2 (run B) | 2 | 71 | 1.17x (76%) | 0.97x (52%) | 1.33x (95%) | 1.00x (56%) |

"Draft" is `CLUAIZ_MTP_DRAFT`, the most tokens guessed per step (llama-server default 3). `p_min`
(`CLUAIZ_MTP_PMIN`) stops a draft at the first guess the drafter gives less than that probability.

Prompt t/s (columns in the files): full GPU, MTP on was not slower on any prompt (`long`, run B: 3689
off, 4011 on); CPU only it was up to 12% slower. In the split run the tool printed 0.0 for MTP on,
which is a measuring fault, not a result.

Text: same with and without MTP in 23 of 52 prompt pairs. Draft 1 kept the text in all 8 pairs; `list`
changed in 2 (split, CPU `p_min` 0.5). A check of 2-4 tokens rounds slightly differently from one
token at a time, which flips a greedy pick where two tokens are nearly tied; the answer is still the
model's own.

## 4. What the runs show

1. The text decides first. Structured output (`list`) gained in every setup, 1.14x to 1.91x; free
   prose (`chat`) lost in most setups. Code was in between.
2. Fewer guesses per step are the safest: draft 1 never fell below 0.96x; draft 3 and 4 fell to
   0.81-0.85x on prose on the GPU; draft 4 gave the best single result (1.91x on `list`).
3. Split and CPU lose more on prose (0.61x, 0.65x): checking 4 tokens there costs much more than one,
   while on a GPU that holds all weights it costs almost the same as one.
4. `p_min` raised the kept share (to 74-90%) but lowered the speed on the GPU: drafts got shorter while
   every step still paid for the drafter. It is left at llama.cpp's default 0.
5. The machine itself moves the numbers: the same command gave 55 t/s without MTP in run A and 72 t/s
   in run B (laptop GPU clock, other apps on the GPU), and the faster the plain run, the smaller the
   gain (draft 1 code: 1.16x then 1.05x). A difference under about 10% is noise here, and rows of
   different runs are not compared to each other.

For the Auto rules (plan step 4): a rule per model and placement alone cannot be right, because one
model on one placement gains on JSON and loses on prose. A guard inside the answer (stop drafting
when the kept share falls) or draft 1 as the default are the candidates; both need a test before
they are chosen.

## 5. How to run it again

```bash
# MTP off and on, the four default prompts, 2 runs each after a warm-up
cluaiz bench qwen3.5-2b-mtp-gguf-Q4_K_M --mtp-compare -r 2
# other draft size, p_min, placement
CLUAIZ_MTP_DRAFT=1 cluaiz bench qwen3.5-2b-mtp-gguf-Q4_K_M --mtp-compare -r 2
CLUAIZ_MTP_PMIN=0.5 cluaiz bench qwen3.5-2b-mtp-gguf-Q4_K_M --mtp-compare -r 1 --n-gpu-layers 0
# own prompts: a JSON list of {"name", "text", "max_tokens"}
cluaiz bench qwen3.5-2b-mtp-gguf-Q4_K_M --mtp-compare --prompts scripts/one_prompt.json -r 1
```

`-o json` prints the same results as JSON. The API comparison with llama-server:
`scripts/bench_cluaiz.sh <tag>` (with `CLUAIZ_MTP=on` for MTP) and `scripts/bench_llama_server.sh`
(add `--spec-type draft-mtp`).

## 6. Not measured yet

| Case | Why it matters | Needs |
|---|---|---|
| MoE with experts in RAM (`--n-cpu-moe`) | each checked token can route to other experts read on the CPU | an MoE MTP model; the smallest Qwen3.5/3.6-35B-A3B MTP file is 11.4 GB |
| Weights larger than free RAM (read from the SSD) | expert reads from the disk per checked token | same model |
| Temperature 0.7 | the sampled token must equal the drafter's greedy guess | a sampler option in `bench --mtp-compare` (it forces 0) |
| A 3000-token prompt | the drafter reads every prompt chunk | a longer prompt file |
| Q4_0, Q8_0, IQ4_XS | other GPU kernels for 2-4 rows | small downloads (1.3-2.1 GB) |
| Off and on alternated per repetition | run A / run B moved the plain speed by 30% | change in `bench_mtp.rs` |
| Prompt t/s with MTP on in a split placement | printed 0.0 | find where the timing is lost |
| Metal (Mac) and Vulkan | other kernels; reports disagree | a Mac and a Vulkan machine (pending) |
