qwen3.5-2b-mtp-gguf-Q4_K_M | greedy, thinking off, 2 rounds of MTP off then on

GPU + RAM split | ngl 4 | n_cpu_moe 0 | n_ctx 4096 | from disk 0.0 GB | VRAM taken: off 0.97 GB, on 1.08 GB

| prompt | tokens | t/s off | t/s on | gain | gain range | kept | prompt t/s off | prompt t/s on | same text |
| ------ | -----: | ------: | -----: | ---: | ---------: | ---: | -------------: | ------------: | :-------: |
| code | 209 | 18.51 | 18.52 | 1.00x | 0.95-1.06 | 67% | 89.4 | 62.2 | no |
| chat | 209 | 16.52 | 14.03 | 0.85x | 0.80-0.90 | 36% | 144.8 | 95.3 | no |
| list | 180 | 16.29 | 24.87 | 1.53x | 1.43-1.61 | 91% | 73.5 | 90.7 | yes |
| long | 128 | 13.89 | 14.59 | 1.05x | 1.03-1.07 | 31% | 861.7 | 821.7 | no |

CPU only | ngl 0 | n_cpu_moe 0 | n_ctx 4096 | from disk 0.0 GB | VRAM taken: off ?, on ?

| prompt | tokens | t/s off | t/s on | gain | gain range | kept | prompt t/s off | prompt t/s on | same text |
| ------ | -----: | ------: | -----: | ---: | ---------: | ---: | -------------: | ------------: | :-------: |
| code | 212 | 18.96 | 23.98 | 1.27x | 1.18-1.36 | 72% | 90.0 | 77.5 | no |
| chat | 169 | 18.98 | 15.84 | 0.83x | 0.81-0.86 | 38% | 86.7 | 85.1 | no |
| list | 181 | 19.47 | 27.83 | 1.43x | 1.34-1.52 | 90% | 91.0 | 81.2 | yes |
| long | 128 | 17.64 | 15.06 | 0.85x | 0.81-0.90 | 47% | 115.7 | 108.4 | no |
