qwen3.5-2b-mtp-gguf-Q4_K_M | greedy, thinking off, 2 rounds of MTP off then on

Full GPU | ngl -1 | n_cpu_moe 0 | n_ctx 16384 | from disk 0.0 GB | VRAM taken: off 1.92 GB, on 2.11 GB

| prompt | tokens | t/s off | t/s on | gain | gain range | kept | prompt t/s off | prompt t/s on | same text |
| ------ | -----: | ------: | -----: | ---: | ---------: | ---: | -------------: | ------------: | :-------: |
| code | 222 | 155.91 | 176.49 | 1.13x | 1.13-1.14 | 64% | 1397.1 | 1033.3 | no |
| chat | 154 | 156.03 | 126.36 | 0.81x | 0.81-0.81 | 34% | 1466.4 | 1095.9 | no |
| list | 181 | 156.08 | 218.72 | 1.40x | 1.40-1.40 | 90% | 1511.2 | 1114.4 | yes |
| long3k | 192 | 151.21 | 137.80 | 0.91x | 0.91-0.92 | 44% | 10595.5 | 8879.9 | no |
