qwen3.6-35b-a3b-mtp-gguf-UD-IQ4_XS | greedy, thinking off, 2 rounds of MTP off then on

Full GPU | ngl -1 | n_cpu_moe 0 | n_ctx 16384 | from disk 0.0 GB | VRAM taken: off 16.96 GB, on 17.67 GB

| prompt | tokens | t/s off | t/s on | gain | gain range | kept | prompt t/s off | prompt t/s on | same text |
| ------ | -----: | ------: | -----: | ---: | ---------: | ---: | -------------: | ------------: | :-------: |
| code | 237 | 73.46 | 110.90 | 1.51x | 1.50-1.51 | 81% | 431.0 | 304.9 | no |
| chat | 198 | 73.51 | 89.15 | 1.21x | 1.21-1.21 | 54% | 447.7 | 323.2 | no |
| list | 241 | 73.31 | 126.63 | 1.73x | 1.73-1.73 | 98% | 462.3 | 308.8 | yes |
| long | 128 | 72.90 | 92.07 | 1.26x | 1.26-1.26 | 58% | 2394.6 | 2085.4 | no |

GPU + RAM split | ngl 41 | n_cpu_moe 0 | n_ctx 16384 | from disk 0.0 GB | VRAM taken: off 16.55 GB, on 17.26 GB

| prompt | tokens | t/s off | t/s on | gain | gain range | kept | prompt t/s off | prompt t/s on | same text |
| ------ | -----: | ------: | -----: | ---: | ---------: | ---: | -------------: | ------------: | :-------: |
| code | 237 | 58.46 | 87.82 | 1.50x | 1.50-1.51 | 85% | 231.9 | 189.5 | yes |
| chat | 204 | 57.02 | 61.94 | 1.09x | 1.06-1.12 | 45% | 236.4 | 194.5 | no |
| list | 241 | 52.11 | 97.21 | 1.87x | 1.66-2.12 | 97% | 225.1 | 199.4 | yes |
| long | 128 | 57.70 | 70.94 | 1.23x | 1.21-1.25 | 62% | 1594.2 | 1474.4 | no |

CPU only | ngl 0 | n_cpu_moe 0 | n_ctx 16384 | from disk 0.0 GB | VRAM taken: off ?, on ?

| prompt | tokens | t/s off | t/s on | gain | gain range | kept | prompt t/s off | prompt t/s on | same text |
| ------ | -----: | ------: | -----: | ---: | ---------: | ---: | -------------: | ------------: | :-------: |
| code | 237 | 6.51 | 7.43 | 1.14x | 1.13-1.15 | 77% | 15.2 | 14.3 | no |
| chat | 199 | 6.53 | 5.47 | 0.84x | 0.84-0.84 | 48% | 15.1 | 14.3 | no |
| list | 241 | 6.52 | 8.76 | 1.34x | 1.34-1.35 | 98% | 15.8 | 14.9 | yes |
| long | 128 | 6.42 | 6.09 | 0.95x | 0.93-0.97 | 61% | 34.9 | 33.4 | no |
