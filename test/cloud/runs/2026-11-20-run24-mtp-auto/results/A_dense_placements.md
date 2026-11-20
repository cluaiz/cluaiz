qwen3.5-2b-mtp-gguf-Q4_K_M | greedy, thinking off, 2 rounds of MTP off, on and auto

Full GPU | ngl -1 | n_cpu_moe 0 | n_ctx 16384 | from disk 0.0 GB | VRAM taken: off 1.92 GB, on 2.11 GB

| prompt | tokens | t/s off | t/s on | gain | gain range | kept | t/s auto | gain auto | auto guessed / stopped | prompt t/s off | prompt t/s on | same text |
| ------ | -----: | ------: | -----: | ---: | ---------: | ---: | -------: | --------: | :--------------------: | -------------: | ------------: | :-------: |
| code | 222 | 158.04 | 176.73 | 1.12x | 1.12-1.12 | 64% | 171.87 | 1.09x | 2/0 of 2 | 1429.3 | 1056.1 | no |
| chat | 154 | 157.99 | 126.61 | 0.80x | 0.80-0.80 | 34% | 134.95 | 0.85x | 2/2 of 2 | 1489.3 | 1112.6 | no |
| list | 181 | 158.03 | 218.63 | 1.38x | 1.38-1.38 | 90% | 209.62 | 1.33x | 2/0 of 2 | 1555.5 | 1153.7 | yes |
| long | 128 | 156.58 | 120.90 | 0.77x | 0.77-0.78 | 32% | 126.44 | 0.81x | 2/2 of 2 | 10182.0 | 8387.2 | no |

GPU + RAM split | ngl 25 | n_cpu_moe 0 | n_ctx 16384 | from disk 0.0 GB | VRAM taken: off 1.87 GB, on 2.08 GB

| prompt | tokens | t/s off | t/s on | gain | gain range | kept | t/s auto | gain auto | auto guessed / stopped | prompt t/s off | prompt t/s on | same text |
| ------ | -----: | ------: | -----: | ---: | ---------: | ---: | -------: | --------: | :--------------------: | -------------: | ------------: | :-------: |
| code | 232 | 122.05 | 127.58 | 1.05x | 1.02-1.07 | 63% | 129.22 | 1.06x | 2/1 of 2 | 763.0 | 623.6 | no |
| chat | 142 | 122.15 | 94.73 | 0.78x | 0.77-0.78 | 35% | 101.40 | 0.83x | 2/2 of 2 | 788.7 | 655.0 | no |
| list | 181 | 122.34 | 162.92 | 1.33x | 1.32-1.35 | 90% | 158.92 | 1.30x | 2/0 of 2 | 827.6 | 668.7 | yes |
| long | 128 | 121.67 | 91.72 | 0.75x | 0.75-0.76 | 34% | 101.73 | 0.84x | 2/2 of 2 | 7546.4 | 6352.5 | no |

CPU only | ngl 0 | n_cpu_moe 0 | n_ctx 16384 | from disk 0.0 GB | VRAM taken: off ?, on ?

| prompt | tokens | t/s off | t/s on | gain | gain range | kept | t/s auto | gain auto | auto guessed / stopped | prompt t/s off | prompt t/s on | same text |
| ------ | -----: | ------: | -----: | ---: | ---------: | ---: | -------: | --------: | :--------------------: | -------------: | ------------: | :-------: |
| code | 235 | 17.70 | 15.83 | 0.89x | 0.89-0.90 | 57% | 17.72 | 1.00x | 2/2 of 2 | 72.4 | 65.2 | no |
| chat | 161 | 17.71 | 12.30 | 0.69x | 0.68-0.70 | 36% | 14.72 | 0.83x | 2/2 of 2 | 76.4 | 69.1 | no |
| list | 181 | 17.42 | 21.73 | 1.25x | 1.24-1.25 | 90% | 21.44 | 1.23x | 2/0 of 2 | 75.6 | 68.9 | yes |
| long | 128 | 17.28 | 12.14 | 0.70x | 0.69-0.72 | 39% | 14.94 | 0.86x | 2/2 of 2 | 117.1 | 108.7 | no |
