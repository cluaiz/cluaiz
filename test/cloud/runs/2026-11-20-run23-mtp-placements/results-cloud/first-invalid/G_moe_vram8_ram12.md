qwen3.6-35b-a3b-mtp-gguf-UD-IQ4_XS | greedy, thinking off, 1 rounds of MTP off then on

MoE split (attention on GPU, experts in RAM) | ngl -1 | n_cpu_moe 41 | n_ctx 16384 | from disk 0.0 GB | VRAM taken: off 2.80 GB, on 3.25 GB

| prompt | tokens | t/s off | t/s on | gain | gain range | kept | prompt t/s off | prompt t/s on | same text |
| ------ | -----: | ------: | -----: | ---: | ---------: | ---: | -------------: | ------------: | :-------: |
| code | 235 | 14.52 | 15.22 | 1.05x | 1.05-1.05 | 87% | 24.1 | 23.5 | no |
| chat | 198 | 14.54 | 10.17 | 0.70x | 0.70-0.70 | 47% | 28.7 | 24.4 | no |
| list | 241 | 14.54 | 16.46 | 1.13x | 1.13-1.13 | 97% | 29.2 | 27.0 | yes |
| long | 128 | 14.45 | 11.88 | 0.82x | 0.82-0.82 | 61% | 147.8 | 139.0 | no |
