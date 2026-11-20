qwen3.6-35b-a3b-mtp-gguf-UD-IQ4_XS | greedy, thinking off, 1 rounds of MTP off, on and auto

MoE split (attention on GPU, experts in RAM) | ngl -1 | n_cpu_moe 41 | n_ctx 16384 | from disk 0.0 GB | VRAM taken: off 2.80 GB, on 3.25 GB

| prompt | tokens | t/s off | t/s on | gain | gain range | kept | t/s auto | gain auto | auto guessed / stopped | prompt t/s off | prompt t/s on | same text |
| ------ | -----: | ------: | -----: | ---: | ---------: | ---: | -------: | --------: | :--------------------: | -------------: | ------------: | :-------: |
| code | 235 | 13.02 | 10.37 | 0.80x | 0.80-0.80 | 87% | 12.63 | 0.97x | 0/0 of 1 | 12.8 | 7.7 | no |
| chat | 198 | 11.47 | 7.33 | 0.64x | 0.64-0.64 | 47% | 11.67 | 1.02x | 0/0 of 1 | 7.2 | 4.4 | no |
| list | 241 | 8.73 | 8.61 | 0.99x | 0.99-0.99 | 97% | 8.79 | 1.01x | 0/0 of 1 | 7.1 | 5.0 | yes |
| long | 128 | 9.26 | 8.43 | 0.91x | 0.91-0.91 | 61% | 9.48 | 1.02x | 0/0 of 1 | 22.7 | 17.2 | no |
