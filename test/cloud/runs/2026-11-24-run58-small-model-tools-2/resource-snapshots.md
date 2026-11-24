# Run 58 resource snapshots

Free RAM is in GiB; free VRAM is in MiB out of 4094 MiB. These values were printed by `runner.py` at each start and end. The earlier sandboxed attempt (7.62 GiB / 2628 MiB at start) made no scored requests and is excluded.

| Scored invocation | Start RAM | Start VRAM | End RAM | End VRAM |
|---|---:|---:|---:|---:|
| Baseline Qwen3 0.6B off/on | 5.15 | 2624 | 7.08 | 2305 |
| Baseline Qwen3 1.7B off/on | 7.15 | 2233 | 6.70 | 2615 |
| Baseline Qwen3 4B off/on | 6.63 | 2524 | 4.93 | 2851 |
| Baseline Llama 3.2 3B off | 4.71 | 2724 | 6.53 | 2782 |
| Baseline LFM3 1.7B off | 6.35 | 2794 | 4.80 | 2778 |
| Baseline MiniCPM5 1B off/on | 3.99 | 2778 | 7.68 | 2828 |
| Hint Qwen3 0.6B and 1.7B off/on | 7.69 | 2829 | 7.73 | 2862 |

The machine was shared with other work, so these timings are observational rather than controlled cross-model speed results.
