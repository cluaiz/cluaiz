qwen3.5-2b-mtp-gguf-Q4_K_M | greedy, thinking off, 2 rounds of MTP off then on

Full GPU | ngl -1 | n_cpu_moe 0 | n_ctx 16384 | from disk 0.0 GB | VRAM taken: off 1.92 GB, on 2.11 GB

| prompt | tokens | t/s off | t/s on | gain | gain range | kept | prompt t/s off | prompt t/s on | same text |
| ------ | -----: | ------: | -----: | ---: | ---------: | ---: | -------------: | ------------: | :-------: |
| code | 222 | 158.07 | 177.31 | 1.12x | 1.12-1.12 | 64% | 1434.2 | 1047.9 | no |
| chat | 154 | 157.88 | 126.82 | 0.80x | 0.80-0.81 | 34% | 1491.9 | 1107.7 | no |
| list | 181 | 157.86 | 219.18 | 1.39x | 1.39-1.39 | 90% | 1534.8 | 1139.7 | yes |
| long | 128 | 156.48 | 121.82 | 0.78x | 0.77-0.78 | 32% | 10150.1 | 8370.2 | no |

GPU + RAM split | ngl 25 | n_cpu_moe 0 | n_ctx 16384 | from disk 0.0 GB | VRAM taken: off 1.87 GB, on 2.08 GB

| prompt | tokens | t/s off | t/s on | gain | gain range | kept | prompt t/s off | prompt t/s on | same text |
| ------ | -----: | ------: | -----: | ---: | ---------: | ---: | -------------: | ------------: | :-------: |
| code | 232 | 123.45 | 131.35 | 1.06x | 1.06-1.06 | 63% | 770.3 | 626.0 | no |
| chat | 142 | 122.26 | 95.89 | 0.78x | 0.78-0.79 | 35% | 813.8 | 662.9 | no |
| list | 181 | 124.20 | 166.30 | 1.34x | 1.34-1.34 | 90% | 830.4 | 680.2 | yes |
| long | 128 | 123.23 | 93.81 | 0.76x | 0.76-0.76 | 34% | 7647.8 | 6390.9 | no |

CPU only | ngl 0 | n_cpu_moe 0 | n_ctx 16384 | from disk 0.0 GB | VRAM taken: off ?, on ?

| prompt | tokens | t/s off | t/s on | gain | gain range | kept | prompt t/s off | prompt t/s on | same text |
| ------ | -----: | ------: | -----: | ---: | ---------: | ---: | -------------: | ------------: | :-------: |
| code | 235 | 18.10 | 16.13 | 0.89x | 0.89-0.90 | 57% | 72.8 | 66.1 | no |
| chat | 161 | 18.24 | 12.46 | 0.68x | 0.68-0.68 | 36% | 76.5 | 69.8 | no |
| list | 181 | 17.90 | 22.00 | 1.23x | 1.20-1.25 | 90% | 76.5 | 69.4 | yes |
| long | 128 | 17.80 | 12.27 | 0.69x | 0.69-0.69 | 39% | 118.1 | 109.3 | no |
