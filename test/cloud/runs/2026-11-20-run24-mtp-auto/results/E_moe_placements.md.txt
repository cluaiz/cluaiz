qwen3.6-35b-a3b-mtp-gguf-UD-IQ4_XS | greedy, thinking off, 2 rounds of MTP off, on and auto

Full GPU | ngl -1 | n_cpu_moe 0 | n_ctx 16384 | from disk 0.0 GB | VRAM taken: off 16.96 GB, on 17.67 GB

| prompt | tokens | t/s off | t/s on | gain | gain range | kept | t/s auto | gain auto | auto guessed / stopped | prompt t/s off | prompt t/s on | same text |
| ------ | -----: | ------: | -----: | ---: | ---------: | ---: | -------: | --------: | :--------------------: | -------------: | ------------: | :-------: |
| code | 237 | 72.50 | 109.53 | 1.51x | 1.50-1.52 | 81% | 111.10 | 1.53x | 2/0 of 2 | 428.5 | 308.7 | no |
| chat | 198 | 72.59 | 87.85 | 1.21x | 1.20-1.22 | 54% | 86.51 | 1.19x | 2/0 of 2 | 441.4 | 318.7 | no |
| list | 241 | 72.44 | 124.63 | 1.72x | 1.71-1.73 | 98% | 120.43 | 1.66x | 2/0 of 2 | 459.1 | 301.4 | yes |
| long | 128 | 72.12 | 90.74 | 1.26x | 1.25-1.27 | 58% | 87.18 | 1.21x | 2/0 of 2 | 2219.2 | 2054.8 | no |

GPU + RAM split | ngl 41 | n_cpu_moe 0 | n_ctx 16384 | from disk 0.0 GB | VRAM taken: off 16.55 GB, on 17.26 GB

| prompt | tokens | t/s off | t/s on | gain | gain range | kept | t/s auto | gain auto | auto guessed / stopped | prompt t/s off | prompt t/s on | same text |
| ------ | -----: | ------: | -----: | ---: | ---------: | ---: | -------: | --------: | :--------------------: | -------------: | ------------: | :-------: |
| code | 237 | 57.89 | 87.22 | 1.51x | 1.50-1.51 | 85% | 85.97 | 1.49x | 2/0 of 2 | 233.1 | 189.0 | yes |
| chat | 204 | 57.79 | 61.16 | 1.06x | 1.06-1.06 | 45% | 66.30 | 1.15x | 2/0 of 2 | 235.7 | 193.5 | no |
| list | 241 | 57.88 | 96.24 | 1.66x | 1.66-1.66 | 97% | 93.27 | 1.61x | 2/0 of 2 | 240.4 | 198.3 | yes |
| long | 128 | 57.66 | 69.53 | 1.21x | 1.20-1.22 | 62% | 68.76 | 1.19x | 2/0 of 2 | 1575.2 | 1448.7 | no |

CPU only | ngl 0 | n_cpu_moe 0 | n_ctx 16384 | from disk 0.0 GB | VRAM taken: off ?, on ?

| prompt | tokens | t/s off | t/s on | gain | gain range | kept | t/s auto | gain auto | auto guessed / stopped | prompt t/s off | prompt t/s on | same text |
| ------ | -----: | ------: | -----: | ---: | ---------: | ---: | -------: | --------: | :--------------------: | -------------: | ------------: | :-------: |
| code | 237 | 6.51 | 7.42 | 1.14x | 1.13-1.15 | 77% | 7.39 | 1.14x | 2/2 of 2 | 15.4 | 14.4 | no |
| chat | 199 | 6.49 | 5.42 | 0.84x | 0.82-0.85 | 48% | 6.10 | 0.94x | 2/2 of 2 | 15.1 | 14.3 | no |
| list | 241 | 6.44 | 8.64 | 1.34x | 1.32-1.36 | 98% | 8.61 | 1.34x | 2/0 of 2 | 15.7 | 14.8 | yes |
| long | 128 | 6.32 | 6.09 | 0.96x | 0.95-0.98 | 61% | 5.62 | 0.89x | 2/2 of 2 | 34.8 | 33.3 | no |
