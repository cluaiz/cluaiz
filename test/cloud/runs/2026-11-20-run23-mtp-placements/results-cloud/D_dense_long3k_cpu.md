qwen3.5-2b-mtp-gguf-Q4_K_M | greedy, thinking off, 1 rounds of MTP off then on

CPU only | ngl 0 | n_cpu_moe 0 | n_ctx 4096 | from disk 0.0 GB | VRAM taken: off ?, on ?

| prompt | tokens | t/s off | t/s on | gain | gain range | kept | prompt t/s off | prompt t/s on | same text |
| ------ | -----: | ------: | -----: | ---: | ---------: | ---: | -------------: | ------------: | :-------: |
| code | 235 | 17.86 | 16.01 | 0.90x | 0.90-0.90 | 57% | 72.7 | 65.8 | no |
| chat | 161 | 17.90 | 12.38 | 0.69x | 0.69-0.69 | 36% | 77.3 | 69.9 | no |
| list | 181 | 17.94 | 21.92 | 1.22x | 1.22-1.22 | 90% | 76.6 | 69.4 | yes |
| long3k | 192 | 15.79 | 10.36 | 0.66x | 0.66-0.66 | 38% | 112.6 | 102.5 | no |
