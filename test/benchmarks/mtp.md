# MTP (multi-token prediction), Auto against off and on (run 24, 2026-11-20)

`cluaiz bench --mtp-compare` on a GCP L4, same pinned placement for off, on and auto, Qwen3.5 2B MTP Q4_K_M
(dense) and Qwen3.6 35B A3B MTP UD-IQ4_XS (MoE). Gain over MTP off:

| Case | Placement | on | Auto |
|---|---|---|---|
| Dense 2B | Full GPU, four prompts | 0.77-1.38x | 0.81-1.33x |
| MoE 35B A3B | Full GPU | 1.21-1.72x | 1.19-1.66x |
| MoE 35B A3B | Split (ngl 41) | 1.06-1.66x | 1.15-1.61x |
| MoE, experts in RAM (8 GB VRAM) | MoE split | 0.70-1.14x | 0.89-1.12x |
| MoE, experts partly from the SSD | MoE split | 0.64-0.99x | 0.97-1.02x (Auto turned MTP off) |

Auto keeps most of the gain where MTP is fast and loses much less where it is slow; its guard costs 2-5% on a
gainful case. Run: [run 24](../cloud/runs/2026-11-20-run24-mtp-auto/SUMMARY.md).
