qwen3.6-35b-a3b-mtp-gguf-UD-IQ4_XS | greedy, thinking off, 2 rounds of MTP off, on and auto

MoE split (attention on GPU, experts in RAM) | ngl -1 | n_cpu_moe 41 | n_ctx 16384 | from disk 0.0 GB | VRAM taken: off 2.80 GB, on 3.25 GB

| prompt | tokens | t/s off | t/s on | gain | gain range | kept | t/s auto | gain auto | auto guessed / stopped | prompt t/s off | prompt t/s on | same text |
| ------ | -----: | ------: | -----: | ---: | ---------: | ---: | -------: | --------: | :--------------------: | -------------: | ------------: | :-------: |
| code | 235 | 14.42 | 15.25 | 1.06x | 1.06-1.06 | 87% | 15.27 | 1.06x | 2/2 of 2 | 24.3 | 23.6 | no |
| chat | 198 | 14.43 | 10.17 | 0.70x | 0.70-0.71 | 47% | 13.76 | 0.95x | 2/2 of 2 | 28.5 | 24.9 | no |
| list | 241 | 14.45 | 16.49 | 1.14x | 1.14-1.14 | 97% | 16.24 | 1.12x | 2/0 of 2 | 29.4 | 27.0 | yes |
| long | 128 | 14.39 | 11.94 | 0.83x | 0.83-0.83 | 61% | 12.75 | 0.89x | 2/2 of 2 | 146.8 | 138.9 | no |
