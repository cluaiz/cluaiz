qwen3.5-2b-mtp-gguf-Q4_K_M | greedy, thinking off, 2 rounds of MTP off, on and auto

Full GPU | ngl -1 | n_cpu_moe 0 | n_ctx 16384 | from disk 0.0 GB | VRAM taken: off 1.92 GB, on 2.11 GB

| prompt | tokens | t/s off | t/s on | gain | gain range | kept | t/s auto | gain auto | auto guessed / stopped | prompt t/s off | prompt t/s on | same text |
| ------ | -----: | ------: | -----: | ---: | ---------: | ---: | -------: | --------: | :--------------------: | -------------: | ------------: | :-------: |
| code | 222 | 156.54 | 172.61 | 1.10x | 1.10-1.10 | 64% | 156.31 | 1.00x | 0/0 of 2 | 1402.0 | 1020.2 | no |
| chat | 154 | 156.50 | 123.73 | 0.79x | 0.79-0.79 | 34% | 156.33 | 1.00x | 0/0 of 2 | 1478.1 | 1097.6 | no |
| list | 181 | 156.72 | 213.97 | 1.37x | 1.36-1.37 | 90% | 156.26 | 1.00x | 0/0 of 2 | 1532.7 | 1109.4 | yes |
| long3k | 192 | 151.58 | 135.13 | 0.89x | 0.89-0.89 | 44% | 151.27 | 1.00x | 0/0 of 2 | 10657.5 | 8846.9 | no |
