qwen3.5-2b-mtp-gguf-Q4_K_M | greedy, thinking off, 1 rounds of MTP off then on

CPU only | ngl 0 | n_cpu_moe 0 | n_ctx 4096 | from disk 0.0 GB | VRAM taken: off ?, on ?

| prompt | tokens | t/s off | t/s on | gain | gain range | kept | prompt t/s off | prompt t/s on | same text |
| ------ | -----: | ------: | -----: | ---: | ---------: | ---: | -------------: | ------------: | :-------: |
| code | 235 | 17.82 | 16.07 | 0.90x | 0.90-0.90 | 57% | 72.9 | 65.4 | no |
| chat | 161 | 17.95 | 12.44 | 0.69x | 0.69-0.69 | 36% | 77.0 | 70.1 | no |
| list | 181 | 18.04 | 22.06 | 1.22x | 1.22-1.22 | 90% | 76.6 | 69.5 | yes |
| long | 128 | 17.59 | 12.40 | 0.70x | 0.70-0.70 | 39% | 117.0 | 109.9 | no |
