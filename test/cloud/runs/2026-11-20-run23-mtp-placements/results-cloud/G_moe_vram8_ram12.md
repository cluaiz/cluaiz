qwen3.6-35b-a3b-mtp-gguf-UD-IQ4_XS | greedy, thinking off, 1 rounds of MTP off then on

MoE split (attention on GPU, experts in RAM) | ngl -1 | n_cpu_moe 41 | n_ctx 16384 | from disk 0.0 GB | VRAM taken: off 2.80 GB, on 3.25 GB

| prompt | tokens | t/s off | t/s on | gain | gain range | kept | prompt t/s off | prompt t/s on | same text |
| ------ | -----: | ------: | -----: | ---: | ---------: | ---: | -------------: | ------------: | :-------: |
| code | 235 | 13.05 | 9.40 | 0.72x | 0.72-0.72 | 87% | 14.2 | 8.4 | no |
| chat | 198 | 11.70 | 7.43 | 0.63x | 0.63-0.63 | 47% | 7.7 | 4.5 | no |
| list | 241 | 9.16 | 9.00 | 0.98x | 0.98-0.98 | 97% | 6.8 | 5.6 | yes |
| long | 128 | 9.46 | 8.59 | 0.91x | 0.91-0.91 | 61% | 23.3 | 18.0 | no |
