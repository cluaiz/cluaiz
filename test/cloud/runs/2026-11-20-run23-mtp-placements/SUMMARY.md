# Run 23: MTP off and on in every placement, laptop and L4 (2026-11-20)

| | |
|---|---|
| Cloud machine | Google Cloud `g2-standard-8` (us-central1-c): NVIDIA L4 23 GB, 8 vCPU, 31 GB RAM, Debian 13, CUDA build for sm_89 |
| Laptop | Ryzen 7 7435HS, 23.7 GB RAM, RTX 3050 Laptop 4 GB, Windows 11; other apps held about 1.5 GB of the GPU |
| Cluaiz | `main` at `07c039e8` plus the changes of this run: `bench --mtp-compare` alternates MTP off and on per round, `--placements` and `--temperature` for it, MTP prompt timing by wall clock |
| Models | `unsloth/Qwen3.5-2B-MTP-GGUF` (dense, 24 layers + 1 NextN layer): Q4_K_M, Q4_0, Q8_0, IQ4_XS; `unsloth/Qwen3.6-35B-A3B-MTP-GGUF` UD-IQ4_XS (MoE, 18.2 GB, about 3B active per token) |
| Records | [`results-cloud/`](results-cloud/): one `.json` (tool output), `.md` (table), `.err` (progress), `.gpu` (GPU clock, power, memory before and after) per case, `steps.log`, `run23.log`; [`results-laptop/`](results-laptop/); [`scripts/`](scripts/) (`run23.sh` runs every cloud case) |

Previous MTP record: [run 22](../2026-11-19-run22-laptop-mtp/SUMMARY.md). What MTP is: `engine/llama/README.md`, "MTP
speculative decoding".

## 1. What changed in the measuring tool before this run

| Change | Why |
|---|---|
| MTP off and on alternate in every round, each a fresh load | run 22 measured all "off" runs, then all "on" runs; the laptop GPU clock moved the plain speed from 55 to 72 t/s between them. The tables below give the gain range over the rounds: on the L4 it was within 0.01x in most rows |
| `--placements` with `--mtp-compare` | every placement the planner can use, so one command shows where MTP gains and where it loses for one model |
| VRAM taken per load (from the load history) | the memory MTP adds, needed before the planner can count it |
| `--temperature` | sampled answers, not only greedy |
| With MTP, prompt time is the wall clock of the prompt decodes, after a synchronize | llama.cpp's counters are updated only at a synchronize and see only the target model; run 22 printed 0.0 prompt t/s in a split placement. The wall clock also counts the drafter reading every prompt chunk, which is MTP's real prompt cost |

## 2. Results

Gain = generation t/s with MTP on over off (mean of the rounds); kept = guesses kept. Prompts as in
run 22: `code`, `chat` (free prose), `list` (JSON array), `long` (summary of a 690-word text);
`long3k` is a summary of `engine/runtime/README.md` (2,467 words, about 3,000 tokens). Draft 3 unless
written.

### Dense 2B, L4 (cases A-D)

| Case | Placement | Off t/s | code | chat | list | long | Prompt t/s off -> on |
|---|---|---|---|---|---|---|---|
| A, draft 3 | Full GPU | 158 | 1.12x (64%) | 0.80x (34%) | 1.39x (90%) | 0.78x (32%) | 1434 -> 1048 (code) |
| A, draft 3 | Split, 1 block in RAM (ngl 25) | 123 | 1.06x | 0.78x | 1.34x | 0.76x | 770 -> 626 |
| A, draft 3 | CPU only | 18 | 0.89x | 0.68x | 1.23x | 0.69x | 73 -> 66 |
| A, draft 1 | Full GPU | 156 | 1.07x (82%) | 0.98x (63%) | 1.16x (98%) | 0.95x (59%) | 1402 -> 1046 |
| A, draft 1 | Split (ngl 25) | 123 | 1.10x | 0.96x | 1.14x | 0.94x | 769 -> 642 |
| A, draft 1 | CPU only | 18 | 0.91x | 0.81x | 0.99x | 0.83x | 73 -> 67 |
| B, Q4_0 | Full GPU | 165 | 1.15x | 0.84x | 1.35x | 0.92x | 1494 -> 1059 |
| B, Q8_0 | Full GPU | 107 | **1.31x** | 0.84x | **1.60x** | 0.96x | 1119 -> 803 |
| B, IQ4_XS | Full GPU | 161 | 1.16x | 0.80x | 1.39x | 0.90x | 1457 -> 1028 |
| C, temperature 0.7 (3 rounds) | Full GPU | 155 | 1.06x (0.92-1.14) | 0.79x | 1.41x | 0.85x | 1378 -> 1022 |
| D, `long3k` | Full GPU | 151 | | | | 0.91x (`long3k`) | 10596 -> 8880 |
| D, `long3k` | CPU only | 16 | | | | 0.66x (`long3k`) | 113 -> 103 |

### MoE 35B-A3B IQ4_XS, L4 (cases E, F)

| Case | Placement | VRAM off -> on | Off t/s | code | chat | list | long | Prompt t/s off -> on (code) |
|---|---|---|---|---|---|---|---|---|
| E | Full GPU | 16.96 -> 17.67 GB | 73 | **1.51x** (81%) | **1.21x** (54%) | **1.73x** (98%) | **1.26x** (58%) | 431 -> 305 |
| E | Split, 1 block in RAM (ngl 41) | 16.55 -> 17.26 GB | 52-58 | 1.50x | 1.09x | 1.87x (1.66-2.12) | 1.23x | 232 -> 190 |
| E | CPU only | - | 6.5 | 1.14x | 0.84x | 1.34x | 0.95x | 15.2 -> 14.3 |
| F, 8 GB VRAM left | MoE split: attention on GPU, experts of 41 blocks in RAM | 2.80 -> 3.25 GB | 14.5 | 1.05x (87%) | **0.70x** | 1.13x (97%) | 0.82x | 24.2 -> 23.6 |

### Dense 2B, laptop (RTX 3050 4 GB, planner's own placements with other apps open)

| Placement | VRAM off -> on | Off t/s | code | chat | list | long | Prompt t/s off -> on (chat) |
|---|---|---|---|---|---|---|---|
| Split, 4 GPU layers (the planner's choice with 1.5 GB of the GPU used by other apps) | 0.97 -> 1.08 GB | 14-19 | 1.00x (0.95-1.06) | 0.85x | 1.53x | 1.05x | 145 -> 95 |
| CPU only | - | 18-19 | 1.27x (1.18-1.36) | 0.83x | 1.43x | 0.85x | 87 -> 85 |

### Weights read from the SSD (cases G, H)

G: the MoE with 8 GB of VRAM left and RAM capped at 12 GB by a cgroup (the experts are about 14 GB),
page cache dropped first. The case read 101,865 MiB from the disk (288,078 major page faults) over
its 16 answers.

| Case | Placement | Off t/s | code | chat | list | long | Prompt t/s off -> on (code) |
|---|---|---|---|---|---|---|---|
| F (experts in RAM, for comparison) | MoE split | 14.5 | 1.05x | 0.70x | 1.13x | 0.82x | 24.2 -> 23.6 |
| G (experts partly from the SSD) | MoE split | 9.2-13.1 | **0.72x** (87%) | **0.63x** | 0.98x (97%) | 0.91x | 14.2 -> 8.4 |

H (dense 2B, CPU only, RAM capped at 1.4 GB) was stopped by the cgroup's memory limit during the
MTP-on load: no result. The dense case on the SSD stays open.

The first G and H outputs, run before the cache drop was added, measured nothing on the disk (the
model was cached outside the cap by earlier cases) and are kept in `results-cloud/first-invalid/`.

## 3. What the runs show

1. **The model decides first, then the text.** The MoE model (about 3B active of 35B) gained on every
   prompt with all weights on the GPU, prose included (chat 1.21x, long 1.26x). The dense 2B lost on
   prose in every placement (0.68-0.85x at draft 3). One rule for all models would be wrong both ways.
2. **Experts in RAM take the MoE gain away.** The same MoE with its experts in RAM (case F, 8 GB VRAM)
   dropped to 0.70x on chat and 0.82x on long; code and JSON still gained a little (1.05x, 1.13x).
   This matches the research note: each checked token can route to other experts, read on the CPU.
3. **Bigger weights per token, bigger gain.** Q8_0 (slowest plain, 107 t/s) gained most (1.31x code,
   1.60x list); Q4_0 and IQ4_XS were like Q4_K_M. A decode that waits on memory reads gains most from
   checking several tokens per read.
4. **Draft 1 is the safe default on the GPU, not on the CPU.** On the L4 draft 1 never fell below
   0.94x (GPU and split); on the CPU it fell to 0.81x. Draft 3 gains more on JSON and code.
5. **Temperature 0.7 behaves like greedy** (list 1.41x, chat 0.79x); only the round-to-round range
   grows (code 0.92-1.14x).
6. **MTP costs prompt speed on the GPU: 19-34% on short prompts, 16% on 3,000 tokens** (10,596 ->
   8,880 t/s); on the CPU 6-14%. The drafter reads every prompt chunk. For a long prompt with a
   short answer, this cost can be larger than the generation gain.
7. **Experts read from the SSD: MTP loses on every prompt** (G: code 0.72x, chat 0.63x, list 0.98x).
   Each checked token can need other experts, and these come from the disk. A rule the planner can
   use without a measurement: MoE with experts streamed from disk, MTP off.
8. **MTP memory:** dense 2B +0.11 to 0.19 GB, MoE +0.45 GB (experts in RAM) to +0.71 GB (full GPU).
   A model that just fits can be pushed into a smaller placement by MTP; the plan must count it.
9. **The laptop's own planner picked a 4-layer split** (other apps held the GPU), where MTP still
   gained on JSON (1.53x) and lost on prose (0.85x). CPU-only gains on the laptop (code 1.27x) were
   higher than on the L4 vCPUs (0.89x): CPU type matters too.

For the Auto rules: a measured result per model and placement (this tool, stored in the speed file)
plus a guard inside each answer (stop drafting when the kept share falls) fit these results;
fixed rules by placement alone do not (MoE full GPU gains on prose, dense full GPU loses). Two
placement rules are safe without a measurement: experts streamed from the SSD (G) and MoE experts
in RAM with prose (F) lose.

## 4. How to run it again

```bash
# one model, every placement, MTP off/on alternated, 2 rounds
cluaiz bench <model> --mtp-compare --placements -r 2
# draft size, temperature, own prompts
CLUAIZ_MTP_DRAFT=1 cluaiz bench <model> --mtp-compare --placements -r 2
cluaiz bench <model> --mtp-compare --temperature 0.7 -r 3
cluaiz bench <model> --mtp-compare --prompts scripts/prompts_long3k.json -r 2
# the whole cloud set on a Linux VM with the source unpacked in ~/cluaiz
bash scripts/run23.sh            # or a subset of cases: run23.sh AE
```

## 5. Not measured yet

| Case | Why | Needs |
|---|---|---|
| Dense weights read from the SSD (H) | expected to gain (one read serves several checked tokens), the opposite of the MoE result | a cap that leaves room for MTP's memory (H was stopped at 1.4 GB) |
| Laptop full GPU with the planner's own choice | the planner chose a 4-layer split with other apps open | a run with the GPU free |
| Vulkan, Metal | other kernels | those machines |
